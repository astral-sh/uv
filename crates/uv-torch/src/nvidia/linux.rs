//! Conservative NVIDIA detection for native x86-64 glibc Linux.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use fs_err::{self as fs, File};
use goblin::container::{Container, Ctx, Endian};
use goblin::elf::{header, note::NoteDataIterator, program_header};
use goblin::elf64::{header::Header, program_header::SIZEOF_PHDR};
use serde::Deserialize;
use tracing::debug;
use uv_pep440::Version;

const MAX_CACHE: u64 = 16 * 1024 * 1024;
const MAX_NOTE: u64 = 16 * 1024;

#[derive(Deserialize)]
struct CudaNote {
    format: u64,
    #[serde(rename = "CUDA version")]
    cuda_version: String,
}

pub(crate) fn cuda_version() -> Option<Version> {
    // NVML requires a running NVIDIA driver, while a CUDA library can be installed on its own.
    // Preloading and secure execution can change loader semantics.
    if !Path::new("/proc/driver/nvidia/version").is_file()
        || ["LD_PRELOAD", "LD_AUDIT", "GLIBC_TUNABLES"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        return None;
    }
    let preload = Path::new("/etc/ld.so.preload");
    if preload.try_exists().ok()?
        && !read_limited(preload, MAX_NOTE)?
            .iter()
            .all(u8::is_ascii_whitespace)
    {
        return None;
    }
    if !unprivileged_process()? || is_wsl()? {
        return None;
    }
    let library_path = std::env::var_os("LD_LIBRARY_PATH");
    let library_path = match &library_path {
        Some(value) => Some(value.to_str()?),
        None => None,
    };
    let library = select_library(library_path, Path::new("/etc/ld.so.cache"))?;
    let version = read_cuda_note(&library)?;
    debug!(
        "Detected CUDA user-mode driver version {version} from {}",
        library.display()
    );
    Some(version)
}

fn is_wsl() -> Option<bool> {
    let release = read_limited(Path::new("/proc/sys/kernel/osrelease"), 1024)?;
    Some(
        String::from_utf8(release)
            .ok()?
            .to_ascii_lowercase()
            .contains("microsoft"),
    )
}

fn unprivileged_process() -> Option<bool> {
    for entry in read_limited(Path::new("/proc/self/auxv"), MAX_NOTE)?
        .as_chunks::<16>()
        .0
    {
        match read_u64(entry, 0)? {
            0 => break,
            23 => return Some(read_u64(entry, 8)? == 0),
            _ => {}
        }
    }
    None
}

/// Resolve the ordinary glibc search paths. A caller with its own RPATH or RUNPATH, or an already
/// loaded driver, can resolve differently. An unsupported candidate is a miss.
fn select_library(library_path: Option<&str>, cache: &Path) -> Option<PathBuf> {
    if let Some(library_path) = library_path {
        for directory in library_path.split(':') {
            if !Path::new(directory).is_absolute() || directory.contains(['$', ';']) {
                return None;
            }
            let directory = Path::new(directory);
            // glibc searches hardware-specific subdirectories before the plain directory.
            for subdirectory in ["glibc-hwcaps", "tls", "x86_64", "haswell", "avx512_1"] {
                if directory.join(subdirectory).try_exists().ok()? {
                    return None;
                }
            }
            let candidate = directory.join("libcuda.so.1");
            if candidate.try_exists().ok()? {
                return Some(candidate);
            }
        }
    }
    select_cache_entry(&read_limited(cache, MAX_CACHE)?)
}

