use std::io;
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::ffi::CString;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::os::fd::AsRawFd;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(target_os = "linux")]
use linux_raw_sys::ioctl::{
    FIEMAP_EXTENT_DATA_INLINE, FIEMAP_EXTENT_DELALLOC, FIEMAP_EXTENT_ENCODED, FIEMAP_EXTENT_LAST,
    FIEMAP_EXTENT_NOT_ALIGNED, FIEMAP_EXTENT_SHARED, FIEMAP_EXTENT_UNKNOWN, FS_IOC_FIEMAP,
};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
use rustc_hash::{FxHashMap, FxHashSet};

/// A filesystem and the allocation-block size used for its physical extents.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct Filesystem {
    device: u64,
    block_size: u64,
}

/// A physical file extent before it has been aligned to filesystem blocks.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
struct FileExtent {
    start: u64,
    length: u64,
}

/// A physical extent aligned to its filesystem's allocation blocks.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
#[derive(Clone, Copy)]
struct PhysicalExtent {
    start: u64,
    end: u64,
}

/// Return whether the current platform can identify individual files' physical storage.
pub const fn supports_physical_space() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios"
    ))
}

/// Return the physical file data and directory storage referenced by `path`.
///
/// Hardlinks are counted once by their device and inode, while copy-on-write clones are counted
/// once by the physical filesystem blocks they reference. Unlike [`physical_space`], shared data
/// referenced outside `path` is still attributed to this tree.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
pub fn physical_disk_usage(path: &Path) -> io::Result<u64> {
    let mut seen_files = FxHashSet::default();
    let mut filesystem_block_sizes = FxHashMap::default();
    let mut physical_extents: FxHashMap<Filesystem, Vec<PhysicalExtent>> = FxHashMap::default();
    let mut untracked_bytes = 0_u64;

    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        let metadata = fs_err::symlink_metadata(entry.path())?;
        let allocated_bytes = metadata.blocks().saturating_mul(512);

        if !metadata.is_file() {
            untracked_bytes = untracked_bytes.saturating_add(allocated_bytes);
            continue;
        }

        if !seen_files.insert((metadata.dev(), metadata.ino())) {
            continue;
        }

        if allocated_bytes == 0 {
            continue;
        }

        let file = fs_err::File::open(entry.path())?;
        let block_size = if let Some(&block_size) = filesystem_block_sizes.get(&metadata.dev()) {
            block_size
        } else {
            let filesystem = rustix::fs::fstatvfs(&file)?;
            let block_size = if filesystem.f_frsize == 0 {
                filesystem.f_bsize
            } else {
                filesystem.f_frsize
            };
            filesystem_block_sizes.insert(metadata.dev(), block_size);
            block_size
        };
        if block_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem reported a zero-byte allocation block",
            ));
        }

        match file_physical_extents(&file, &metadata) {
            Ok(extents) => {
                let device_extents = physical_extents
                    .entry(Filesystem {
                        device: metadata.dev(),
                        block_size,
                    })
                    .or_default();
                for FileExtent { start, length } in extents {
                    let end = start.checked_add(length).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "a physical extent exceeds the filesystem address space",
                        )
                    })?;
                    let aligned_start = (start / block_size).saturating_mul(block_size);
                    let aligned_end =
                        end.checked_next_multiple_of(block_size).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "an aligned physical extent exceeds the filesystem address space",
                            )
                        })?;
                    device_extents.push(PhysicalExtent {
                        start: aligned_start,
                        end: aligned_end,
                    });
                }
            }
            Err(error) => {
                tracing::debug!(
                    "Failed to map physical storage for {}: {error}",
                    entry.path().display()
                );
                untracked_bytes = untracked_bytes.saturating_add(allocated_bytes);
            }
        }
    }

    for extents in physical_extents.values_mut() {
        extents.sort_unstable_by_key(|extent| extent.start);
        let mut current: Option<PhysicalExtent> = None;

        for &extent in extents.iter() {
            if let Some(current_extent) = current.as_mut() {
                if extent.start <= current_extent.end {
                    current_extent.end = current_extent.end.max(extent.end);
                } else {
                    untracked_bytes = untracked_bytes
                        .saturating_add(current_extent.end.saturating_sub(current_extent.start));
                    *current_extent = extent;
                }
            } else {
                current = Some(extent);
            }
        }

        if let Some(extent) = current {
            untracked_bytes =
                untracked_bytes.saturating_add(extent.end.saturating_sub(extent.start));
        }
    }

    Ok(untracked_bytes)
}

