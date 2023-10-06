//! Abstractions for understanding the current platform (operating system and architecture).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{fmt, fs, io};

use goblin::elf::Elf;
use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;
use tracing::trace;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Failed to detect the operating system version: {0}")]
    OsVersionDetectionError(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Platform {
    os: Os,
    arch: Arch,
}

impl Platform {
    pub fn current() -> Result<Self, PlatformError> {
        let os = Os::current()?;
        let arch = Arch::current()?;
        Ok(Self { os, arch })
    }

    pub fn is_windows(&self) -> bool {
        matches!(self.os, Os::Windows)
    }

    pub fn compatible_tags(
        &self,
        python_version: (u8, u8),
    ) -> Result<Vec<(String, String, String)>, PlatformError> {
        compatible_tags(python_version, &self.os, self.arch)
    }
}

/// All supported operating systems.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Os {
    Manylinux { major: u16, minor: u16 },
    Musllinux { major: u16, minor: u16 },
    Windows,
    Macos { major: u16, minor: u16 },
    FreeBsd { release: String },
    NetBsd { release: String },
    OpenBsd { release: String },
    Dragonfly { release: String },
    Illumos { release: String, arch: String },
    Haiku { release: String },
}

impl Os {
    pub fn current() -> Result<Self, PlatformError> {
        let target_triple = target_lexicon::HOST;

        let os = match target_triple.operating_system {
            target_lexicon::OperatingSystem::Linux => Self::detect_linux_libc()?,
            target_lexicon::OperatingSystem::Windows => Os::Windows,
            target_lexicon::OperatingSystem::MacOSX { major, minor, .. } => {
                Os::Macos { major, minor }
            }
            target_lexicon::OperatingSystem::Darwin => {
                let (major, minor) = get_mac_os_version()?;
                Os::Macos { major, minor }
            }
            target_lexicon::OperatingSystem::Netbsd => Os::NetBsd {
                release: Os::platform_info()?.release().to_string_lossy().to_string(),
            },
            target_lexicon::OperatingSystem::Freebsd => Os::FreeBsd {
                release: Os::platform_info()?.release().to_string_lossy().to_string(),
            },
            target_lexicon::OperatingSystem::Openbsd => Os::OpenBsd {
                release: Os::platform_info()?.release().to_string_lossy().to_string(),
            },
            target_lexicon::OperatingSystem::Dragonfly => Os::Dragonfly {
                release: Os::platform_info()?.release().to_string_lossy().to_string(),
            },
            target_lexicon::OperatingSystem::Illumos => {
                let platform_info = Os::platform_info()?;
                Os::Illumos {
                    release: platform_info.release().to_string_lossy().to_string(),
                    arch: platform_info.machine().to_string_lossy().to_string(),
                }
            }
            target_lexicon::OperatingSystem::Haiku => Os::Haiku {
                release: Os::platform_info()?.release().to_string_lossy().to_string(),
            },
            unsupported => {
                return Err(PlatformError::OsVersionDetectionError(format!(
                    "The operating system {unsupported:?} is not supported"
                )));
            }
        };
        Ok(os)
    }

    fn platform_info() -> Result<PlatformInfo, PlatformError> {
        PlatformInfo::new().map_err(|err| PlatformError::OsVersionDetectionError(err.to_string()))
    }

    fn detect_linux_libc() -> Result<Self, PlatformError> {
        let libc = find_libc()?;
        let linux = if let Ok(Some((major, minor))) = get_musl_version(&libc) {
            Os::Musllinux { major, minor }
        } else if let Ok(glibc_ld) = fs::read_link(&libc) {
            // Try reading the link first as it's faster
            let filename = glibc_ld
                .file_name()
                .ok_or_else(|| {
                    PlatformError::OsVersionDetectionError(
                        "Expected the glibc ld to be a file".to_string(),
                    )
                })?
                .to_string_lossy();
            let expr = Regex::new(r"ld-(\d{1,3})\.(\d{1,3})\.so").unwrap();

            if let Some(capture) = expr.captures(&filename) {
                let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
                let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
                Os::Manylinux { major, minor }
            } else {
                trace!("Couldn't use ld filename, using `ldd --version`");
                // runs `ldd --version`
                let version = glibc_version::get_version().map_err(|err| {
                    PlatformError::OsVersionDetectionError(format!(
                        "Failed to determine glibc version with `ldd --version`: {err}"
                    ))
                })?;
                Os::Manylinux {
                    major: u16::try_from(version.major).map_err(|_| {
                        PlatformError::OsVersionDetectionError(format!(
                            "Invalid glibc major version {}",
                            version.major
                        ))
                    })?,
                    minor: u16::try_from(version.minor).map_err(|_| {
                        PlatformError::OsVersionDetectionError(format!(
                            "Invalid glibc minor version {}",
                            version.minor
                        ))
                    })?,
                }
            }
        } else {
            return Err(PlatformError::OsVersionDetectionError("Couldn't detect neither glibc version nor musl libc version, at least one of which is required".to_string()));
        };
        trace!("libc: {}", linux);
        Ok(linux)
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Os::Manylinux { .. } => write!(f, "Manylinux"),
            Os::Musllinux { .. } => write!(f, "Musllinux"),
            Os::Windows => write!(f, "Windows"),
            Os::Macos { .. } => write!(f, "MacOS"),
            Os::FreeBsd { .. } => write!(f, "FreeBSD"),
            Os::NetBsd { .. } => write!(f, "NetBSD"),
            Os::OpenBsd { .. } => write!(f, "OpenBSD"),
            Os::Dragonfly { .. } => write!(f, "DragonFly"),
            Os::Illumos { .. } => write!(f, "Illumos"),
            Os::Haiku { .. } => write!(f, "Haiku"),
        }
    }
}