/// Parse glibc's 1.1 cache layout. Multiple target-ABI or hardware-specific entries require the
/// loader's own ordering rules and are deliberately left to the fallback.
fn select_cache_entry(cache: &[u8]) -> Option<PathBuf> {
    let endian = *cache.get(28)? & 3;
    if cache.get(..20)? != b"glibc-ld.so.cache1.1" || (endian != 0 && endian != 2) {
        return None;
    }
    let count = usize::try_from(read_u32(cache, 20)?).ok()?;
    let table_end = 48usize.checked_add(count.checked_mul(24)?)?;
    let mut selected = None;
    for entry in cache.get(48..table_end)?.as_chunks::<24>().0 {
        if read_u32(entry, 0)? != 0x303 {
            continue;
        }
        let name = cache_string(cache, usize::try_from(read_u32(entry, 4)?).ok()?)?;
        if name != "libcuda.so.1" {
            continue;
        }
        if read_u64(entry, 16)? != 0 || selected.is_some() {
            return None;
        }
        let path = PathBuf::from(cache_string(
            cache,
            usize::try_from(read_u32(entry, 8)?).ok()?,
        )?);
        if !path.is_absolute() {
            return None;
        }
        selected = Some(path);
    }
    selected
}

fn cache_string(data: &[u8], offset: usize) -> Option<&str> {
    let data = data.get(offset..)?;
    let end = data.iter().position(|byte| *byte == 0)?;
    std::str::from_utf8(&data[..end]).ok()
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn read_limited(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= usize::try_from(limit).ok()?).then_some(bytes)
}

fn read_range(file: &mut File, offset: u64, size: u64, limit: u64) -> Option<Vec<u8>> {
    if size > limit || offset.checked_add(size)? > file.metadata().ok()?.len() {
        return None;
    }
    let mut bytes = vec![0; usize::try_from(size).ok()?];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// Read CUDA version notes from the ELF's program segments.
fn read_cuda_note(path: &Path) -> Option<Version> {
    let mut file = File::open(path).ok()?;
    let elf = Header::parse(&read_range(&mut file, 0, 64, 64)?).ok()?;
    if elf.e_ident[..7] != *b"\x7fELF\x02\x01\x01"
        || elf.e_type != header::ET_DYN
        || elf.e_machine != header::EM_X86_64
        || elf.e_version != 1
        || elf.e_ehsize != 64
        || elf.e_phnum == 0
        || elf.e_phoff < u64::from(elf.e_ehsize)
        || usize::from(elf.e_phentsize) != SIZEOF_PHDR
    {
        return None;
    }
    let context = Ctx::new(Container::Big, Endian::Little);
    let headers = read_range(
        &mut file,
        elf.e_phoff,
        u64::from(elf.e_phnum) * u64::from(elf.e_phentsize),
        MAX_NOTE,
    )?;
    let headers =
        program_header::ProgramHeader::parse(&headers, 0, usize::from(elf.e_phnum), context)
            .ok()?;
    let mut version = None;
    for segment in headers {
        if segment.p_type != program_header::PT_NOTE {
            continue;
        }
        let data = read_range(&mut file, segment.p_offset, segment.p_filesz, MAX_NOTE)?;
        let mut notes = NoteDataIterator {
            data: &data,
            size: data.len(),
            offset: 0,
            ctx: (usize::try_from(segment.p_align).ok()?, context),
        };
        for note in notes.by_ref() {
            let note = note.ok()?;
            if note.name != "NVIDIA Corporation" {
                continue;
            }
            if note.n_type != 0 || version.is_some() {
                return None;
            }
            let description = note.desc.strip_suffix(&[0]).unwrap_or(note.desc);
            let note: CudaNote = serde_json::from_slice(description).ok()?;
            let (major, minor) = note.cuda_version.split_once('.')?;
            if note.format != 1
                || [major, minor]
                    .into_iter()
                    .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return None;
            }
            version = Some(note.cuda_version.parse().ok()?);
        }
        if notes.offset != data.len() {
            return None;
        }
    }
    version
}

pub(crate) fn compute_capability() -> Option<(u8, u8)> {
    // Container runtimes can impose an additional device mask that is not represented by CUDA's
    // mask. The device nodes are checked below, but an explicit runtime mask needs NVML.
    if is_wsl()? {
        return None;
    }
    if let Some(visibility) = std::env::var_os("NVIDIA_VISIBLE_DEVICES")
        && visibility.to_str()? != "all"
    {
        return None;
    }
    let mig_minors = Path::new("/proc/driver/nvidia-caps/mig-minors");
    if mig_minors.try_exists().ok()?
        && read_limited(mig_minors, MAX_NOTE)?
            .windows(3)
            .any(|window| window == b"/gi")
    {
        return None;
    }
    probe_gpus(
        Path::new("/proc/driver/nvidia/gpus"),
        Path::new("/sys/bus/pci/devices"),
        Path::new("/dev"),
    )
}

