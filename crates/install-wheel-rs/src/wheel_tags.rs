//! Parses the wheel filename, the current host os/arch and checks wheels for compatibility

use crate::Error;
use fs_err as fs;
use goblin::elf::Elf;
use once_cell::sync::Lazy;
use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use regex::Regex;
use serde::Deserialize;
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use tracing::trace;

/// The name of a wheel split into its parts ([PEP 491](https://peps.python.org/pep-0491/))
///
/// Ignores the build tag atm.
///
/// ```
/// use std::str::FromStr;
/// use install_wheel_rs::WheelFilename;
///
/// let filename = WheelFilename::from_str("foo-1.0-py32-none-any.whl").unwrap();
/// assert_eq!(filename, WheelFilename {
///     distribution: "foo".to_string(),
///     version: "1.0".to_string(),
///     python_tag: vec!["py32".to_string()],
///     abi_tag: vec!["none".to_string()],
///     platform_tag: vec!["any".to_string()]
/// });
/// let filename = WheelFilename::from_str(
///     "numpy-1.26.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl"
/// ).unwrap();
/// assert_eq!(filename, WheelFilename {
///     distribution: "numpy".to_string(),
///     version: "1.26.0".to_string(),
///     python_tag: vec!["cp312".to_string()],
///     abi_tag: vec!["cp312".to_string()],
///     platform_tag: vec![
///         "manylinux_2_17_aarch64".to_string(),
///         "manylinux2014_aarch64".to_string()
///     ]
/// });
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WheelFilename {
    pub distribution: String,
    pub version: String,
    pub python_tag: Vec<String>,
    pub abi_tag: Vec<String>,
    pub platform_tag: Vec<String>,
}

impl FromStr for WheelFilename {
    type Err = Error;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let basename = filename.strip_suffix(".whl").ok_or_else(|| {
            Error::InvalidWheelFileName(filename.to_string(), "Must end with .whl".to_string())
        })?;
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        match basename.split('-').collect::<Vec<_>>().as_slice() {
            // TODO: Build tag precedence
            &[distribution, version, _, python_tag, abi_tag, platform_tag]
            | &[distribution, version, python_tag, abi_tag, platform_tag] => Ok(WheelFilename {
                distribution: distribution.to_string(),
                version: version.to_string(),
                python_tag: python_tag.split('.').map(String::from).collect(),
                abi_tag: abi_tag.split('.').map(String::from).collect(),
                platform_tag: platform_tag.split('.').map(String::from).collect(),
            }),
            _ => Err(Error::InvalidWheelFileName(
                filename.to_string(),
                "Expected four or five dashes (\"-\") in the filename".to_string(),
            )),
        }
    }
}

impl WheelFilename {
    /// Returns Some(precedence) is the wheels are compatible, otherwise none
    ///
    /// Precedence is e.g. used to install newer manylinux wheels over older manylinux wheels
    pub fn compatibility(&self, compatible_tags: &CompatibleTags) -> Result<usize, Error> {
        compatible_tags
            .iter()
            .enumerate()
            .filter_map(|(precedence, tag)| {
                if self.python_tag.contains(&tag.0)
                    && self.abi_tag.contains(&tag.1)
                    && self.platform_tag.contains(&tag.2)
                {
                    Some(precedence)
                } else {
                    None
                }
            })
            .next()
            .ok_or_else(|| Error::IncompatibleWheel {
                os: compatible_tags.os.clone(),
                arch: compatible_tags.arch,
            })
    }

    /// Effectively undoes the wheel filename parsing step
    pub fn get_tag(&self) -> String {
        format!(
            "{}-{}-{}",
            self.python_tag.join("."),
            self.abi_tag.join("."),
            self.platform_tag.join(".")
        )
    }
}

/// A platform, defined by the list of compatible wheel tags in order
pub struct CompatibleTags {
    pub os: Os,
    pub arch: Arch,
    pub tags: Vec<(String, String, String)>,
}

impl Deref for CompatibleTags {
    type Target = [(String, String, String)];

    fn deref(&self) -> &Self::Target {
        &self.tags
    }
}

/// Returns the compatible tags in a (python_tag, abi_tag, platform_tag) format, ordered from
/// highest precedence to lowest precedence
impl CompatibleTags {
    /// Compatible tags for the current operating system and architecture
    pub fn current(python_version: (u8, u8)) -> Result<CompatibleTags, Error> {
        Self::new(python_version, Os::current()?, Arch::current()?)
    }