/// All supported CPU architectures
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Arch {
    Aarch64,
    Armv7L,
    Powerpc64Le,
    Powerpc64,
    X86,
    X86_64,
    S390X,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Arch::Aarch64 => write!(f, "aarch64"),
            Arch::Armv7L => write!(f, "armv7l"),
            Arch::Powerpc64Le => write!(f, "ppc64le"),
            Arch::Powerpc64 => write!(f, "ppc64"),
            Arch::X86 => write!(f, "i686"),
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::S390X => write!(f, "s390x"),
        }
    }
}

impl Arch {
    pub fn current() -> Result<Arch, PlatformError> {
        let target_triple = target_lexicon::HOST;
        let arch = match target_triple.architecture {
            target_lexicon::Architecture::X86_64 => Arch::X86_64,
            target_lexicon::Architecture::X86_32(_) => Arch::X86,
            target_lexicon::Architecture::Arm(_) => Arch::Armv7L,
            target_lexicon::Architecture::Aarch64(_) => Arch::Aarch64,
            target_lexicon::Architecture::Powerpc64 => Arch::Powerpc64,
            target_lexicon::Architecture::Powerpc64le => Arch::Powerpc64Le,
            target_lexicon::Architecture::S390x => Arch::S390X,
            unsupported => {
                return Err(PlatformError::OsVersionDetectionError(format!(
                    "The architecture {unsupported} is not supported"
                )));
            }
        };
        Ok(arch)
    }

    /// Returns the oldest possible Manylinux tag for this architecture
    pub fn get_minimum_manylinux_minor(&self) -> u16 {
        match self {
            // manylinux 2014
            Arch::Aarch64 | Arch::Armv7L | Arch::Powerpc64 | Arch::Powerpc64Le | Arch::S390X => 17,
            // manylinux 1
            Arch::X86 | Arch::X86_64 => 5,
        }
    }
}

fn get_mac_os_version() -> Result<(u16, u16), PlatformError> {
    // This is actually what python does
    // https://github.com/python/cpython/blob/cb2b3c8d3566ae46b3b8d0718019e1c98484589e/Lib/platform.py#L409-L428
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct SystemVersion {
        product_version: String,
    }
    let system_version: SystemVersion =
        plist::from_file("/System/Library/CoreServices/SystemVersion.plist")
            .map_err(|err| PlatformError::OsVersionDetectionError(err.to_string()))?;

    let invalid_mac_os_version = || {
        PlatformError::OsVersionDetectionError(format!(
            "Invalid macOS version {}",
            system_version.product_version
        ))
    };
    match system_version
        .product_version
        .split('.')
        .collect::<Vec<&str>>()
        .as_slice()
    {
        [major, minor] | [major, minor, _] => {
            let major = major.parse::<u16>().map_err(|_| invalid_mac_os_version())?;
            let minor = minor.parse::<u16>().map_err(|_| invalid_mac_os_version())?;
            Ok((major, minor))
        }
        _ => Err(invalid_mac_os_version()),
    }
}

/// Determine the appropriate binary formats for a macOS version.
/// Source: <https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L314>
fn get_mac_binary_formats(major: u16, minor: u16, arch: Arch) -> Vec<String> {
    let mut formats = vec![match arch {
        Arch::Aarch64 => "arm64".to_string(),
        _ => arch.to_string(),
    }];

    if matches!(arch, Arch::X86_64) {
        if (major, minor) < (10, 4) {
            return vec![];
        }
        formats.extend([
            "intel".to_string(),
            "fat64".to_string(),
            "fat32".to_string(),
        ]);
    }

    if matches!(arch, Arch::X86_64 | Arch::Aarch64) {
        formats.push("universal2".to_string());
    }

    if matches!(arch, Arch::X86_64) {
        formats.push("universal".to_string());
    }

    formats
}