fn probe_gpus(proc_gpus: &Path, sys_devices: &Path, dev: &Path) -> Option<(u8, u8)> {
    let mut maximum = None;
    for entry in fs::read_dir(proc_gpus).ok()? {
        let entry = entry.ok()?;
        let information =
            String::from_utf8(read_limited(&entry.path().join("information"), MAX_NOTE)?).ok()?;
        let mut minor = None;
        for (key, value) in information.lines().filter_map(|line| line.split_once(':')) {
            match key.trim() {
                "Device Minor" => minor = value.trim().parse::<u32>().ok(),
                "GPU Excluded" if value.trim() != "No" => return None,
                _ => {}
            }
        }
        // Opening the device also detects device-cgroup restrictions that stat cannot see.
        if File::open(dev.join(format!("nvidia{}", minor?))).is_err() {
            return None;
        }
        let pci = sys_devices.join(entry.file_name());
        if read_pci_id(&pci.join("vendor"))? != 0x10de {
            return None;
        }
        maximum = maximum.max(Some(gpu_capability(read_pci_id(&pci.join("device"))?)?));
    }
    maximum
}

fn read_pci_id(path: &Path) -> Option<u16> {
    let contents = String::from_utf8(read_limited(path, 64)?).ok()?;
    u16::from_str_radix(contents.trim().strip_prefix("0x")?, 16).ok()
}