    pub fn new(python_version: (u8, u8), os: Os, arch: Arch) -> Result<CompatibleTags, Error> {
        assert_eq!(python_version.0, 3);
        let mut tags = Vec::new();
        let platform_tags = compatible_platform_tags(&os, &arch)?;
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
        Ok(CompatibleTags { os, arch, tags })
    }
}

/// All supported operating system
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
    fn detect_linux_libc() -> Result<Self, Error> {
        let libc = find_libc()?;
        let linux = if let Ok(Some((major, minor))) = get_musl_version(&libc) {
            Os::Musllinux { major, minor }
        } else if let Ok(glibc_ld) = fs::read_link(&libc) {
            // Try reading the link first as it's faster
            let filename = glibc_ld
                .file_name()
                .ok_or_else(|| {
                    Error::OsVersionDetection("Expected the glibc ld to be a file".to_string())
                })?
                .to_string_lossy();
            #[allow(non_upper_case_globals)]
            static expr: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"ld-(\d{1,3})\.(\d{1,3})\.so").unwrap());

            if let Some(capture) = expr.captures(&filename) {
                let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
                let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
                Os::Manylinux { major, minor }
            } else {
                trace!("Couldn't use ld filename, using `ldd --version`");
                // runs `ldd --version`
                let version = glibc_version::get_version().map_err(|err| {
                    Error::OsVersionDetection(format!(
                        "Failed to determine glibc version with `ldd --version`: {}",
                        err
                    ))
                })?;
                Os::Manylinux {
                    major: version.major as u16,
                    minor: version.minor as u16,
                }
            }
        } else {
            return Err(Error::OsVersionDetection("Couldn't detect neither glibc version nor musl libc version, at least one of which is required".to_string()));
        };
        trace!("libc: {}", linux);
        Ok(linux)
    }

    pub fn current() -> Result<Self, Error> {
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
                release: PlatformInfo::new()
                    .map_err(Error::PlatformInfo)?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Freebsd => Os::FreeBsd {
                release: PlatformInfo::new()
                    .map_err(Error::PlatformInfo)?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Openbsd => Os::OpenBsd {
                release: PlatformInfo::new()
                    .map_err(Error::PlatformInfo)?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Dragonfly => Os::Dragonfly {
                release: PlatformInfo::new()
                    .map_err(Error::PlatformInfo)?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Illumos => {
                let platform_info = PlatformInfo::new().map_err(Error::PlatformInfo)?;
                Os::Illumos {
                    release: platform_info.release().to_string_lossy().to_string(),
                    arch: platform_info.machine().to_string_lossy().to_string(),
                }
            }
            target_lexicon::OperatingSystem::Haiku => Os::Haiku {
                release: PlatformInfo::new()
                    .map_err(Error::PlatformInfo)?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            unsupported => {
                return Err(Error::OsVersionDetection(format!(
                    "The operating system {:?} is not supported",
                    unsupported
                )))
            }
        };
        Ok(os)
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
    pub fn current() -> Result<Arch, Error> {
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
                return Err(Error::OsVersionDetection(format!(
                    "The architecture {} is not supported",
                    unsupported
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

fn get_mac_os_version() -> Result<(u16, u16), Error> {
    // This is actually what python does
    // https://github.com/python/cpython/blob/cb2b3c8d3566ae46b3b8d0718019e1c98484589e/Lib/platform.py#L409-L428
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct SystemVersion {
        product_version: String,
    }
    let system_version: SystemVersion =
        plist::from_file("/System/Library/CoreServices/SystemVersion.plist")
            .map_err(|err| Error::OsVersionDetection(err.to_string()))?;

    let invalid_mac_os_version = || {
        Error::OsVersionDetection(format!(
            "Invalid mac os version {}",
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

/// Determine the appropriate binary formats for a mac os version.
/// Source: https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L314
fn get_mac_binary_formats(major: u16, minor: u16, arch: &Arch) -> Vec<String> {
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
pub fn find_libc() -> Result<PathBuf, Error> {
    let buffer = fs::read("/bin/ls")?;
    let error_str = "Couldn't parse /bin/ls for detecting the ld version";
    let elf = Elf::parse(&buffer)
        .map_err(|err| Error::OsVersionDetection(format!("{}: {}", error_str, err)))?;
    if let Some(elf_interpreter) = elf.interpreter {
        Ok(PathBuf::from(elf_interpreter))
    } else {
        Err(Error::OsVersionDetection(error_str.to_string()))
    }
}

/// Read the musl version from libc library's output. Taken from maturin
///
/// The libc library should output something like this to stderr::
///
/// musl libc (x86_64)
/// Version 1.2.2
/// Dynamic Program Loader
pub fn get_musl_version(ld_path: impl AsRef<Path>) -> std::io::Result<Option<(u16, u16)>> {
    let output = Command::new(ld_path.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    #[allow(non_upper_case_globals)]
    static expr: Lazy<Regex> = Lazy::new(|| Regex::new(r"Version (\d{2,4})\.(\d{2,4})").unwrap());
    if let Some(capture) = expr.captures(&stderr) {
        let major = capture.get(1).unwrap().as_str().parse::<u16>().unwrap();
        let minor = capture.get(2).unwrap().as_str().parse::<u16>().unwrap();
        return Ok(Some((major, minor)));
    }
    Ok(None)
}

/// Returns the compatible platform tags from highest precedence to lowest precedence
///
/// Examples: manylinux_2_17, macosx_11_0_arm64, win_amd64
///
/// We have two cases: Actual platform specific tags (including "merged" tags such as universal2)
/// and "any".
///
/// Bit of a mess, needs to be cleaned up. The order also isn't exactly matching that of pip yet,
/// but works good enough in practice
pub fn compatible_platform_tags(os: &Os, arch: &Arch) -> Result<Vec<String>, Error> {
    let platform_tags = match (os.clone(), *arch) {
        (Os::Manylinux { major, minor }, _) => {
            let mut platform_tags = vec![format!("linux_{}", arch)];
            // Use newer manylinux first like pip does
            platform_tags.extend(
                (arch.get_minimum_manylinux_minor()..=minor)
                    .rev()
                    .map(|minor| format!("manylinux_{}_{}_{}", major, minor, arch)),
            );
            if (arch.get_minimum_manylinux_minor()..=minor).contains(&17) {
                platform_tags.push(format!("manylinux2014_{}", arch))
            }
            if (arch.get_minimum_manylinux_minor()..=minor).contains(&12) {
                platform_tags.push(format!("manylinux2010_{}", arch))
            }
            if (arch.get_minimum_manylinux_minor()..=minor).contains(&5) {
                platform_tags.push(format!("manylinux1_{}", arch))
            }
            platform_tags
        }
        (Os::Musllinux { major, minor }, _) => {
            let mut platform_tags = vec![format!("linux_{}", arch)];
            // musl 1.1 is the lowest supported version in musllinux
            platform_tags.extend(
                (1..=minor)
                    .rev()
                    .map(|minor| format!("musllinux_{}_{}_{}", major, minor, arch)),
            );
            platform_tags
        }
        (Os::Macos { major, minor }, Arch::X86_64) => {
            // Source: https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L346
            let mut platform_tags = vec![];
            match major {
                10 => {
                    // Prior to Mac OS 11, each yearly release of Mac OS bumped the "minor" version
                    // number. The major version was always 10.
                    for minor in (0..=minor).rev() {
                        for binary_format in get_mac_binary_formats(major, minor, arch) {
                            platform_tags
                                .push(format!("macosx_{}_{}_{}", major, minor, binary_format));
                        }
                    }
                }
                x if x >= 11 => {
                    // Starting with Mac OS 11, each yearly release bumps the major version number.
                    // The minor versions are now the midyear updates.
                    for major in (10..=major).rev() {
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
                    return Err(Error::OsVersionDetection(format!(
                        "Unsupported mac os version: {}",
                        major,
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
            for major in (10..=major).rev() {
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
            Os::FreeBsd { release: _ }
            | Os::NetBsd { release: _ }
            | Os::OpenBsd { release: _ }
            | Os::Dragonfly { release: _ }
            | Os::Haiku { release: _ },
            _,
        ) => {
            let info = PlatformInfo::new().map_err(Error::PlatformInfo)?;
            let release = info.release().to_string_lossy().replace(['.', '-'], "_");
            vec![format!(
                "{}_{}_{}",
                os.to_string().to_lowercase(),
                release,
                arch
            )]
        }
        (
            Os::Illumos {
                mut release,
                mut arch,
            },
            _,
        ) => {
            let mut os = os.to_string().to_lowercase();
            // See https://github.com/python/cpython/blob/46c8d915715aa2bd4d697482aa051fe974d440e1/Lib/sysconfig.py#L722-L730
            if let Some((major, other)) = release.split_once('_') {
                let major_ver: u64 = major.parse().map_err(|err| {
                    Error::OsVersionDetection(format!(
                        "illumos major version is not a number: {}",
                        err
                    ))
                })?;
                if major_ver >= 5 {
                    // SunOS 5 == Solaris 2
                    os = "solaris".to_string();
                    release = format!("{}_{}", major_ver - 3, other);
                    arch = format!("{}_64bit", arch);
                }
            }
            vec![format!("{}_{}_{}", os, release, arch)]
        }
        _ => {
            return Err(Error::OsVersionDetection(format!(
                "Unsupported operating system and architecture combination: {} {}",
                os, arch
            )));
        }
    };
    Ok(platform_tags)
}

#[cfg(test)]
mod test {
    use super::{compatible_platform_tags, WheelFilename};
    use crate::{Arch, CompatibleTags, Error, Os};
    use fs_err::File;
    use std::str::FromStr;

    const FILENAMES: &[&str] = &[
        "numpy-1.22.2-pp38-pypy38_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        "numpy-1.22.2-cp310-cp310-win_amd64.whl",
        "numpy-1.22.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        "numpy-1.22.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
        "numpy-1.22.2-cp310-cp310-macosx_11_0_arm64.whl",
        "numpy-1.22.2-cp310-cp310-macosx_10_14_x86_64.whl",
        "numpy-1.22.2-cp39-cp39-win_amd64.whl",
        "numpy-1.22.2-cp39-cp39-win32.whl",
        "numpy-1.22.2-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        "numpy-1.22.2-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
        "numpy-1.22.2-cp39-cp39-macosx_11_0_arm64.whl",
        "numpy-1.22.2-cp39-cp39-macosx_10_14_x86_64.whl",
        "numpy-1.22.2-cp38-cp38-win_amd64.whl",
        "numpy-1.22.2-cp38-cp38-win32.whl",
        "numpy-1.22.2-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        "numpy-1.22.2-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
        "numpy-1.22.2-cp38-cp38-macosx_11_0_arm64.whl",
        "numpy-1.22.2-cp38-cp38-macosx_10_14_x86_64.whl",
        "tqdm-4.62.3-py2.py3-none-any.whl",
    ];

    /// Test that we can parse the filenames
    #[test]
    fn test_wheel_filename_parsing() -> Result<(), Error> {
        for filename in FILENAMES {
            WheelFilename::from_str(filename)?;
        }
        Ok(())
    }

    /// Test that we correctly identify compatible pairs
    #[test]
    fn test_compatibility() -> Result<(), Error> {
        let filenames = [
            (
                "numpy-1.22.2-cp38-cp38-win_amd64.whl",
                ((3, 8), Os::Windows, Arch::X86_64),
            ),
            (
                "numpy-1.22.2-cp38-cp38-win32.whl",
                ((3, 8), Os::Windows, Arch::X86),
            ),
            (
                "numpy-1.22.2-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
                (
                    (3, 8),
                    Os::Manylinux {
                        major: 2,
                        minor: 31,
                    },
                    Arch::X86_64,
                ),
            ),
            (
                "numpy-1.22.2-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl",
                (
                    (3, 8),
                    Os::Manylinux {
                        major: 2,
                        minor: 31,
                    },
                    Arch::Aarch64,
                ),
            ),
            (
                "numpy-1.22.2-cp38-cp38-macosx_11_0_arm64.whl",
                (
                    (3, 8),
                    Os::Macos {
                        major: 11,
                        minor: 0,
                    },
                    Arch::Aarch64,
                ),
            ),
            (
                "numpy-1.22.2-cp38-cp38-macosx_10_14_x86_64.whl",
                (
                    (3, 8),
                    // Test backwards compatibility here
                    Os::Macos {
                        major: 11,
                        minor: 0,
                    },
                    Arch::X86_64,
                ),
            ),
            (
                "ruff-0.0.63-py3-none-macosx_10_9_x86_64.macosx_11_0_arm64.macosx_10_9_universal2.whl",
                (
                    (3, 8),
                    Os::Macos {
                        major: 12,
                        minor: 0,
                    },
                    Arch::X86_64,
                ),
            ),
            (
                "ruff-0.0.63-py3-none-macosx_10_9_x86_64.macosx_11_0_arm64.macosx_10_9_universal2.whl",
                (
                    (3, 8),
                    Os::Macos {
                        major: 12,
                        minor: 0,
                    },
                    Arch::Aarch64,
                ),
            ),
            (
                "tqdm-4.62.3-py2.py3-none-any.whl",
                (
                    (3, 8),
                    Os::Manylinux {
                        major: 2,
                        minor: 31,
                    },
                    Arch::X86_64,
                ),
            ),
        ];

        for (filename, (python_version, os, arch)) in filenames {
            let compatible_tags = CompatibleTags::new(python_version, os, arch)?;
            assert!(
                WheelFilename::from_str(filename)?
                    .compatibility(&compatible_tags)
                    .is_ok(),
                "{}",
                filename
            );
        }
        Ok(())
    }

    /// Test that incompatible pairs don't pass is_compatible
    #[test]
    fn test_compatibility_filter() -> Result<(), Error> {
        let compatible_tags = CompatibleTags::new(
            (3, 8),
            Os::Manylinux {
                major: 2,
                minor: 31,
            },
            Arch::X86_64,
        )?;

        let compatible: Vec<&str> = FILENAMES
            .iter()
            .filter(|filename| {
                WheelFilename::from_str(filename)
                    .unwrap()
                    .compatibility(&compatible_tags)
                    .is_ok()
            })
            .cloned()
            .collect();
        assert_eq!(
            vec![
                "numpy-1.22.2-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
                "tqdm-4.62.3-py2.py3-none-any.whl"
            ],
            compatible
        );
        Ok(())
    }

    fn get_ubuntu_20_04_tags() -> Vec<String> {
        serde_json::from_reader(File::open("../../test-data/tags/cp38-ubuntu-20-04.json").unwrap())
            .unwrap()
    }

    /// Check against the tags that packaging.tags reports as compatible
    #[test]
    fn ubuntu_20_04_compatible() -> Result<(), Error> {
        let tags = get_ubuntu_20_04_tags();
        for tag in tags {
            let compatible_tags = CompatibleTags::new(
                (3, 8),
                Os::Manylinux {
                    major: 2,
                    minor: 31,
                },
                Arch::X86_64,
            )?;

            assert!(
                WheelFilename::from_str(&format!("foo-1.0-{}.whl", tag))?
                    .compatibility(&compatible_tags)
                    .is_ok(),
                "{}",
                tag
            )
        }
        Ok(())
    }

    /// Check against the tags that packaging.tags reports as compatible
    #[test]
    fn ubuntu_20_04_list() -> Result<(), Error> {
        let expected_tags = get_ubuntu_20_04_tags();
        let actual_tags: Vec<String> = CompatibleTags::new(
            (3, 8),
            Os::Manylinux {
                major: 2,
                minor: 31,
            },
            Arch::X86_64,
        )?
        .iter()
        .map(|(python_tag, abi_tag, platform_tag)| {
            format!("{}-{}-{}", python_tag, abi_tag, platform_tag)
        })
        .collect();
        assert_eq!(expected_tags, actual_tags);
        Ok(())
    }

    #[test]
    fn test_precedence() {
        let tags = CompatibleTags::new(
            (3, 8),
            Os::Manylinux {
                major: 2,
                minor: 31,
            },
            Arch::X86_64,
        )
        .unwrap();
        let pairs = [
            (
                "greenlet-2.0.2-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
                "greenlet-2.0.2-cp38-cp38-manylinux2010_x86_64.whl"
            ),
            (
                "regex-2022.10.31-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", 
                "regex-2022.10.31-cp38-cp38-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_12_x86_64.manylinux2010_x86_64.whl"
            ),
        ];
        for (higher_str, lower_str) in pairs {
            let higher = WheelFilename::from_str(higher_str).unwrap();
            let lower = WheelFilename::from_str(lower_str).unwrap();
            let higher_precedence = higher.compatibility(&tags).unwrap();
            let lower_precedence = lower.compatibility(&tags).unwrap();
            assert!(
                higher_precedence < lower_precedence,
                "{} {} {} {}",
                higher_str,
                higher_precedence,
                lower_str,
                lower_precedence
            );
        }
    }

    /// Basic does-it-work test
    #[test]
    fn host_arch() -> Result<(), Error> {
        let os = Os::current()?;
        let arch = Arch::current()?;
        compatible_platform_tags(&os, &arch)?;
        Ok(())
    }
}