/// Return the physical file data and directory storage referenced by `path`.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
pub fn physical_disk_usage(_path: &Path) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "physical disk usage is unsupported on this platform",
    ))
}

/// Return the physical file data that would be reclaimed by deleting `path`.
///
/// The result excludes data retained by another hardlink, copy-on-write clone, or snapshot.
/// Filesystem metadata is not included.
pub fn physical_space(path: &Path, metadata: &std::fs::Metadata) -> io::Result<u64> {
    if !metadata.is_file() {
        #[cfg(unix)]
        {
            return Ok(metadata.blocks().saturating_mul(512));
        }

        #[cfg(not(unix))]
        {
            return Ok(0);
        }
    }

    #[cfg(unix)]
    if metadata.nlink() > 1 {
        return Ok(0);
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        apple_physical_space(path)
    }

    #[cfg(target_os = "linux")]
    {
        linux_physical_space(path)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "per-file space measurement is unsupported on this platform",
        ))
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[expect(unsafe_code)]
fn file_physical_extents(
    file: &fs_err::File,
    metadata: &std::fs::Metadata,
) -> io::Result<Vec<FileExtent>> {
    let mut extents = Vec::new();
    let mut offset = 0_u64;

    while offset < metadata.len() {
        let start = match rustix::fs::seek(file, rustix::fs::SeekFrom::Data(offset)) {
            Ok(start) => start,
            Err(rustix::io::Errno::NXIO) => break,
            Err(error) => return Err(error.into()),
        };
        let end = rustix::fs::seek(file, rustix::fs::SeekFrom::Hole(start))?.min(metadata.len());
        if end <= start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned a non-advancing data extent",
            ));
        }

        let mut extent_offset = start;
        while extent_offset < end {
            let mut request = libc::log2phys {
                l2p_flags: 0,
                l2p_contigbytes: i64::try_from(end - extent_offset).map_err(io::Error::other)?,
                l2p_devoffset: i64::try_from(extent_offset).map_err(io::Error::other)?,
            };

            // SAFETY: `file` is an open readable file, and `request` is a valid, initialized
            // `log2phys` buffer that remains live for the duration of the `fcntl` call.
            let result =
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_LOG2PHYS_EXT, &raw mut request) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }

            let contiguous = u64::try_from(request.l2p_contigbytes).map_err(io::Error::other)?;
            let physical = u64::try_from(request.l2p_devoffset).map_err(io::Error::other)?;
            if contiguous == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the filesystem returned a zero-length physical extent",
                ));
            }

            let length = contiguous.min(end - extent_offset);
            extents.push(FileExtent {
                start: physical,
                length,
            });
            extent_offset = extent_offset.saturating_add(length);
        }

        offset = end;
    }

    Ok(extents)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[expect(unsafe_code)]
fn apple_physical_space(path: &Path) -> io::Result<u64> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: libc::ATTR_CMNEXT_PRIVATESIZE,
    };
    let mut response = [0_u8; 32];

    // SAFETY: `path` is null-terminated and valid for the duration of the call. `attributes` is a
    // valid attribute request, and `response` has enough space for the length, returned attribute
    // set, and requested private-size value.
    let result = unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&raw mut attributes).cast(),
            response.as_mut_ptr().cast(),
            response.len(),
            libc::FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    let returned_fork_attributes =
        u32::from_ne_bytes(response[20..24].try_into().map_err(io::Error::other)?);
    if returned_fork_attributes & libc::ATTR_CMNEXT_PRIVATESIZE == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the filesystem does not report private file sizes",
        ));
    }

    let private_size = i64::from_ne_bytes(response[24..32].try_into().map_err(io::Error::other)?);
    u64::try_from(private_size).map_err(io::Error::other)
}