/// NVIDIA's PCI IDs: <https://github.com/NVIDIA/open-gpu-kernel-modules#compatible-gpus>;
/// model compute capabilities: <https://developer.nvidia.com/cuda/gpus>.
fn gpu_capability(device: u16) -> Option<(u8, u8)> {
    match device {
        0x1eb8 => Some((7, 5)),                                     // Tesla T4
        0x20b0 | 0x20b2 | 0x20b5 | 0x20b7 | 0x20f1 => Some((8, 0)), // A100, A30
        0x2203 | 0x2204 | 0x2206 | 0x2208 | 0x2230 | 0x2231 | 0x2235 | 0x2236 | 0x2237 | 0x24b0
        | 0x25b6 => Some((8, 6)),
        0x2684 | 0x26b1 | 0x26b5 | 0x26b9 | 0x2704 | 0x2709 | 0x27b8 => Some((8, 9)),
        0x2330 | 0x2331 | 0x2335 | 0x2339 => Some((9, 0)),
        0x2901 => Some((10, 0)), // B200
        0x2b85 | 0x2c02 | 0x2c05 | 0x2c09 => Some((12, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::Path;

    use fs_err as fs;
    use uv_pep440::Version;

    use super::{probe_gpus, read_cuda_note, select_cache_entry, select_library};

    /// A minimal synthetic ELF containing the CUDA version note.
    fn library(path: &Path, version: &str) -> io::Result<()> {
        let description = format!("{{\"format\":1,\"CUDA version\":\"{version}\"}}\0");
        let mut elf = vec![0; 120];
        elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        elf[16] = 3; // ET_DYN
        elf[18] = 62; // EM_X86_64
        elf[20] = 1; // ELF version
        elf[32] = 64; // program-header offset
        elf[52] = 64; // ELF header size
        elf[54] = 56; // program-header size
        elf[56] = 1; // program-header count
        elf[64] = 4; // PT_NOTE
        elf[72] = 120; // note offset
        elf[112] = 4; // note alignment
        elf.extend_from_slice(&19u32.to_le_bytes());
        elf.extend_from_slice(
            &u32::try_from(description.len())
                .map_err(io::Error::other)?
                .to_le_bytes(),
        );
        elf.extend_from_slice(&0u32.to_le_bytes());
        elf.extend_from_slice(b"NVIDIA Corporation\0\0");
        elf.extend_from_slice(description.as_bytes());
        elf.resize(elf.len().next_multiple_of(4), 0);
        let note_size = u64::try_from(elf.len() - 120).map_err(io::Error::other)?;
        elf[96..104].copy_from_slice(&note_size.to_le_bytes());
        fs::write(path, elf)
    }

    fn cache(path: &Path) -> Vec<u8> {
        let mut data = vec![0; 48 + 2 * 24];
        data[..20].copy_from_slice(b"glibc-ld.so.cache1.1");
        data[20..24].copy_from_slice(&2u32.to_le_bytes());
        data[28] = 2;
        let name_offset = 96u32;
        data.extend_from_slice(b"libcuda.so.1\0");
        let path_offset = 109u32;
        data.extend_from_slice(path.to_string_lossy().as_bytes());
        data.push(0);
        // Ignore a 32-bit cache entry, then select the x86-64 glibc entry.
        data[48..52].copy_from_slice(&3u32.to_le_bytes());
        data[52..56].copy_from_slice(&name_offset.to_le_bytes());
        data[56..60].copy_from_slice(&path_offset.to_le_bytes());
        data[72..76].copy_from_slice(&0x303u32.to_le_bytes());
        data[76..80].copy_from_slice(&name_offset.to_le_bytes());
        data[80..84].copy_from_slice(&path_offset.to_le_bytes());
        data
    }

    /// A cuda-compat directory changes the selected user-mode driver.
    #[test]
    fn loader_selects_compat_library() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let system = temp.path().join("system");
        let compat = temp.path().join("cuda-12.8/compat");
        fs::create_dir_all(&system)?;
        fs::create_dir_all(&compat)?;
        let system_library = system.join("libcuda.so.1");
        let compat_library = compat.join("libcuda.so.1");
        library(&system_library, "12.4")?;
        library(&compat_library, "12.8")?;
        let cache_path = temp.path().join("ld.so.cache");
        fs::write(&cache_path, cache(&system_library))?;

        assert_eq!(
            read_cuda_note(&select_library(None, &cache_path).ok_or(io::ErrorKind::NotFound)?),
            Some(Version::new([12, 4]))
        );
        assert_eq!(
            read_cuda_note(
                &select_library(compat.to_str(), &cache_path).ok_or(io::ErrorKind::NotFound)?
            ),
            Some(Version::new([12, 8]))
        );
        assert_eq!(select_library(Some("relative"), &cache_path), None);
        fs::create_dir(compat.join("glibc-hwcaps"))?;
        assert_eq!(select_library(compat.to_str(), &cache_path), None);
        let mut ambiguous_cache = cache(&system_library);
        ambiguous_cache[48..52].copy_from_slice(&0x303u32.to_le_bytes());
        assert_eq!(select_cache_entry(&ambiguous_cache), None);
        Ok(())
    }

    #[test]
    fn gpu_capability_uses_known_devices() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let proc = temp.path().join("gpus");
        let sys = temp.path().join("pci");
        let dev = temp.path().join("dev");
        fs::create_dir_all(&dev)?;
        for (bus, minor, device) in [("0000:01:00.0", 0, "0x20b0"), ("0000:02:00.0", 1, "0x2330")] {
            fs::create_dir_all(proc.join(bus))?;
            fs::create_dir_all(sys.join(bus))?;
            fs::write(
                proc.join(bus).join("information"),
                format!("Device Minor: {minor}\nGPU Excluded: No\n"),
            )?;
            fs::write(sys.join(bus).join("vendor"), "0x10de\n")?;
            fs::write(sys.join(bus).join("device"), device)?;
            fs::write(dev.join(format!("nvidia{minor}")), "")?;
        }
        assert_eq!(probe_gpus(&proc, &sys, &dev), Some((9, 0)));
        fs::write(sys.join("0000:02:00.0/device"), "0xffff\n")?;
        assert_eq!(probe_gpus(&proc, &sys, &dev), None);
        Ok(())
    }
}
