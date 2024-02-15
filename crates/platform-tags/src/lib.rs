use std::str::FromStr;
use std::sync::Arc;
use std::{cmp, num::NonZeroU32};

use rustc_hash::FxHashMap;

use platform_host::{Arch, Os, Platform, PlatformError};

#[derive(Debug, thiserror::Error)]
pub enum TagsError {
    #[error(transparent)]
    PlatformError(#[from] PlatformError),
    #[error("Unsupported implementation: {0}")]
    UnsupportedImplementation(String),
    #[error("Unknown implementation: {0}")]
    UnknownImplementation(String),
    #[error("Invalid priority: {0}")]
    InvalidPriority(usize, #[source] std::num::TryFromIntError),
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Clone)]
pub enum IncompatibleTag {
    Invalid,
    Python,
    Abi,
    Platform,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TagCompatibility {
    Incompatible(IncompatibleTag),
    Compatible(TagPriority),
}

impl Ord for TagCompatibility {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Compatible(p_self), Self::Compatible(p_other)) => p_self.cmp(p_other),
            (Self::Incompatible(_), Self::Compatible(_)) => cmp::Ordering::Less,
            (Self::Compatible(_), Self::Incompatible(_)) => cmp::Ordering::Greater,
            (Self::Incompatible(t_self), Self::Incompatible(t_other)) => t_self.cmp(t_other),
        }
    }
}

impl PartialOrd for TagCompatibility {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(TagCompatibility::cmp(self, other))
    }
}

impl TagCompatibility {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_))
    }
}

/// A set of compatible tags for a given Python version and platform.
///
/// Its principle function is to determine whether the tags for a particular
/// wheel are compatible with the current environment.
#[derive(Debug, Clone)]
pub struct Tags {
    /// python_tag |--> abi_tag |--> platform_tag |--> priority
    #[allow(clippy::type_complexity)]
    map: Arc<FxHashMap<String, FxHashMap<String, FxHashMap<String, TagPriority>>>>,
}

impl Tags {
    /// Create a new set of tags.
    ///
    /// Tags are prioritized based on their position in the given vector. Specifically, tags that
    /// appear earlier in the vector are given higher priority than tags that appear later.
    pub fn new(tags: Vec<(String, String, String)>) -> Self {
        let mut map = FxHashMap::default();
        for (index, (py, abi, platform)) in tags.into_iter().rev().enumerate() {
            map.entry(py.to_string())
                .or_insert(FxHashMap::default())
                .entry(abi.to_string())
                .or_insert(FxHashMap::default())
                .entry(platform.to_string())
                .or_insert(TagPriority::try_from(index).expect("valid tag priority"));
        }
        Self { map: Arc::new(map) }
    }