/// Find musl libc path from executable's ELF header
fn find_libc() -> Result<PathBuf, PlatformError> {
    let buffer = fs::read("/bin/ls")?;
    let error_str = "Couldn't parse /bin/ls for detecting the ld version";
    let elf = Elf::parse(&buffer)
        .map_err(|err| PlatformError::OsVersionDetectionError(format!("{error_str}: {err}")))?;
    if let Some(elf_interpreter) = elf.interpreter {
        Ok(PathBuf::from(elf_interpreter))
    } else {
        Err(PlatformError::OsVersionDetectionError(
            error_str.to_string(),
        ))
    }
}

/// Read the musl version from libc library's output. Taken from maturin
///
/// The libc library should output something like this to `stderr::`
///
/// musl libc (`x86_64`)
/// Version 1.2.2
/// Dynamic Program Loader
fn get_musl_version(ld_path: impl AsRef<Path>) -> std::io::Result<Option<(u16, u16)>> {
    let output = Command::new(ld_path.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expr = Regex::new(r"Version (\d{2,4})\.(\d{2,4})").unwrap();
    if let Some(capture) = expr.captures(&stderr) {
        let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
        let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
        return Ok(Some((major, minor)));
    }
    Ok(None)
}

/// Returns the compatible platform tags, e.g. `manylinux_2_17`, `macosx_11_0_arm64` or `win_amd64`
///
/// We have two cases: Actual platform specific tags (including "merged" tags such as universal2)
/// and "any".
///
/// Bit of a mess, needs to be cleaned up
pub fn compatible_platform_tags(os: &Os, arch: Arch) -> Result<Vec<String>, PlatformError> {
    let platform_tags = match (&os, arch) {
        (Os::Manylinux { major, minor }, _) => {
            let mut platform_tags = vec![format!("linux_{}", arch)];
            platform_tags.extend(
                (arch.get_minimum_manylinux_minor()..=*minor)
                    .map(|minor| format!("manylinux_{major}_{minor}_{arch}")),
            );
            if (arch.get_minimum_manylinux_minor()..=*minor).contains(&12) {
                platform_tags.push(format!("manylinux2010_{arch}"));
            }
            if (arch.get_minimum_manylinux_minor()..=*minor).contains(&17) {
                platform_tags.push(format!("manylinux2014_{arch}"));
            }
            if (arch.get_minimum_manylinux_minor()..=*minor).contains(&5) {
                platform_tags.push(format!("manylinux1_{arch}"));
            }
            platform_tags
        }
        (Os::Musllinux { major, minor }, _) => {
            let mut platform_tags = vec![format!("linux_{}", arch)];
            // musl 1.1 is the lowest supported version in musllinux
            platform_tags
                .extend((1..=*minor).map(|minor| format!("musllinux_{major}_{minor}_{arch}")));
            platform_tags
        }
        (Os::Macos { major, minor }, Arch::X86_64) => {
            // Source: https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L346
            let mut platform_tags = vec![];
            match major {
                10 => {
                    // Prior to Mac OS 11, each yearly release of Mac OS bumped the "minor" version
                    // number. The major version was always 10.
                    for minor in (0..=*minor).rev() {
                        for binary_format in get_mac_binary_formats(*major, minor, arch) {
                            platform_tags.push(format!("macosx_{major}_{minor}_{binary_format}"));
                        }
                    }
                }
                value if *value >= 11 => {
                    // Starting with Mac OS 11, each yearly release bumps the major version number.
                    // The minor versions are now the midyear updates.
                    for major in (10..=*major).rev() {
                        for binary_format in get_mac_binary_formats(major, 0, arch) {
                            platform_tags.push(format!("macosx_{}_{}_{}", major, 0, binary_format));
                        }
                    }
                    // The "universal2" binary format can have a macOS version earlier than 11.0
                    // when the x86_64 part of the binary supports that version of macOS.
                    for minor in (4..=16).rev() {
                        for binary_format in get_mac_binary_formats(10, minor, arch) {
                            platform_tags
                                .push(format!("macosx_{}_{}_{}", 10, minor, binary_format));
                        }
                    }
                }
                _ => {
                    return Err(PlatformError::OsVersionDetectionError(format!(
                        "Unsupported macOS version: {major}",
                    )));
                }
            }
            platform_tags
        }
        (Os::Macos { major, .. }, Arch::Aarch64) => {
            // Source: https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L346
            let mut platform_tags = vec![];
            // Starting with Mac OS 11, each yearly release bumps the major version number.
            // The minor versions are now the midyear updates.
            for major in (10..=*major).rev() {
                for binary_format in get_mac_binary_formats(major, 0, arch) {
                    platform_tags.push(format!("macosx_{}_{}_{}", major, 0, binary_format));
                }
            }
            // The "universal2" binary format can have a macOS version earlier than 11.0
            // when the x86_64 part of the binary supports that version of macOS.
            platform_tags.extend(
                (4..=16)
                    .rev()
                    .map(|minor| format!("macosx_{}_{}_universal2", 10, minor)),
            );
            platform_tags
        }
        (Os::Windows, Arch::X86) => {
            vec!["win32".to_string()]
        }
        (Os::Windows, Arch::X86_64) => {
            vec!["win_amd64".to_string()]
        }
        (Os::Windows, Arch::Aarch64) => vec!["win_arm64".to_string()],
        (
            Os::FreeBsd { release }
            | Os::NetBsd { release }
            | Os::OpenBsd { release }
            | Os::Dragonfly { release }
            | Os::Haiku { release },
            _,
        ) => {
            let release = release.replace(['.', '-'], "_");
            vec![format!(
                "{}_{}_{}",
                os.to_string().to_lowercase(),
                release,
                arch
            )]
        }
        (Os::Illumos { release, arch }, _) => {
            // See https://github.com/python/cpython/blob/46c8d915715aa2bd4d697482aa051fe974d440e1/Lib/sysconfig.py#L722-L730
            if let Some((major, other)) = release.split_once('_') {
                let major_ver: u64 = major.parse().map_err(|err| {
                    PlatformError::OsVersionDetectionError(format!(
                        "illumos major version is not a number: {err}"
                    ))
                })?;
                if major_ver >= 5 {
                    // SunOS 5 == Solaris 2
                    let os = "solaris".to_string();
                    let release = format!("{}_{}", major_ver - 3, other);
                    let arch = format!("{arch}_64bit");
                    return Ok(vec![format!("{}_{}_{}", os, release, arch)]);
                }
            }

            let os = os.to_string().to_lowercase();
            vec![format!("{}_{}_{}", os, release, arch)]
        }
        _ => {
            return Err(PlatformError::OsVersionDetectionError(format!(
                "Unsupported operating system and architecture combination: {os} {arch}"
            )));
        }
    };
    Ok(platform_tags)
}

/// Returns the compatible tags in a (`python_tag`, `abi_tag`, `platform_tag`) format
pub fn compatible_tags(
    python_version: (u8, u8),
    os: &Os,
    arch: Arch,
) -> Result<Vec<(String, String, String)>, PlatformError> {
    assert_eq!(python_version.0, 3);
    let mut tags = Vec::new();
    let platform_tags = compatible_platform_tags(os, arch)?;
    // 1. This exact c api version
    for platform_tag in &platform_tags {
        tags.push((
            format!("cp{}{}", python_version.0, python_version.1),
            format!(
                "cp{}{}{}",
                python_version.0,
                python_version.1,
                // hacky but that's legacy anyways
                if python_version.1 <= 7 { "m" } else { "" }
            ),
            platform_tag.clone(),
        ));
        tags.push((
            format!("cp{}{}", python_version.0, python_version.1),
            "none".to_string(),
            platform_tag.clone(),
        ));
    }
    // 2. abi3 and no abi (e.g. executable binary)
    // For some reason 3.2 is the minimum python for the cp abi
    for minor in 2..=python_version.1 {
        for platform_tag in &platform_tags {
            tags.push((
                format!("cp{}{}", python_version.0, minor),
                "abi3".to_string(),
                platform_tag.clone(),
            ));
        }
    }
    // 3. no abi (e.g. executable binary)
    for minor in 0..=python_version.1 {
        for platform_tag in &platform_tags {
            tags.push((
                format!("py{}{}", python_version.0, minor),
                "none".to_string(),
                platform_tag.clone(),
            ));
        }
    }
    // 4. major only
    for platform_tag in platform_tags {
        tags.push((
            format!("py{}", python_version.0),
            "none".to_string(),
            platform_tag,
        ));
    }
    // 5. no binary
    for minor in 0..=python_version.1 {
        tags.push((
            format!("py{}{}", python_version.0, minor),
            "none".to_string(),
            "any".to_string(),
        ));
    }
    tags.push((
        format!("py{}", python_version.0),
        "none".to_string(),
        "any".to_string(),
    ));
    tags.sort();
    Ok(tags)
}