#[cfg(target_os = "linux")]
fn linux_physical_space(path: &Path) -> io::Result<u64> {
    let file = fs_err::File::open(path)?;
    let mut physical = 0_u64;

    linux_file_extents(&file, |extent| {
        if extent.flags
            & (FIEMAP_EXTENT_DELALLOC | FIEMAP_EXTENT_DATA_INLINE | FIEMAP_EXTENT_SHARED)
            != 0
        {
            return Ok(());
        }

        if extent.flags
            & (FIEMAP_EXTENT_UNKNOWN | FIEMAP_EXTENT_ENCODED | FIEMAP_EXTENT_NOT_ALIGNED)
            != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the filesystem cannot report the physical size of an extent",
            ));
        }

        physical = physical.saturating_add(extent.length);
        Ok(())
    })?;

    Ok(physical)
}

#[cfg(target_os = "linux")]
fn file_physical_extents(
    file: &fs_err::File,
    _metadata: &std::fs::Metadata,
) -> io::Result<Vec<FileExtent>> {
    let mut extents = Vec::new();

    linux_file_extents(file, |extent| {
        if extent.flags & (FIEMAP_EXTENT_DELALLOC | FIEMAP_EXTENT_DATA_INLINE) != 0 {
            return Ok(());
        }

        if extent.flags
            & (FIEMAP_EXTENT_UNKNOWN | FIEMAP_EXTENT_ENCODED | FIEMAP_EXTENT_NOT_ALIGNED)
            != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the filesystem cannot report the physical location of an extent",
            ));
        }

        extents.push(FileExtent {
            start: extent.physical,
            length: extent.length,
        });
        Ok(())
    })?;

    Ok(extents)
}

#[cfg(target_os = "linux")]
#[derive(Default)]
#[repr(C)]
struct Fiemap {
    start: u64,
    length: u64,
    flags: u32,
    mapped_extents: u32,
    extent_count: u32,
    reserved: u32,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct FiemapExtent {
    logical: u64,
    physical: u64,
    length: u64,
    reserved64: [u64; 2],
    flags: u32,
    reserved: [u32; 3],
}

#[cfg(target_os = "linux")]
const MAX_EXTENTS: usize = 32;

#[cfg(target_os = "linux")]
#[derive(Default)]
#[repr(C)]
struct FiemapBuffer {
    header: Fiemap,
    extents: [FiemapExtent; MAX_EXTENTS],
}

#[cfg(target_os = "linux")]
#[expect(unsafe_code)]
fn linux_file_extents(
    file: &fs_err::File,
    mut on_extent: impl FnMut(&FiemapExtent) -> io::Result<()>,
) -> io::Result<()> {
    let mut start = 0_u64;

    loop {
        let mut request = FiemapBuffer {
            header: Fiemap {
                start,
                length: u64::MAX.saturating_sub(start),
                extent_count: u32::try_from(MAX_EXTENTS).map_err(io::Error::other)?,
                ..Fiemap::default()
            },
            ..FiemapBuffer::default()
        };

        // SAFETY: `FS_IOC_FIEMAP` is the Linux fiemap ioctl opcode, and `request` begins with the
        // expected fiemap header followed by enough initialized storage for all requested extents.
        unsafe {
            rustix::ioctl::ioctl(
                file,
                rustix::ioctl::Updater::<{ FS_IOC_FIEMAP as rustix::ioctl::Opcode }, _>::new(
                    &mut request,
                ),
            )?;
        }

        let mapped_extents =
            usize::try_from(request.header.mapped_extents).map_err(io::Error::other)?;
        if mapped_extents > request.extents.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned more extents than requested",
            ));
        }
        if mapped_extents == 0 {
            return Ok(());
        }

        for extent in &request.extents[..mapped_extents] {
            on_extent(extent)?;
        }

        let last_extent = &request.extents[mapped_extents - 1];
        if last_extent.flags & FIEMAP_EXTENT_LAST != 0 {
            return Ok(());
        }

        let next = last_extent.logical.saturating_add(last_extent.length);
        if next <= start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned a non-advancing extent",
            ));
        }
        start = next;
    }
}