    /// Returns the compatible tags for the given Python implementation (e.g., `cpython`), version,
    /// and platform.
    pub fn from_env(
        platform: &Platform,
        python_version: (u8, u8),
        implementation_name: &str,
        implementation_version: (u8, u8),
    ) -> Result<Self, TagsError> {
        let implementation = Implementation::from_str(implementation_name)?;
        let platform_tags = compatible_tags(platform)?;

        let mut tags = Vec::with_capacity(5 * platform_tags.len());

        // 1. This exact c api version
        for platform_tag in &platform_tags {
            tags.push((
                implementation.language_tag(python_version),
                implementation.abi_tag(python_version, implementation_version),
                platform_tag.clone(),
            ));
            tags.push((
                implementation.language_tag(python_version),
                "none".to_string(),
                platform_tag.clone(),
            ));
        }
        // 2. abi3 and no abi (e.g. executable binary)
        if matches!(implementation, Implementation::CPython) {
            // For some reason 3.2 is the minimum python for the cp abi
            for minor in 2..=python_version.1 {
                for platform_tag in &platform_tags {
                    tags.push((
                        implementation.language_tag((python_version.0, minor)),
                        "abi3".to_string(),
                        platform_tag.clone(),
                    ));
                }
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
        Ok(Self::new(tags))
    }

    /// Returns true when there exists at least one tag for this platform
    /// whose individual components all appear in each of the slices given.
    ///
    /// Like [`Tags::compatibility`], but short-circuits as soon as a compatible
    /// tag is found.
    pub fn is_compatible(
        &self,
        wheel_python_tags: &[String],
        wheel_abi_tags: &[String],
        wheel_platform_tags: &[String],
    ) -> bool {
        // NOTE: A typical work-load is a context in which the platform tags
        // are quite large, but the tags of a wheel are quite small. It is
        // common, for example, for the lengths of the slices given to all be
        // 1. So while the looping here might look slow, the key thing we want
        // to avoid is looping over all of the platform tags. We avoid that
        // with hashmap lookups.

        for wheel_py in wheel_python_tags {
            let Some(abis) = self.map.get(wheel_py) else {
                continue;
            };
            for wheel_abi in wheel_abi_tags {
                let Some(platforms) = abis.get(wheel_abi) else {
                    continue;
                };
                for wheel_platform in wheel_platform_tags {
                    if platforms.contains_key(wheel_platform) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Returns the [`TagCompatiblity`] of the given tags.
    ///
    /// If compatible, includes the score of the most-compatible platform tag.
    /// If incompatible, includes the tag part which was a closest match.
    pub fn compatibility(
        &self,
        wheel_python_tags: &[String],
        wheel_abi_tags: &[String],
        wheel_platform_tags: &[String],
    ) -> TagCompatibility {
        let mut max_compatibility = TagCompatibility::Incompatible(IncompatibleTag::Invalid);

        for wheel_py in wheel_python_tags {
            let Some(abis) = self.map.get(wheel_py) else {
                max_compatibility =
                    max_compatibility.max(TagCompatibility::Incompatible(IncompatibleTag::Python));
                continue;
            };
            for wheel_abi in wheel_abi_tags {
                let Some(platforms) = abis.get(wheel_abi) else {
                    max_compatibility =
                        max_compatibility.max(TagCompatibility::Incompatible(IncompatibleTag::Abi));
                    continue;
                };
                for wheel_platform in wheel_platform_tags {
                    let priority = platforms.get(wheel_platform).copied();
                    if let Some(priority) = priority {
                        max_compatibility =
                            max_compatibility.max(TagCompatibility::Compatible(priority));
                    } else {
                        max_compatibility = max_compatibility
                            .max(TagCompatibility::Incompatible(IncompatibleTag::Platform));
                    }
                }
            }
        }
        max_compatibility
    }
}

/// The priority of a platform tag.
///
/// A wrapper around [`NonZeroU32`]. Higher values indicate higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TagPriority(NonZeroU32);

impl TryFrom<usize> for TagPriority {
    type Error = TagsError;

    /// Create a [`TagPriority`] from a `usize`, where higher `usize` values are given higher
    /// priority.
    fn try_from(priority: usize) -> Result<Self, TagsError> {
        match u32::try_from(priority).and_then(|priority| NonZeroU32::try_from(1 + priority)) {
            Ok(priority) => Ok(Self(priority)),
            Err(err) => Err(TagsError::InvalidPriority(priority, err)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Implementation {
    CPython,
    PyPy,
}

impl Implementation {
    /// Returns the "language implementation and version tag" for the current implementation and
    /// Python version (e.g., `cp39` or `pp37`).
    pub fn language_tag(&self, python_version: (u8, u8)) -> String {
        match self {
            // Ex) `cp39`
            Implementation::CPython => format!("cp{}{}", python_version.0, python_version.1),
            // Ex) `pp39`
            Implementation::PyPy => format!("pp{}{}", python_version.0, python_version.1),
        }
    }

    pub fn abi_tag(&self, python_version: (u8, u8), implementation_version: (u8, u8)) -> String {
        match self {
            // Ex) `cp39`
            Implementation::CPython => {
                if python_version.1 <= 7 {
                    format!("cp{}{}m", python_version.0, python_version.1)
                } else {
                    format!("cp{}{}", python_version.0, python_version.1)
                }
            }
            // Ex) `pypy39_pp73`
            Implementation::PyPy => format!(
                "pypy{}{}_pp{}{}",
                python_version.0,
                python_version.1,
                implementation_version.0,
                implementation_version.1
            ),
        }
    }
}

impl FromStr for Implementation {
    type Err = TagsError;

    fn from_str(s: &str) -> Result<Self, TagsError> {
        match s {
            // Known and supported implementations.
            "cpython" => Ok(Self::CPython),
            "pypy" => Ok(Self::PyPy),
            // Known but unsupported implementations.
            "python" => Err(TagsError::UnsupportedImplementation(s.to_string())),
            "ironpython" => Err(TagsError::UnsupportedImplementation(s.to_string())),
            "jython" => Err(TagsError::UnsupportedImplementation(s.to_string())),
            // Unknown implementations.
            _ => Err(TagsError::UnknownImplementation(s.to_string())),
        }
    }
}

/// Returns the compatible tags for the current [`Platform`] (e.g., `manylinux_2_17`,
/// `macosx_11_0_arm64`, or `win_amd64`).
///
/// We have two cases: Actual platform specific tags (including "merged" tags such as universal2)
/// and "any".
///
/// Bit of a mess, needs to be cleaned up.
fn compatible_tags(platform: &Platform) -> Result<Vec<String>, PlatformError> {
    let os = platform.os();
    let arch = platform.arch();

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
