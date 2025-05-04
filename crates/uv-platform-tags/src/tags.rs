use std::collections::BTreeSet;
use std::fmt::Formatter;
use std::str::FromStr;
use std::sync::Arc;
use std::{cmp, num::NonZeroU32};

use rustc_hash::FxHashMap;

use uv_small_str::SmallString;

use crate::{AbiTag, Arch, LanguageTag, Os, Platform, PlatformError, PlatformTag};

#[derive(Debug, thiserror::Error)]
pub enum TagsError {
    #[error(transparent)]
    PlatformError(#[from] PlatformError),
    #[error("Unsupported implementation: `{0}`")]
    UnsupportedImplementation(String),
    #[error("Unknown implementation: `{0}`")]
    UnknownImplementation(String),
    #[error("Invalid priority: `{0}`")]
    InvalidPriority(usize, #[source] std::num::TryFromIntError),
    #[error("Only CPython can be freethreading, not: {0}")]
    GilIsACPythonProblem(String),
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Copy, Clone)]
pub enum IncompatibleTag {
    /// The tag is invalid and cannot be used.
    Invalid,
    /// The Python implementation tag is incompatible.
    Python,
    /// The ABI tag is incompatible.
    Abi,
    /// The Python version component of the ABI tag is incompatible with `requires-python`.
    AbiPythonVersion,
    /// The platform tag is incompatible.
    Platform,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum TagCompatibility {
    Incompatible(IncompatibleTag),
    Compatible(TagPriority),
}

impl Ord for TagCompatibility {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
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
        Some(Self::cmp(self, other))
    }
}

impl TagCompatibility {
    /// Returns `true` if the tag is compatible.
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
    /// `python_tag` |--> `abi_tag` |--> `platform_tag` |--> priority
    #[allow(clippy::type_complexity)]
    map: Arc<FxHashMap<LanguageTag, FxHashMap<AbiTag, FxHashMap<PlatformTag, TagPriority>>>>,
    /// The highest-priority tag for the Python version and platform.
    best: Option<(LanguageTag, AbiTag, PlatformTag)>,
}

impl Tags {
    /// Create a new set of tags.
    ///
    /// Tags are prioritized based on their position in the given vector. Specifically, tags that
    /// appear earlier in the vector are given higher priority than tags that appear later.
    pub fn new(tags: Vec<(LanguageTag, AbiTag, PlatformTag)>) -> Self {
        // Store the highest-priority tag for each component.
        let best = tags.first().cloned();

        // Index the tags by Python version, ABI, and platform.
        let mut map = FxHashMap::default();
        for (index, (py, abi, platform)) in tags.into_iter().rev().enumerate() {
            map.entry(py)
                .or_insert(FxHashMap::default())
                .entry(abi)
                .or_insert(FxHashMap::default())
                .entry(platform)
                .or_insert(TagPriority::try_from(index).expect("valid tag priority"));
        }

        Self {
            map: Arc::new(map),
            best,
        }
    }

    /// Returns the compatible tags for the given Python implementation (e.g., `cpython`), version,
    /// and platform.
    pub fn from_env(
        platform: &Platform,
        python_version: (u8, u8),
        implementation_name: &str,
        implementation_version: (u8, u8),
        manylinux_compatible: bool,
        gil_disabled: bool,
    ) -> Result<Self, TagsError> {
        let implementation = Implementation::parse(implementation_name, gil_disabled)?;

        // Determine the compatible tags for the current platform.
        let platform_tags = {
            let mut platform_tags = compatible_tags(platform)?;
            if matches!(platform.os(), Os::Manylinux { .. }) && !manylinux_compatible {
                platform_tags.retain(|tag| !tag.is_manylinux());
            }
            platform_tags
        };

        let mut tags = Vec::with_capacity(5 * platform_tags.len());

        // 1. This exact c api version
        for platform_tag in &platform_tags {
            tags.push((
                implementation.language_tag(python_version),
                implementation.abi_tag(python_version, implementation_version),
                platform_tag.clone(),
            ));
        }
        // 2. abi3 and no abi (e.g. executable binary)
        if let Implementation::CPython { gil_disabled } = implementation {
            // For some reason 3.2 is the minimum python for the cp abi
            for minor in (2..=python_version.1).rev() {
                // No abi3 for free-threading python
                if !gil_disabled {
                    for platform_tag in &platform_tags {
                        tags.push((
                            implementation.language_tag((python_version.0, minor)),
                            AbiTag::Abi3,
                            platform_tag.clone(),
                        ));
                    }
                }
                // Only include `none` tags for the current CPython version
                if minor == python_version.1 {
                    for platform_tag in &platform_tags {
                        tags.push((
                            implementation.language_tag((python_version.0, minor)),
                            AbiTag::None,
                            platform_tag.clone(),
                        ));
                    }
                }
            }
        }
        // 3. no abi (e.g. executable binary)
        for minor in (0..=python_version.1).rev() {
            for platform_tag in &platform_tags {
                tags.push((
                    LanguageTag::Python {
                        major: python_version.0,
                        minor: Some(minor),
                    },
                    AbiTag::None,
                    platform_tag.clone(),
                ));
            }
            // After the matching version emit `none` tags for the major version i.e. `py3`
            if minor == python_version.1 {
                for platform_tag in &platform_tags {
                    tags.push((
                        LanguageTag::Python {
                            major: python_version.0,
                            minor: None,
                        },
                        AbiTag::None,
                        platform_tag.clone(),
                    ));
                }
            }
        }
        // 4. no binary
        if matches!(implementation, Implementation::CPython { .. }) {
            tags.push((
                implementation.language_tag(python_version),
                AbiTag::None,
                PlatformTag::Any,
            ));
        }
        for minor in (0..=python_version.1).rev() {
            tags.push((
                LanguageTag::Python {
                    major: python_version.0,
                    minor: Some(minor),
                },
                AbiTag::None,
                PlatformTag::Any,
            ));
            // After the matching version emit `none` tags for the major version i.e. `py3`
            if minor == python_version.1 {
                tags.push((
                    LanguageTag::Python {
                        major: python_version.0,
                        minor: None,
                    },
                    AbiTag::None,
                    PlatformTag::Any,
                ));
            }
        }
        Ok(Self::new(tags))
    }

    /// Returns true when there exists at least one tag for this platform
    /// whose individual components all appear in each of the slices given.
    ///
    /// Like [`Tags::compatibility`], but short-circuits as soon as a compatible
    /// tag is found.
    pub fn is_compatible(
        &self,
        wheel_python_tags: &[LanguageTag],
        wheel_abi_tags: &[AbiTag],
        wheel_platform_tags: &[PlatformTag],
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

    /// Returns the [`TagCompatibility`] of the given tags.
    ///
    /// If compatible, includes the score of the most-compatible platform tag.
    /// If incompatible, includes the tag part which was a closest match.
    pub fn compatibility(
        &self,
        wheel_python_tags: &[LanguageTag],
        wheel_abi_tags: &[AbiTag],
        wheel_platform_tags: &[PlatformTag],
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

    /// Return the highest-priority Python tag for the [`Tags`].
    pub fn python_tag(&self) -> Option<LanguageTag> {
        self.best.as_ref().map(|(python, _, _)| *python)
    }

    /// Return the highest-priority ABI tag for the [`Tags`].
    pub fn abi_tag(&self) -> Option<AbiTag> {
        self.best.as_ref().map(|(_, abi, _)| *abi)
    }

    /// Return the highest-priority platform tag for the [`Tags`].
    pub fn platform_tag(&self) -> Option<&PlatformTag> {
        self.best.as_ref().map(|(_, _, platform)| platform)
    }

    /// Returns `true` if the given language and ABI tags are compatible with the current
    /// environment.
    pub fn is_compatible_abi(&self, python_tag: LanguageTag, abi_tag: AbiTag) -> bool {
        self.map
            .get(&python_tag)
            .map(|abis| abis.contains_key(&abi_tag))
            .unwrap_or(false)
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

impl std::fmt::Display for Tags {
    /// Display tags from high to low priority
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut tags = BTreeSet::new();
        for (python_tag, abi_tags) in self.map.iter() {
            for (abi_tag, platform_tags) in abi_tags {
                for (platform_tag, priority) in platform_tags {
                    tags.insert((priority, format!("{python_tag}-{abi_tag}-{platform_tag}")));
                }
            }
        }
        for (_, tag) in tags.iter().rev() {
            writeln!(f, "{tag}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Implementation {
    CPython { gil_disabled: bool },
    PyPy,
    GraalPy,
    Pyston,
}

impl Implementation {
    /// Returns the "language implementation and version tag" for the current implementation and
    /// Python version (e.g., `cp39` or `pp37`).
    fn language_tag(self, python_version: (u8, u8)) -> LanguageTag {
        match self {
            // Ex) `cp39`
            Self::CPython { .. } => LanguageTag::CPython { python_version },
            // Ex) `pp39`
            Self::PyPy => LanguageTag::PyPy { python_version },
            // Ex) `graalpy310`
            Self::GraalPy => LanguageTag::GraalPy { python_version },
            // Ex) `pyston38`
            Self::Pyston => LanguageTag::Pyston { python_version },
        }
    }

    fn abi_tag(self, python_version: (u8, u8), implementation_version: (u8, u8)) -> AbiTag {
        match self {
            // Ex) `cp39`
            Self::CPython { gil_disabled } => AbiTag::CPython {
                gil_disabled,
                python_version,
            },
            // Ex) `pypy39_pp73`
            Self::PyPy => AbiTag::PyPy {
                python_version: Some(python_version),
                implementation_version,
            },
            // Ex) `graalpy240_310_native
            Self::GraalPy => AbiTag::GraalPy {
                python_version,
                implementation_version,
            },
            // Ex) `pyston_23_x86_64_linux`
            Self::Pyston => AbiTag::Pyston {
                implementation_version,
            },
        }
    }

    fn parse(name: &str, gil_disabled: bool) -> Result<Self, TagsError> {
        if gil_disabled && name != "cpython" {
            return Err(TagsError::GilIsACPythonProblem(name.to_string()));
        }
        match name {
            // Known and supported implementations.
            "cpython" => Ok(Self::CPython { gil_disabled }),
            "pypy" => Ok(Self::PyPy),
            "graalpy" => Ok(Self::GraalPy),
            "pyston" => Ok(Self::Pyston),
            // Known but unsupported implementations.
            "python" => Err(TagsError::UnsupportedImplementation(name.to_string())),
            "ironpython" => Err(TagsError::UnsupportedImplementation(name.to_string())),
            "jython" => Err(TagsError::UnsupportedImplementation(name.to_string())),
            // Unknown implementations.
            _ => Err(TagsError::UnknownImplementation(name.to_string())),
        }
    }
}

/// Returns the compatible tags for the current [`Platform`] (e.g., `manylinux_2_17`,
/// `macosx_11_0_arm64`, or `win_amd64`).
///
/// We have two cases: Actual platform specific tags (including "merged" tags such as universal2)
/// and "any".
fn compatible_tags(platform: &Platform) -> Result<Vec<PlatformTag>, PlatformError> {
    let os = platform.os();
    let arch = platform.arch();

    let platform_tags = match (&os, arch) {
        (Os::Manylinux { major, minor }, _) => {
            let mut platform_tags = Vec::new();
            if let Some(min_minor) = arch.get_minimum_manylinux_minor() {
                for minor in (min_minor..=*minor).rev() {
                    platform_tags.push(PlatformTag::Manylinux {
                        major: *major,
                        minor,
                        arch,
                    });
                    // Support legacy manylinux tags with lower priority
                    // <https://peps.python.org/pep-0600/#legacy-manylinux-tags>
                    if minor == 12 {
                        platform_tags.push(PlatformTag::Manylinux2010 { arch });
                    }
                    if minor == 17 {
                        platform_tags.push(PlatformTag::Manylinux2014 { arch });
                    }
                    if minor == 5 {
                        platform_tags.push(PlatformTag::Manylinux1 { arch });
                    }
                }
            }
            // Non-manylinux is given lowest priority.
            // <https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L444>
            platform_tags.push(PlatformTag::Linux { arch });
            platform_tags
        }
        (Os::Musllinux { major, minor }, _) => {
            let mut platform_tags = vec![PlatformTag::Linux { arch }];
            // musl 1.1 is the lowest supported version in musllinux
            platform_tags.extend((1..=*minor).map(|minor| PlatformTag::Musllinux {
                major: *major,
                minor,
                arch,
            }));
            platform_tags
        }
        (Os::Macos { major, minor }, Arch::X86_64) => {
            // Source: https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L346
            let mut platform_tags = vec![];
            match major {
                10 => {
                    // Prior to Mac OS 11, each yearly release of Mac OS bumped the "minor" version
                    // number. The major version was always 10.
                    for minor in (4..=*minor).rev() {
                        for binary_format in BinaryFormat::from_arch(arch) {
                            platform_tags.push(PlatformTag::Macos {
                                major: 10,
                                minor,
                                binary_format: *binary_format,
                            });
                        }
                    }
                }
                value if *value >= 11 => {
                    // Starting with Mac OS 11, each yearly release bumps the major version number.
                    // The minor versions are now the midyear updates.
                    for major in (11..=*major).rev() {
                        for binary_format in BinaryFormat::from_arch(arch) {
                            platform_tags.push(PlatformTag::Macos {
                                major,
                                minor: 0,
                                binary_format: *binary_format,
                            });
                        }
                    }
                    // The "universal2" binary format can have a macOS version earlier than 11.0
                    // when the x86_64 part of the binary supports that version of macOS.
                    for minor in (4..=16).rev() {
                        for binary_format in BinaryFormat::from_arch(arch) {
                            platform_tags.push(PlatformTag::Macos {
                                major: 10,
                                minor,
                                binary_format: *binary_format,
                            });
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
            for major in (11..=*major).rev() {
                for binary_format in BinaryFormat::from_arch(arch) {
                    platform_tags.push(PlatformTag::Macos {
                        major,
                        minor: 0,
                        binary_format: *binary_format,
                    });
                }
            }
            // The "universal2" binary format can have a macOS version earlier than 11.0
            // when the x86_64 part of the binary supports that version of macOS.
            platform_tags.extend((4..=16).rev().map(|minor| PlatformTag::Macos {
                major: 10,
                minor,
                binary_format: BinaryFormat::Universal2,
            }));
            platform_tags
        }
        (Os::Windows, Arch::X86) => {
            vec![PlatformTag::Win32]
        }
        (Os::Windows, Arch::X86_64) => {
            vec![PlatformTag::WinAmd64]
        }
        (Os::Windows, Arch::Aarch64) => vec![PlatformTag::WinArm64],
        (Os::FreeBsd { release }, arch) => {
            let release = release.replace(['.', '-'], "_");
            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::FreeBsd {
                release_arch: SmallString::from(release_arch),
            }]
        }
        (Os::NetBsd { release }, arch) => {
            let release = release.replace(['.', '-'], "_");
            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::NetBsd {
                release_arch: SmallString::from(release_arch),
            }]
        }
        (Os::OpenBsd { release }, arch) => {
            let release = release.replace(['.', '-'], "_");
            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::OpenBsd {
                release_arch: SmallString::from(release_arch),
            }]
        }
        (Os::Dragonfly { release }, arch) => {
            let release = release.replace(['.', '-'], "_");
            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::Dragonfly {
                release_arch: SmallString::from(release_arch),
            }]
        }
        (Os::Haiku { release }, arch) => {
            let release = release.replace(['.', '-'], "_");
            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::Haiku {
                release_arch: SmallString::from(release_arch),
            }]
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
                    let release = format!("{}_{}", major_ver - 3, other);
                    let arch = format!("{arch}_64bit");
                    let release_arch = format!("{release}_{arch}");
                    return Ok(vec![PlatformTag::Solaris {
                        release_arch: SmallString::from(release_arch),
                    }]);
                }
            }

            let release_arch = format!("{release}_{arch}");
            vec![PlatformTag::Illumos {
                release_arch: SmallString::from(release_arch),
            }]
        }
        (Os::Android { api_level }, _) => {
            vec![PlatformTag::Android {
                api_level: *api_level,
                arch,
            }]
        }
        _ => {
            return Err(PlatformError::OsVersionDetectionError(format!(
                "Unsupported operating system and architecture combination: {os} {arch}"
            )));
        }
    };
    Ok(platform_tags)
}

#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub enum BinaryFormat {
    Arm64,
    Fat,
    Fat32,
    Fat64,
    I386,
    Intel,
    Ppc,
    Ppc64,
    Universal,
    Universal2,
    X86_64,
}

impl std::fmt::Display for BinaryFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl FromStr for BinaryFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "arm64" => Ok(Self::Arm64),
            "fat" => Ok(Self::Fat),
            "fat32" => Ok(Self::Fat32),
            "fat64" => Ok(Self::Fat64),
            "i386" => Ok(Self::I386),
            "intel" => Ok(Self::Intel),
            "ppc" => Ok(Self::Ppc),
            "ppc64" => Ok(Self::Ppc64),
            "universal" => Ok(Self::Universal),
            "universal2" => Ok(Self::Universal2),
            "x86_64" => Ok(Self::X86_64),
            _ => Err(format!("Invalid binary format: {s}")),
        }
    }
}

impl BinaryFormat {
    /// Determine the appropriate binary formats for a macOS version.
    ///
    /// See: <https://github.com/pypa/packaging/blob/fd4f11139d1c884a637be8aa26bb60a31fbc9411/packaging/tags.py#L314>
    pub fn from_arch(arch: Arch) -> &'static [Self] {
        match arch {
            Arch::Aarch64 => &[Self::Arm64, Self::Universal2],
            Arch::Powerpc64 => &[Self::Ppc64, Self::Fat64, Self::Universal],
            Arch::Powerpc => &[Self::Ppc, Self::Fat32, Self::Fat, Self::Universal],
            Arch::X86 => &[
                Self::I386,
                Self::Intel,
                Self::Fat32,
                Self::Fat,
                Self::Universal,
            ],
            Arch::X86_64 => &[
                Self::X86_64,
                Self::Intel,
                Self::Fat64,
                Self::Fat32,
                Self::Universal2,
                Self::Universal,
            ],
            _ => unreachable!(),
        }
    }

    /// Return the supported `platform_machine` tags for the binary format.
    ///
    /// This is roughly the inverse of the above: given a binary format, which `platform_machine`
    /// tags are supported?
    pub fn platform_machine(&self) -> &'static [BinaryFormat] {
        match self {
            Self::Arm64 => &[Self::Arm64],
            Self::Fat => &[Self::X86_64, Self::Ppc],
            Self::Fat32 => &[Self::X86_64, Self::I386, Self::Ppc, Self::Ppc64],
            Self::Fat64 => &[Self::X86_64, Self::Ppc64],
            Self::I386 => &[Self::I386],
            Self::Intel => &[Self::X86_64, Self::I386],
            Self::Ppc => &[Self::Ppc],
            Self::Ppc64 => &[Self::Ppc64],
            Self::Universal => &[
                Self::X86_64,
                Self::I386,
                Self::Ppc64,
                Self::Ppc,
                Self::Intel,
            ],
            Self::Universal2 => &[Self::X86_64, Self::Arm64],
            Self::X86_64 => &[Self::X86_64],
        }
    }

    /// Return the canonical name of the binary format.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::Fat => "fat",
            Self::Fat32 => "fat32",
            Self::Fat64 => "fat64",
            Self::I386 => "i386",
            Self::Intel => "intel",
            Self::Ppc => "ppc",
            Self::Ppc64 => "ppc64",
            Self::Universal => "universal",
            Self::Universal2 => "universal2",
            Self::X86_64 => "x86_64",
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::*;

    /// Check platform tag ordering.
    /// The list is displayed in decreasing priority.
    ///
    /// A reference list can be generated with:
    /// ```text
    /// $ python -c "from packaging import tags; [print(tag) for tag in tags.platform_tags()]"`
    /// ````
    #[test]
    fn test_platform_tags_manylinux() {
        let tags = compatible_tags(&Platform::new(
            Os::Manylinux {
                major: 2,
                minor: 20,
            },
            Arch::X86_64,
        ))
        .unwrap();
        let tags = tags.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_debug_snapshot!(
            tags,
            @r###"
    [
        "manylinux_2_20_x86_64",
        "manylinux_2_19_x86_64",
        "manylinux_2_18_x86_64",
        "manylinux_2_17_x86_64",
        "manylinux2014_x86_64",
        "manylinux_2_16_x86_64",
        "manylinux_2_15_x86_64",
        "manylinux_2_14_x86_64",
        "manylinux_2_13_x86_64",
        "manylinux_2_12_x86_64",
        "manylinux2010_x86_64",
        "manylinux_2_11_x86_64",
        "manylinux_2_10_x86_64",
        "manylinux_2_9_x86_64",
        "manylinux_2_8_x86_64",
        "manylinux_2_7_x86_64",
        "manylinux_2_6_x86_64",
        "manylinux_2_5_x86_64",
        "manylinux1_x86_64",
        "linux_x86_64",
    ]
    "###
        );
    }

    #[test]
    fn test_platform_tags_macos() {
        let tags = compatible_tags(&Platform::new(
            Os::Macos {
                major: 21,
                minor: 6,
            },
            Arch::X86_64,
        ))
        .unwrap();
        let tags = tags.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_debug_snapshot!(
            tags,
            @r###"
    [
        "macosx_21_0_x86_64",
        "macosx_21_0_intel",
        "macosx_21_0_fat64",
        "macosx_21_0_fat32",
        "macosx_21_0_universal2",
        "macosx_21_0_universal",
        "macosx_20_0_x86_64",
        "macosx_20_0_intel",
        "macosx_20_0_fat64",
        "macosx_20_0_fat32",
        "macosx_20_0_universal2",
        "macosx_20_0_universal",
        "macosx_19_0_x86_64",
        "macosx_19_0_intel",
        "macosx_19_0_fat64",
        "macosx_19_0_fat32",
        "macosx_19_0_universal2",
        "macosx_19_0_universal",
        "macosx_18_0_x86_64",
        "macosx_18_0_intel",
        "macosx_18_0_fat64",
        "macosx_18_0_fat32",
        "macosx_18_0_universal2",
        "macosx_18_0_universal",
        "macosx_17_0_x86_64",
        "macosx_17_0_intel",
        "macosx_17_0_fat64",
        "macosx_17_0_fat32",
        "macosx_17_0_universal2",
        "macosx_17_0_universal",
        "macosx_16_0_x86_64",
        "macosx_16_0_intel",
        "macosx_16_0_fat64",
        "macosx_16_0_fat32",
        "macosx_16_0_universal2",
        "macosx_16_0_universal",
        "macosx_15_0_x86_64",
        "macosx_15_0_intel",
        "macosx_15_0_fat64",
        "macosx_15_0_fat32",
        "macosx_15_0_universal2",
        "macosx_15_0_universal",
        "macosx_14_0_x86_64",
        "macosx_14_0_intel",
        "macosx_14_0_fat64",
        "macosx_14_0_fat32",
        "macosx_14_0_universal2",
        "macosx_14_0_universal",
        "macosx_13_0_x86_64",
        "macosx_13_0_intel",
        "macosx_13_0_fat64",
        "macosx_13_0_fat32",
        "macosx_13_0_universal2",
        "macosx_13_0_universal",
        "macosx_12_0_x86_64",
        "macosx_12_0_intel",
        "macosx_12_0_fat64",
        "macosx_12_0_fat32",
        "macosx_12_0_universal2",
        "macosx_12_0_universal",
        "macosx_11_0_x86_64",
        "macosx_11_0_intel",
        "macosx_11_0_fat64",
        "macosx_11_0_fat32",
        "macosx_11_0_universal2",
        "macosx_11_0_universal",
        "macosx_10_16_x86_64",
        "macosx_10_16_intel",
        "macosx_10_16_fat64",
        "macosx_10_16_fat32",
        "macosx_10_16_universal2",
        "macosx_10_16_universal",
        "macosx_10_15_x86_64",
        "macosx_10_15_intel",
        "macosx_10_15_fat64",
        "macosx_10_15_fat32",
        "macosx_10_15_universal2",
        "macosx_10_15_universal",
        "macosx_10_14_x86_64",
        "macosx_10_14_intel",
        "macosx_10_14_fat64",
        "macosx_10_14_fat32",
        "macosx_10_14_universal2",
        "macosx_10_14_universal",
        "macosx_10_13_x86_64",
        "macosx_10_13_intel",
        "macosx_10_13_fat64",
        "macosx_10_13_fat32",
        "macosx_10_13_universal2",
        "macosx_10_13_universal",
        "macosx_10_12_x86_64",
        "macosx_10_12_intel",
        "macosx_10_12_fat64",
        "macosx_10_12_fat32",
        "macosx_10_12_universal2",
        "macosx_10_12_universal",
        "macosx_10_11_x86_64",
        "macosx_10_11_intel",
        "macosx_10_11_fat64",
        "macosx_10_11_fat32",
        "macosx_10_11_universal2",
        "macosx_10_11_universal",
        "macosx_10_10_x86_64",
        "macosx_10_10_intel",
        "macosx_10_10_fat64",
        "macosx_10_10_fat32",
        "macosx_10_10_universal2",
        "macosx_10_10_universal",
        "macosx_10_9_x86_64",
        "macosx_10_9_intel",
        "macosx_10_9_fat64",
        "macosx_10_9_fat32",
        "macosx_10_9_universal2",
        "macosx_10_9_universal",
        "macosx_10_8_x86_64",
        "macosx_10_8_intel",
        "macosx_10_8_fat64",
        "macosx_10_8_fat32",
        "macosx_10_8_universal2",
        "macosx_10_8_universal",
        "macosx_10_7_x86_64",
        "macosx_10_7_intel",
        "macosx_10_7_fat64",
        "macosx_10_7_fat32",
        "macosx_10_7_universal2",
        "macosx_10_7_universal",
        "macosx_10_6_x86_64",
        "macosx_10_6_intel",
        "macosx_10_6_fat64",
        "macosx_10_6_fat32",
        "macosx_10_6_universal2",
        "macosx_10_6_universal",
        "macosx_10_5_x86_64",
        "macosx_10_5_intel",
        "macosx_10_5_fat64",
        "macosx_10_5_fat32",
        "macosx_10_5_universal2",
        "macosx_10_5_universal",
        "macosx_10_4_x86_64",
        "macosx_10_4_intel",
        "macosx_10_4_fat64",
        "macosx_10_4_fat32",
        "macosx_10_4_universal2",
        "macosx_10_4_universal",
    ]
    "###
        );

        let tags = compatible_tags(&Platform::new(
            Os::Macos {
                major: 14,
                minor: 0,
            },
            Arch::X86_64,
        ))
        .unwrap();
        let tags = tags.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_debug_snapshot!(
            tags,
            @r###"
    [
        "macosx_14_0_x86_64",
        "macosx_14_0_intel",
        "macosx_14_0_fat64",
        "macosx_14_0_fat32",
        "macosx_14_0_universal2",
        "macosx_14_0_universal",
        "macosx_13_0_x86_64",
        "macosx_13_0_intel",
        "macosx_13_0_fat64",
        "macosx_13_0_fat32",
        "macosx_13_0_universal2",
        "macosx_13_0_universal",
        "macosx_12_0_x86_64",
        "macosx_12_0_intel",
        "macosx_12_0_fat64",
        "macosx_12_0_fat32",
        "macosx_12_0_universal2",
        "macosx_12_0_universal",
        "macosx_11_0_x86_64",
        "macosx_11_0_intel",
        "macosx_11_0_fat64",
        "macosx_11_0_fat32",
        "macosx_11_0_universal2",
        "macosx_11_0_universal",
        "macosx_10_16_x86_64",
        "macosx_10_16_intel",
        "macosx_10_16_fat64",
        "macosx_10_16_fat32",
        "macosx_10_16_universal2",
        "macosx_10_16_universal",
        "macosx_10_15_x86_64",
        "macosx_10_15_intel",
        "macosx_10_15_fat64",
        "macosx_10_15_fat32",
        "macosx_10_15_universal2",
        "macosx_10_15_universal",
        "macosx_10_14_x86_64",
        "macosx_10_14_intel",
        "macosx_10_14_fat64",
        "macosx_10_14_fat32",
        "macosx_10_14_universal2",
        "macosx_10_14_universal",
        "macosx_10_13_x86_64",
        "macosx_10_13_intel",
        "macosx_10_13_fat64",
        "macosx_10_13_fat32",
        "macosx_10_13_universal2",
        "macosx_10_13_universal",
        "macosx_10_12_x86_64",
        "macosx_10_12_intel",
        "macosx_10_12_fat64",
        "macosx_10_12_fat32",
        "macosx_10_12_universal2",
        "macosx_10_12_universal",
        "macosx_10_11_x86_64",
        "macosx_10_11_intel",
        "macosx_10_11_fat64",
        "macosx_10_11_fat32",
        "macosx_10_11_universal2",
        "macosx_10_11_universal",
        "macosx_10_10_x86_64",
        "macosx_10_10_intel",
        "macosx_10_10_fat64",
        "macosx_10_10_fat32",
        "macosx_10_10_universal2",
        "macosx_10_10_universal",
        "macosx_10_9_x86_64",
        "macosx_10_9_intel",
        "macosx_10_9_fat64",
        "macosx_10_9_fat32",
        "macosx_10_9_universal2",
        "macosx_10_9_universal",
        "macosx_10_8_x86_64",
        "macosx_10_8_intel",
        "macosx_10_8_fat64",
        "macosx_10_8_fat32",
        "macosx_10_8_universal2",
        "macosx_10_8_universal",
        "macosx_10_7_x86_64",
        "macosx_10_7_intel",
        "macosx_10_7_fat64",
        "macosx_10_7_fat32",
        "macosx_10_7_universal2",
        "macosx_10_7_universal",
        "macosx_10_6_x86_64",
        "macosx_10_6_intel",
        "macosx_10_6_fat64",
        "macosx_10_6_fat32",
        "macosx_10_6_universal2",
        "macosx_10_6_universal",
        "macosx_10_5_x86_64",
        "macosx_10_5_intel",
        "macosx_10_5_fat64",
        "macosx_10_5_fat32",
        "macosx_10_5_universal2",
        "macosx_10_5_universal",
        "macosx_10_4_x86_64",
        "macosx_10_4_intel",
        "macosx_10_4_fat64",
        "macosx_10_4_fat32",
        "macosx_10_4_universal2",
        "macosx_10_4_universal",
    ]
    "###
        );

        let tags = compatible_tags(&Platform::new(
            Os::Macos {
                major: 10,
                minor: 6,
            },
            Arch::X86_64,
        ))
        .unwrap();
        let tags = tags.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_debug_snapshot!(
            tags,
            @r###"
    [
        "macosx_10_6_x86_64",
        "macosx_10_6_intel",
        "macosx_10_6_fat64",
        "macosx_10_6_fat32",
        "macosx_10_6_universal2",
        "macosx_10_6_universal",
        "macosx_10_5_x86_64",
        "macosx_10_5_intel",
        "macosx_10_5_fat64",
        "macosx_10_5_fat32",
        "macosx_10_5_universal2",
        "macosx_10_5_universal",
        "macosx_10_4_x86_64",
        "macosx_10_4_intel",
        "macosx_10_4_fat64",
        "macosx_10_4_fat32",
        "macosx_10_4_universal2",
        "macosx_10_4_universal",
    ]
    "###
        );
    }

    /// Ensure the tags returned do not include the `manylinux` tags
    /// when `manylinux_incompatible` is set to `false`.
    #[test]
    fn test_manylinux_incompatible() {
        let tags = Tags::from_env(
            &Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            (3, 9),
            "cpython",
            (3, 9),
            false,
            false,
        )
        .unwrap();
        assert_snapshot!(
        tags,
        @r###"
    cp39-cp39-linux_x86_64
    cp39-abi3-linux_x86_64
    cp39-none-linux_x86_64
    cp38-abi3-linux_x86_64
    cp37-abi3-linux_x86_64
    cp36-abi3-linux_x86_64
    cp35-abi3-linux_x86_64
    cp34-abi3-linux_x86_64
    cp33-abi3-linux_x86_64
    cp32-abi3-linux_x86_64
    py39-none-linux_x86_64
    py3-none-linux_x86_64
    py38-none-linux_x86_64
    py37-none-linux_x86_64
    py36-none-linux_x86_64
    py35-none-linux_x86_64
    py34-none-linux_x86_64
    py33-none-linux_x86_64
    py32-none-linux_x86_64
    py31-none-linux_x86_64
    py30-none-linux_x86_64
    cp39-none-any
    py39-none-any
    py3-none-any
    py38-none-any
    py37-none-any
    py36-none-any
    py35-none-any
    py34-none-any
    py33-none-any
    py32-none-any
    py31-none-any
    py30-none-any
    "###);
    }

    /// Check full tag ordering.
    /// The list is displayed in decreasing priority.
    ///
    /// A reference list can be generated with:
    /// ```text
    /// $ python -c "from packaging import tags; [print(tag) for tag in tags.sys_tags()]"`
    /// ```
    #[test]
    fn test_system_tags_manylinux() {
        let tags = Tags::from_env(
            &Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            (3, 9),
            "cpython",
            (3, 9),
            true,
            false,
        )
        .unwrap();
        assert_snapshot!(
            tags,
            @r###"
    cp39-cp39-manylinux_2_28_x86_64
    cp39-cp39-manylinux_2_27_x86_64
    cp39-cp39-manylinux_2_26_x86_64
    cp39-cp39-manylinux_2_25_x86_64
    cp39-cp39-manylinux_2_24_x86_64
    cp39-cp39-manylinux_2_23_x86_64
    cp39-cp39-manylinux_2_22_x86_64
    cp39-cp39-manylinux_2_21_x86_64
    cp39-cp39-manylinux_2_20_x86_64
    cp39-cp39-manylinux_2_19_x86_64
    cp39-cp39-manylinux_2_18_x86_64
    cp39-cp39-manylinux_2_17_x86_64
    cp39-cp39-manylinux2014_x86_64
    cp39-cp39-manylinux_2_16_x86_64
    cp39-cp39-manylinux_2_15_x86_64
    cp39-cp39-manylinux_2_14_x86_64
    cp39-cp39-manylinux_2_13_x86_64
    cp39-cp39-manylinux_2_12_x86_64
    cp39-cp39-manylinux2010_x86_64
    cp39-cp39-manylinux_2_11_x86_64
    cp39-cp39-manylinux_2_10_x86_64
    cp39-cp39-manylinux_2_9_x86_64
    cp39-cp39-manylinux_2_8_x86_64
    cp39-cp39-manylinux_2_7_x86_64
    cp39-cp39-manylinux_2_6_x86_64
    cp39-cp39-manylinux_2_5_x86_64
    cp39-cp39-manylinux1_x86_64
    cp39-cp39-linux_x86_64
    cp39-abi3-manylinux_2_28_x86_64
    cp39-abi3-manylinux_2_27_x86_64
    cp39-abi3-manylinux_2_26_x86_64
    cp39-abi3-manylinux_2_25_x86_64
    cp39-abi3-manylinux_2_24_x86_64
    cp39-abi3-manylinux_2_23_x86_64
    cp39-abi3-manylinux_2_22_x86_64
    cp39-abi3-manylinux_2_21_x86_64
    cp39-abi3-manylinux_2_20_x86_64
    cp39-abi3-manylinux_2_19_x86_64
    cp39-abi3-manylinux_2_18_x86_64
    cp39-abi3-manylinux_2_17_x86_64
    cp39-abi3-manylinux2014_x86_64
    cp39-abi3-manylinux_2_16_x86_64
    cp39-abi3-manylinux_2_15_x86_64
    cp39-abi3-manylinux_2_14_x86_64
    cp39-abi3-manylinux_2_13_x86_64
    cp39-abi3-manylinux_2_12_x86_64
    cp39-abi3-manylinux2010_x86_64
    cp39-abi3-manylinux_2_11_x86_64
    cp39-abi3-manylinux_2_10_x86_64
    cp39-abi3-manylinux_2_9_x86_64
    cp39-abi3-manylinux_2_8_x86_64
    cp39-abi3-manylinux_2_7_x86_64
    cp39-abi3-manylinux_2_6_x86_64
    cp39-abi3-manylinux_2_5_x86_64
    cp39-abi3-manylinux1_x86_64
    cp39-abi3-linux_x86_64
    cp39-none-manylinux_2_28_x86_64
    cp39-none-manylinux_2_27_x86_64
    cp39-none-manylinux_2_26_x86_64
    cp39-none-manylinux_2_25_x86_64
    cp39-none-manylinux_2_24_x86_64
    cp39-none-manylinux_2_23_x86_64
    cp39-none-manylinux_2_22_x86_64
    cp39-none-manylinux_2_21_x86_64
    cp39-none-manylinux_2_20_x86_64
    cp39-none-manylinux_2_19_x86_64
    cp39-none-manylinux_2_18_x86_64
    cp39-none-manylinux_2_17_x86_64
    cp39-none-manylinux2014_x86_64
    cp39-none-manylinux_2_16_x86_64
    cp39-none-manylinux_2_15_x86_64
    cp39-none-manylinux_2_14_x86_64
    cp39-none-manylinux_2_13_x86_64
    cp39-none-manylinux_2_12_x86_64
    cp39-none-manylinux2010_x86_64
    cp39-none-manylinux_2_11_x86_64
    cp39-none-manylinux_2_10_x86_64
    cp39-none-manylinux_2_9_x86_64
    cp39-none-manylinux_2_8_x86_64
    cp39-none-manylinux_2_7_x86_64
    cp39-none-manylinux_2_6_x86_64
    cp39-none-manylinux_2_5_x86_64
    cp39-none-manylinux1_x86_64
    cp39-none-linux_x86_64
    cp38-abi3-manylinux_2_28_x86_64
    cp38-abi3-manylinux_2_27_x86_64
    cp38-abi3-manylinux_2_26_x86_64
    cp38-abi3-manylinux_2_25_x86_64
    cp38-abi3-manylinux_2_24_x86_64
    cp38-abi3-manylinux_2_23_x86_64
    cp38-abi3-manylinux_2_22_x86_64
    cp38-abi3-manylinux_2_21_x86_64
    cp38-abi3-manylinux_2_20_x86_64
    cp38-abi3-manylinux_2_19_x86_64
    cp38-abi3-manylinux_2_18_x86_64
    cp38-abi3-manylinux_2_17_x86_64
    cp38-abi3-manylinux2014_x86_64
    cp38-abi3-manylinux_2_16_x86_64
    cp38-abi3-manylinux_2_15_x86_64
    cp38-abi3-manylinux_2_14_x86_64
    cp38-abi3-manylinux_2_13_x86_64
    cp38-abi3-manylinux_2_12_x86_64
    cp38-abi3-manylinux2010_x86_64
    cp38-abi3-manylinux_2_11_x86_64
    cp38-abi3-manylinux_2_10_x86_64
    cp38-abi3-manylinux_2_9_x86_64
    cp38-abi3-manylinux_2_8_x86_64
    cp38-abi3-manylinux_2_7_x86_64
    cp38-abi3-manylinux_2_6_x86_64
    cp38-abi3-manylinux_2_5_x86_64
    cp38-abi3-manylinux1_x86_64
    cp38-abi3-linux_x86_64
    cp37-abi3-manylinux_2_28_x86_64
    cp37-abi3-manylinux_2_27_x86_64
    cp37-abi3-manylinux_2_26_x86_64
    cp37-abi3-manylinux_2_25_x86_64
    cp37-abi3-manylinux_2_24_x86_64
    cp37-abi3-manylinux_2_23_x86_64
    cp37-abi3-manylinux_2_22_x86_64
    cp37-abi3-manylinux_2_21_x86_64
    cp37-abi3-manylinux_2_20_x86_64
    cp37-abi3-manylinux_2_19_x86_64
    cp37-abi3-manylinux_2_18_x86_64
    cp37-abi3-manylinux_2_17_x86_64
    cp37-abi3-manylinux2014_x86_64
    cp37-abi3-manylinux_2_16_x86_64
    cp37-abi3-manylinux_2_15_x86_64
    cp37-abi3-manylinux_2_14_x86_64
    cp37-abi3-manylinux_2_13_x86_64
    cp37-abi3-manylinux_2_12_x86_64
    cp37-abi3-manylinux2010_x86_64
    cp37-abi3-manylinux_2_11_x86_64
    cp37-abi3-manylinux_2_10_x86_64
    cp37-abi3-manylinux_2_9_x86_64
    cp37-abi3-manylinux_2_8_x86_64
    cp37-abi3-manylinux_2_7_x86_64
    cp37-abi3-manylinux_2_6_x86_64
    cp37-abi3-manylinux_2_5_x86_64
    cp37-abi3-manylinux1_x86_64
    cp37-abi3-linux_x86_64
    cp36-abi3-manylinux_2_28_x86_64
    cp36-abi3-manylinux_2_27_x86_64
    cp36-abi3-manylinux_2_26_x86_64
    cp36-abi3-manylinux_2_25_x86_64
    cp36-abi3-manylinux_2_24_x86_64
    cp36-abi3-manylinux_2_23_x86_64
    cp36-abi3-manylinux_2_22_x86_64
    cp36-abi3-manylinux_2_21_x86_64
    cp36-abi3-manylinux_2_20_x86_64
    cp36-abi3-manylinux_2_19_x86_64
    cp36-abi3-manylinux_2_18_x86_64
    cp36-abi3-manylinux_2_17_x86_64
    cp36-abi3-manylinux2014_x86_64
    cp36-abi3-manylinux_2_16_x86_64
    cp36-abi3-manylinux_2_15_x86_64
    cp36-abi3-manylinux_2_14_x86_64
    cp36-abi3-manylinux_2_13_x86_64
    cp36-abi3-manylinux_2_12_x86_64
    cp36-abi3-manylinux2010_x86_64
    cp36-abi3-manylinux_2_11_x86_64
    cp36-abi3-manylinux_2_10_x86_64
    cp36-abi3-manylinux_2_9_x86_64
    cp36-abi3-manylinux_2_8_x86_64
    cp36-abi3-manylinux_2_7_x86_64
    cp36-abi3-manylinux_2_6_x86_64
    cp36-abi3-manylinux_2_5_x86_64
    cp36-abi3-manylinux1_x86_64
    cp36-abi3-linux_x86_64
    cp35-abi3-manylinux_2_28_x86_64
    cp35-abi3-manylinux_2_27_x86_64
    cp35-abi3-manylinux_2_26_x86_64
    cp35-abi3-manylinux_2_25_x86_64
    cp35-abi3-manylinux_2_24_x86_64
    cp35-abi3-manylinux_2_23_x86_64
    cp35-abi3-manylinux_2_22_x86_64
    cp35-abi3-manylinux_2_21_x86_64
    cp35-abi3-manylinux_2_20_x86_64
    cp35-abi3-manylinux_2_19_x86_64
    cp35-abi3-manylinux_2_18_x86_64
    cp35-abi3-manylinux_2_17_x86_64
    cp35-abi3-manylinux2014_x86_64
    cp35-abi3-manylinux_2_16_x86_64
    cp35-abi3-manylinux_2_15_x86_64
    cp35-abi3-manylinux_2_14_x86_64
    cp35-abi3-manylinux_2_13_x86_64
    cp35-abi3-manylinux_2_12_x86_64
    cp35-abi3-manylinux2010_x86_64
    cp35-abi3-manylinux_2_11_x86_64
    cp35-abi3-manylinux_2_10_x86_64
    cp35-abi3-manylinux_2_9_x86_64
    cp35-abi3-manylinux_2_8_x86_64
    cp35-abi3-manylinux_2_7_x86_64
    cp35-abi3-manylinux_2_6_x86_64
    cp35-abi3-manylinux_2_5_x86_64
    cp35-abi3-manylinux1_x86_64
    cp35-abi3-linux_x86_64
    cp34-abi3-manylinux_2_28_x86_64
    cp34-abi3-manylinux_2_27_x86_64
    cp34-abi3-manylinux_2_26_x86_64
    cp34-abi3-manylinux_2_25_x86_64
    cp34-abi3-manylinux_2_24_x86_64
    cp34-abi3-manylinux_2_23_x86_64
    cp34-abi3-manylinux_2_22_x86_64
    cp34-abi3-manylinux_2_21_x86_64
    cp34-abi3-manylinux_2_20_x86_64
    cp34-abi3-manylinux_2_19_x86_64
    cp34-abi3-manylinux_2_18_x86_64
    cp34-abi3-manylinux_2_17_x86_64
    cp34-abi3-manylinux2014_x86_64
    cp34-abi3-manylinux_2_16_x86_64
    cp34-abi3-manylinux_2_15_x86_64
    cp34-abi3-manylinux_2_14_x86_64
    cp34-abi3-manylinux_2_13_x86_64
    cp34-abi3-manylinux_2_12_x86_64
    cp34-abi3-manylinux2010_x86_64
    cp34-abi3-manylinux_2_11_x86_64
    cp34-abi3-manylinux_2_10_x86_64
    cp34-abi3-manylinux_2_9_x86_64
    cp34-abi3-manylinux_2_8_x86_64
    cp34-abi3-manylinux_2_7_x86_64
    cp34-abi3-manylinux_2_6_x86_64
    cp34-abi3-manylinux_2_5_x86_64
    cp34-abi3-manylinux1_x86_64
    cp34-abi3-linux_x86_64
    cp33-abi3-manylinux_2_28_x86_64
    cp33-abi3-manylinux_2_27_x86_64
    cp33-abi3-manylinux_2_26_x86_64
    cp33-abi3-manylinux_2_25_x86_64
    cp33-abi3-manylinux_2_24_x86_64
    cp33-abi3-manylinux_2_23_x86_64
    cp33-abi3-manylinux_2_22_x86_64
    cp33-abi3-manylinux_2_21_x86_64
    cp33-abi3-manylinux_2_20_x86_64
    cp33-abi3-manylinux_2_19_x86_64
    cp33-abi3-manylinux_2_18_x86_64
    cp33-abi3-manylinux_2_17_x86_64
    cp33-abi3-manylinux2014_x86_64
    cp33-abi3-manylinux_2_16_x86_64
    cp33-abi3-manylinux_2_15_x86_64
    cp33-abi3-manylinux_2_14_x86_64
    cp33-abi3-manylinux_2_13_x86_64
    cp33-abi3-manylinux_2_12_x86_64
    cp33-abi3-manylinux2010_x86_64
    cp33-abi3-manylinux_2_11_x86_64
    cp33-abi3-manylinux_2_10_x86_64
    cp33-abi3-manylinux_2_9_x86_64
    cp33-abi3-manylinux_2_8_x86_64
    cp33-abi3-manylinux_2_7_x86_64
    cp33-abi3-manylinux_2_6_x86_64
    cp33-abi3-manylinux_2_5_x86_64
    cp33-abi3-manylinux1_x86_64
    cp33-abi3-linux_x86_64
    cp32-abi3-manylinux_2_28_x86_64
    cp32-abi3-manylinux_2_27_x86_64
    cp32-abi3-manylinux_2_26_x86_64
    cp32-abi3-manylinux_2_25_x86_64
    cp32-abi3-manylinux_2_24_x86_64
    cp32-abi3-manylinux_2_23_x86_64
    cp32-abi3-manylinux_2_22_x86_64
    cp32-abi3-manylinux_2_21_x86_64
    cp32-abi3-manylinux_2_20_x86_64
    cp32-abi3-manylinux_2_19_x86_64
    cp32-abi3-manylinux_2_18_x86_64
    cp32-abi3-manylinux_2_17_x86_64
    cp32-abi3-manylinux2014_x86_64
    cp32-abi3-manylinux_2_16_x86_64
    cp32-abi3-manylinux_2_15_x86_64
    cp32-abi3-manylinux_2_14_x86_64
    cp32-abi3-manylinux_2_13_x86_64
    cp32-abi3-manylinux_2_12_x86_64
    cp32-abi3-manylinux2010_x86_64
    cp32-abi3-manylinux_2_11_x86_64
    cp32-abi3-manylinux_2_10_x86_64
    cp32-abi3-manylinux_2_9_x86_64
    cp32-abi3-manylinux_2_8_x86_64
    cp32-abi3-manylinux_2_7_x86_64
    cp32-abi3-manylinux_2_6_x86_64
    cp32-abi3-manylinux_2_5_x86_64
    cp32-abi3-manylinux1_x86_64
    cp32-abi3-linux_x86_64
    py39-none-manylinux_2_28_x86_64
    py39-none-manylinux_2_27_x86_64
    py39-none-manylinux_2_26_x86_64
    py39-none-manylinux_2_25_x86_64
    py39-none-manylinux_2_24_x86_64
    py39-none-manylinux_2_23_x86_64
    py39-none-manylinux_2_22_x86_64
    py39-none-manylinux_2_21_x86_64
    py39-none-manylinux_2_20_x86_64
    py39-none-manylinux_2_19_x86_64
    py39-none-manylinux_2_18_x86_64
    py39-none-manylinux_2_17_x86_64
    py39-none-manylinux2014_x86_64
    py39-none-manylinux_2_16_x86_64
    py39-none-manylinux_2_15_x86_64
    py39-none-manylinux_2_14_x86_64
    py39-none-manylinux_2_13_x86_64
    py39-none-manylinux_2_12_x86_64
    py39-none-manylinux2010_x86_64
    py39-none-manylinux_2_11_x86_64
    py39-none-manylinux_2_10_x86_64
    py39-none-manylinux_2_9_x86_64
    py39-none-manylinux_2_8_x86_64
    py39-none-manylinux_2_7_x86_64
    py39-none-manylinux_2_6_x86_64
    py39-none-manylinux_2_5_x86_64
    py39-none-manylinux1_x86_64
    py39-none-linux_x86_64
    py3-none-manylinux_2_28_x86_64
    py3-none-manylinux_2_27_x86_64
    py3-none-manylinux_2_26_x86_64
    py3-none-manylinux_2_25_x86_64
    py3-none-manylinux_2_24_x86_64
    py3-none-manylinux_2_23_x86_64
    py3-none-manylinux_2_22_x86_64
    py3-none-manylinux_2_21_x86_64
    py3-none-manylinux_2_20_x86_64
    py3-none-manylinux_2_19_x86_64
    py3-none-manylinux_2_18_x86_64
    py3-none-manylinux_2_17_x86_64
    py3-none-manylinux2014_x86_64
    py3-none-manylinux_2_16_x86_64
    py3-none-manylinux_2_15_x86_64
    py3-none-manylinux_2_14_x86_64
    py3-none-manylinux_2_13_x86_64
    py3-none-manylinux_2_12_x86_64
    py3-none-manylinux2010_x86_64
    py3-none-manylinux_2_11_x86_64
    py3-none-manylinux_2_10_x86_64
    py3-none-manylinux_2_9_x86_64
    py3-none-manylinux_2_8_x86_64
    py3-none-manylinux_2_7_x86_64
    py3-none-manylinux_2_6_x86_64
    py3-none-manylinux_2_5_x86_64
    py3-none-manylinux1_x86_64
    py3-none-linux_x86_64
    py38-none-manylinux_2_28_x86_64
    py38-none-manylinux_2_27_x86_64
    py38-none-manylinux_2_26_x86_64
    py38-none-manylinux_2_25_x86_64
    py38-none-manylinux_2_24_x86_64
    py38-none-manylinux_2_23_x86_64
    py38-none-manylinux_2_22_x86_64
    py38-none-manylinux_2_21_x86_64
    py38-none-manylinux_2_20_x86_64
    py38-none-manylinux_2_19_x86_64
    py38-none-manylinux_2_18_x86_64
    py38-none-manylinux_2_17_x86_64
    py38-none-manylinux2014_x86_64
    py38-none-manylinux_2_16_x86_64
    py38-none-manylinux_2_15_x86_64
    py38-none-manylinux_2_14_x86_64
    py38-none-manylinux_2_13_x86_64
    py38-none-manylinux_2_12_x86_64
    py38-none-manylinux2010_x86_64
    py38-none-manylinux_2_11_x86_64
    py38-none-manylinux_2_10_x86_64
    py38-none-manylinux_2_9_x86_64
    py38-none-manylinux_2_8_x86_64
    py38-none-manylinux_2_7_x86_64
    py38-none-manylinux_2_6_x86_64
    py38-none-manylinux_2_5_x86_64
    py38-none-manylinux1_x86_64
    py38-none-linux_x86_64
    py37-none-manylinux_2_28_x86_64
    py37-none-manylinux_2_27_x86_64
    py37-none-manylinux_2_26_x86_64
    py37-none-manylinux_2_25_x86_64
    py37-none-manylinux_2_24_x86_64
    py37-none-manylinux_2_23_x86_64
    py37-none-manylinux_2_22_x86_64
    py37-none-manylinux_2_21_x86_64
    py37-none-manylinux_2_20_x86_64
    py37-none-manylinux_2_19_x86_64
    py37-none-manylinux_2_18_x86_64
    py37-none-manylinux_2_17_x86_64
    py37-none-manylinux2014_x86_64
    py37-none-manylinux_2_16_x86_64
    py37-none-manylinux_2_15_x86_64
    py37-none-manylinux_2_14_x86_64
    py37-none-manylinux_2_13_x86_64
    py37-none-manylinux_2_12_x86_64
    py37-none-manylinux2010_x86_64
    py37-none-manylinux_2_11_x86_64
    py37-none-manylinux_2_10_x86_64
    py37-none-manylinux_2_9_x86_64
    py37-none-manylinux_2_8_x86_64
    py37-none-manylinux_2_7_x86_64
    py37-none-manylinux_2_6_x86_64
    py37-none-manylinux_2_5_x86_64
    py37-none-manylinux1_x86_64
    py37-none-linux_x86_64
    py36-none-manylinux_2_28_x86_64
    py36-none-manylinux_2_27_x86_64
    py36-none-manylinux_2_26_x86_64
    py36-none-manylinux_2_25_x86_64
    py36-none-manylinux_2_24_x86_64
    py36-none-manylinux_2_23_x86_64
    py36-none-manylinux_2_22_x86_64
    py36-none-manylinux_2_21_x86_64
    py36-none-manylinux_2_20_x86_64
    py36-none-manylinux_2_19_x86_64
    py36-none-manylinux_2_18_x86_64
    py36-none-manylinux_2_17_x86_64
    py36-none-manylinux2014_x86_64
    py36-none-manylinux_2_16_x86_64
    py36-none-manylinux_2_15_x86_64
    py36-none-manylinux_2_14_x86_64
    py36-none-manylinux_2_13_x86_64
    py36-none-manylinux_2_12_x86_64
    py36-none-manylinux2010_x86_64
    py36-none-manylinux_2_11_x86_64
    py36-none-manylinux_2_10_x86_64
    py36-none-manylinux_2_9_x86_64
    py36-none-manylinux_2_8_x86_64
    py36-none-manylinux_2_7_x86_64
    py36-none-manylinux_2_6_x86_64
    py36-none-manylinux_2_5_x86_64
    py36-none-manylinux1_x86_64
    py36-none-linux_x86_64
    py35-none-manylinux_2_28_x86_64
    py35-none-manylinux_2_27_x86_64
    py35-none-manylinux_2_26_x86_64
    py35-none-manylinux_2_25_x86_64
    py35-none-manylinux_2_24_x86_64
    py35-none-manylinux_2_23_x86_64
    py35-none-manylinux_2_22_x86_64
    py35-none-manylinux_2_21_x86_64
    py35-none-manylinux_2_20_x86_64
    py35-none-manylinux_2_19_x86_64
    py35-none-manylinux_2_18_x86_64
    py35-none-manylinux_2_17_x86_64
    py35-none-manylinux2014_x86_64
    py35-none-manylinux_2_16_x86_64
    py35-none-manylinux_2_15_x86_64
    py35-none-manylinux_2_14_x86_64
    py35-none-manylinux_2_13_x86_64
    py35-none-manylinux_2_12_x86_64
    py35-none-manylinux2010_x86_64
    py35-none-manylinux_2_11_x86_64
    py35-none-manylinux_2_10_x86_64
    py35-none-manylinux_2_9_x86_64
    py35-none-manylinux_2_8_x86_64
    py35-none-manylinux_2_7_x86_64
    py35-none-manylinux_2_6_x86_64
    py35-none-manylinux_2_5_x86_64
    py35-none-manylinux1_x86_64
    py35-none-linux_x86_64
    py34-none-manylinux_2_28_x86_64
    py34-none-manylinux_2_27_x86_64
    py34-none-manylinux_2_26_x86_64
    py34-none-manylinux_2_25_x86_64
    py34-none-manylinux_2_24_x86_64
    py34-none-manylinux_2_23_x86_64
    py34-none-manylinux_2_22_x86_64
    py34-none-manylinux_2_21_x86_64
    py34-none-manylinux_2_20_x86_64
    py34-none-manylinux_2_19_x86_64
    py34-none-manylinux_2_18_x86_64
    py34-none-manylinux_2_17_x86_64
    py34-none-manylinux2014_x86_64
    py34-none-manylinux_2_16_x86_64
    py34-none-manylinux_2_15_x86_64
    py34-none-manylinux_2_14_x86_64
    py34-none-manylinux_2_13_x86_64
    py34-none-manylinux_2_12_x86_64
    py34-none-manylinux2010_x86_64
    py34-none-manylinux_2_11_x86_64
    py34-none-manylinux_2_10_x86_64
    py34-none-manylinux_2_9_x86_64
    py34-none-manylinux_2_8_x86_64
    py34-none-manylinux_2_7_x86_64
    py34-none-manylinux_2_6_x86_64
    py34-none-manylinux_2_5_x86_64
    py34-none-manylinux1_x86_64
    py34-none-linux_x86_64
    py33-none-manylinux_2_28_x86_64
    py33-none-manylinux_2_27_x86_64
    py33-none-manylinux_2_26_x86_64
    py33-none-manylinux_2_25_x86_64
    py33-none-manylinux_2_24_x86_64
    py33-none-manylinux_2_23_x86_64
    py33-none-manylinux_2_22_x86_64
    py33-none-manylinux_2_21_x86_64
    py33-none-manylinux_2_20_x86_64
    py33-none-manylinux_2_19_x86_64
    py33-none-manylinux_2_18_x86_64
    py33-none-manylinux_2_17_x86_64
    py33-none-manylinux2014_x86_64
    py33-none-manylinux_2_16_x86_64
    py33-none-manylinux_2_15_x86_64
    py33-none-manylinux_2_14_x86_64
    py33-none-manylinux_2_13_x86_64
    py33-none-manylinux_2_12_x86_64
    py33-none-manylinux2010_x86_64
    py33-none-manylinux_2_11_x86_64
    py33-none-manylinux_2_10_x86_64
    py33-none-manylinux_2_9_x86_64
    py33-none-manylinux_2_8_x86_64
    py33-none-manylinux_2_7_x86_64
    py33-none-manylinux_2_6_x86_64
    py33-none-manylinux_2_5_x86_64
    py33-none-manylinux1_x86_64
    py33-none-linux_x86_64
    py32-none-manylinux_2_28_x86_64
    py32-none-manylinux_2_27_x86_64
    py32-none-manylinux_2_26_x86_64
    py32-none-manylinux_2_25_x86_64
    py32-none-manylinux_2_24_x86_64
    py32-none-manylinux_2_23_x86_64
    py32-none-manylinux_2_22_x86_64
    py32-none-manylinux_2_21_x86_64
    py32-none-manylinux_2_20_x86_64
    py32-none-manylinux_2_19_x86_64
    py32-none-manylinux_2_18_x86_64
    py32-none-manylinux_2_17_x86_64
    py32-none-manylinux2014_x86_64
    py32-none-manylinux_2_16_x86_64
    py32-none-manylinux_2_15_x86_64
    py32-none-manylinux_2_14_x86_64
    py32-none-manylinux_2_13_x86_64
    py32-none-manylinux_2_12_x86_64
    py32-none-manylinux2010_x86_64
    py32-none-manylinux_2_11_x86_64
    py32-none-manylinux_2_10_x86_64
    py32-none-manylinux_2_9_x86_64
    py32-none-manylinux_2_8_x86_64
    py32-none-manylinux_2_7_x86_64
    py32-none-manylinux_2_6_x86_64
    py32-none-manylinux_2_5_x86_64
    py32-none-manylinux1_x86_64
    py32-none-linux_x86_64
    py31-none-manylinux_2_28_x86_64
    py31-none-manylinux_2_27_x86_64
    py31-none-manylinux_2_26_x86_64
    py31-none-manylinux_2_25_x86_64
    py31-none-manylinux_2_24_x86_64
    py31-none-manylinux_2_23_x86_64
    py31-none-manylinux_2_22_x86_64
    py31-none-manylinux_2_21_x86_64
    py31-none-manylinux_2_20_x86_64
    py31-none-manylinux_2_19_x86_64
    py31-none-manylinux_2_18_x86_64
    py31-none-manylinux_2_17_x86_64
    py31-none-manylinux2014_x86_64
    py31-none-manylinux_2_16_x86_64
    py31-none-manylinux_2_15_x86_64
    py31-none-manylinux_2_14_x86_64
    py31-none-manylinux_2_13_x86_64
    py31-none-manylinux_2_12_x86_64
    py31-none-manylinux2010_x86_64
    py31-none-manylinux_2_11_x86_64
    py31-none-manylinux_2_10_x86_64
    py31-none-manylinux_2_9_x86_64
    py31-none-manylinux_2_8_x86_64
    py31-none-manylinux_2_7_x86_64
    py31-none-manylinux_2_6_x86_64
    py31-none-manylinux_2_5_x86_64
    py31-none-manylinux1_x86_64
    py31-none-linux_x86_64
    py30-none-manylinux_2_28_x86_64
    py30-none-manylinux_2_27_x86_64
    py30-none-manylinux_2_26_x86_64
    py30-none-manylinux_2_25_x86_64
    py30-none-manylinux_2_24_x86_64
    py30-none-manylinux_2_23_x86_64
    py30-none-manylinux_2_22_x86_64
    py30-none-manylinux_2_21_x86_64
    py30-none-manylinux_2_20_x86_64
    py30-none-manylinux_2_19_x86_64
    py30-none-manylinux_2_18_x86_64
    py30-none-manylinux_2_17_x86_64
    py30-none-manylinux2014_x86_64
    py30-none-manylinux_2_16_x86_64
    py30-none-manylinux_2_15_x86_64
    py30-none-manylinux_2_14_x86_64
    py30-none-manylinux_2_13_x86_64
    py30-none-manylinux_2_12_x86_64
    py30-none-manylinux2010_x86_64
    py30-none-manylinux_2_11_x86_64
    py30-none-manylinux_2_10_x86_64
    py30-none-manylinux_2_9_x86_64
    py30-none-manylinux_2_8_x86_64
    py30-none-manylinux_2_7_x86_64
    py30-none-manylinux_2_6_x86_64
    py30-none-manylinux_2_5_x86_64
    py30-none-manylinux1_x86_64
    py30-none-linux_x86_64
    cp39-none-any
    py39-none-any
    py3-none-any
    py38-none-any
    py37-none-any
    py36-none-any
    py35-none-any
    py34-none-any
    py33-none-any
    py32-none-any
    py31-none-any
    py30-none-any
    "###
        );
    }

    #[test]
    fn test_system_tags_macos() {
        let tags = Tags::from_env(
            &Platform::new(
                Os::Macos {
                    major: 14,
                    minor: 0,
                },
                Arch::Aarch64,
            ),
            (3, 9),
            "cpython",
            (3, 9),
            false,
            false,
        )
        .unwrap();
        assert_snapshot!(
            tags,
            @r###"
    cp39-cp39-macosx_14_0_arm64
    cp39-cp39-macosx_14_0_universal2
    cp39-cp39-macosx_13_0_arm64
    cp39-cp39-macosx_13_0_universal2
    cp39-cp39-macosx_12_0_arm64
    cp39-cp39-macosx_12_0_universal2
    cp39-cp39-macosx_11_0_arm64
    cp39-cp39-macosx_11_0_universal2
    cp39-cp39-macosx_10_16_universal2
    cp39-cp39-macosx_10_15_universal2
    cp39-cp39-macosx_10_14_universal2
    cp39-cp39-macosx_10_13_universal2
    cp39-cp39-macosx_10_12_universal2
    cp39-cp39-macosx_10_11_universal2
    cp39-cp39-macosx_10_10_universal2
    cp39-cp39-macosx_10_9_universal2
    cp39-cp39-macosx_10_8_universal2
    cp39-cp39-macosx_10_7_universal2
    cp39-cp39-macosx_10_6_universal2
    cp39-cp39-macosx_10_5_universal2
    cp39-cp39-macosx_10_4_universal2
    cp39-abi3-macosx_14_0_arm64
    cp39-abi3-macosx_14_0_universal2
    cp39-abi3-macosx_13_0_arm64
    cp39-abi3-macosx_13_0_universal2
    cp39-abi3-macosx_12_0_arm64
    cp39-abi3-macosx_12_0_universal2
    cp39-abi3-macosx_11_0_arm64
    cp39-abi3-macosx_11_0_universal2
    cp39-abi3-macosx_10_16_universal2
    cp39-abi3-macosx_10_15_universal2
    cp39-abi3-macosx_10_14_universal2
    cp39-abi3-macosx_10_13_universal2
    cp39-abi3-macosx_10_12_universal2
    cp39-abi3-macosx_10_11_universal2
    cp39-abi3-macosx_10_10_universal2
    cp39-abi3-macosx_10_9_universal2
    cp39-abi3-macosx_10_8_universal2
    cp39-abi3-macosx_10_7_universal2
    cp39-abi3-macosx_10_6_universal2
    cp39-abi3-macosx_10_5_universal2
    cp39-abi3-macosx_10_4_universal2
    cp39-none-macosx_14_0_arm64
    cp39-none-macosx_14_0_universal2
    cp39-none-macosx_13_0_arm64
    cp39-none-macosx_13_0_universal2
    cp39-none-macosx_12_0_arm64
    cp39-none-macosx_12_0_universal2
    cp39-none-macosx_11_0_arm64
    cp39-none-macosx_11_0_universal2
    cp39-none-macosx_10_16_universal2
    cp39-none-macosx_10_15_universal2
    cp39-none-macosx_10_14_universal2
    cp39-none-macosx_10_13_universal2
    cp39-none-macosx_10_12_universal2
    cp39-none-macosx_10_11_universal2
    cp39-none-macosx_10_10_universal2
    cp39-none-macosx_10_9_universal2
    cp39-none-macosx_10_8_universal2
    cp39-none-macosx_10_7_universal2
    cp39-none-macosx_10_6_universal2
    cp39-none-macosx_10_5_universal2
    cp39-none-macosx_10_4_universal2
    cp38-abi3-macosx_14_0_arm64
    cp38-abi3-macosx_14_0_universal2
    cp38-abi3-macosx_13_0_arm64
    cp38-abi3-macosx_13_0_universal2
    cp38-abi3-macosx_12_0_arm64
    cp38-abi3-macosx_12_0_universal2
    cp38-abi3-macosx_11_0_arm64
    cp38-abi3-macosx_11_0_universal2
    cp38-abi3-macosx_10_16_universal2
    cp38-abi3-macosx_10_15_universal2
    cp38-abi3-macosx_10_14_universal2
    cp38-abi3-macosx_10_13_universal2
    cp38-abi3-macosx_10_12_universal2
    cp38-abi3-macosx_10_11_universal2
    cp38-abi3-macosx_10_10_universal2
    cp38-abi3-macosx_10_9_universal2
    cp38-abi3-macosx_10_8_universal2
    cp38-abi3-macosx_10_7_universal2
    cp38-abi3-macosx_10_6_universal2
    cp38-abi3-macosx_10_5_universal2
    cp38-abi3-macosx_10_4_universal2
    cp37-abi3-macosx_14_0_arm64
    cp37-abi3-macosx_14_0_universal2
    cp37-abi3-macosx_13_0_arm64
    cp37-abi3-macosx_13_0_universal2
    cp37-abi3-macosx_12_0_arm64
    cp37-abi3-macosx_12_0_universal2
    cp37-abi3-macosx_11_0_arm64
    cp37-abi3-macosx_11_0_universal2
    cp37-abi3-macosx_10_16_universal2
    cp37-abi3-macosx_10_15_universal2
    cp37-abi3-macosx_10_14_universal2
    cp37-abi3-macosx_10_13_universal2
    cp37-abi3-macosx_10_12_universal2
    cp37-abi3-macosx_10_11_universal2
    cp37-abi3-macosx_10_10_universal2
    cp37-abi3-macosx_10_9_universal2
    cp37-abi3-macosx_10_8_universal2
    cp37-abi3-macosx_10_7_universal2
    cp37-abi3-macosx_10_6_universal2
    cp37-abi3-macosx_10_5_universal2
    cp37-abi3-macosx_10_4_universal2
    cp36-abi3-macosx_14_0_arm64
    cp36-abi3-macosx_14_0_universal2
    cp36-abi3-macosx_13_0_arm64
    cp36-abi3-macosx_13_0_universal2
    cp36-abi3-macosx_12_0_arm64
    cp36-abi3-macosx_12_0_universal2
    cp36-abi3-macosx_11_0_arm64
    cp36-abi3-macosx_11_0_universal2
    cp36-abi3-macosx_10_16_universal2
    cp36-abi3-macosx_10_15_universal2
    cp36-abi3-macosx_10_14_universal2
    cp36-abi3-macosx_10_13_universal2
    cp36-abi3-macosx_10_12_universal2
    cp36-abi3-macosx_10_11_universal2
    cp36-abi3-macosx_10_10_universal2
    cp36-abi3-macosx_10_9_universal2
    cp36-abi3-macosx_10_8_universal2
    cp36-abi3-macosx_10_7_universal2
    cp36-abi3-macosx_10_6_universal2
    cp36-abi3-macosx_10_5_universal2
    cp36-abi3-macosx_10_4_universal2
    cp35-abi3-macosx_14_0_arm64
    cp35-abi3-macosx_14_0_universal2
    cp35-abi3-macosx_13_0_arm64
    cp35-abi3-macosx_13_0_universal2
    cp35-abi3-macosx_12_0_arm64
    cp35-abi3-macosx_12_0_universal2
    cp35-abi3-macosx_11_0_arm64
    cp35-abi3-macosx_11_0_universal2
    cp35-abi3-macosx_10_16_universal2
    cp35-abi3-macosx_10_15_universal2
    cp35-abi3-macosx_10_14_universal2
    cp35-abi3-macosx_10_13_universal2
    cp35-abi3-macosx_10_12_universal2
    cp35-abi3-macosx_10_11_universal2
    cp35-abi3-macosx_10_10_universal2
    cp35-abi3-macosx_10_9_universal2
    cp35-abi3-macosx_10_8_universal2
    cp35-abi3-macosx_10_7_universal2
    cp35-abi3-macosx_10_6_universal2
    cp35-abi3-macosx_10_5_universal2
    cp35-abi3-macosx_10_4_universal2
    cp34-abi3-macosx_14_0_arm64
    cp34-abi3-macosx_14_0_universal2
    cp34-abi3-macosx_13_0_arm64
    cp34-abi3-macosx_13_0_universal2
    cp34-abi3-macosx_12_0_arm64
    cp34-abi3-macosx_12_0_universal2
    cp34-abi3-macosx_11_0_arm64
    cp34-abi3-macosx_11_0_universal2
    cp34-abi3-macosx_10_16_universal2
    cp34-abi3-macosx_10_15_universal2
    cp34-abi3-macosx_10_14_universal2
    cp34-abi3-macosx_10_13_universal2
    cp34-abi3-macosx_10_12_universal2
    cp34-abi3-macosx_10_11_universal2
    cp34-abi3-macosx_10_10_universal2
    cp34-abi3-macosx_10_9_universal2
    cp34-abi3-macosx_10_8_universal2
    cp34-abi3-macosx_10_7_universal2
    cp34-abi3-macosx_10_6_universal2
    cp34-abi3-macosx_10_5_universal2
    cp34-abi3-macosx_10_4_universal2
    cp33-abi3-macosx_14_0_arm64
    cp33-abi3-macosx_14_0_universal2
    cp33-abi3-macosx_13_0_arm64
    cp33-abi3-macosx_13_0_universal2
    cp33-abi3-macosx_12_0_arm64
    cp33-abi3-macosx_12_0_universal2
    cp33-abi3-macosx_11_0_arm64
    cp33-abi3-macosx_11_0_universal2
    cp33-abi3-macosx_10_16_universal2
    cp33-abi3-macosx_10_15_universal2
    cp33-abi3-macosx_10_14_universal2
    cp33-abi3-macosx_10_13_universal2
    cp33-abi3-macosx_10_12_universal2
    cp33-abi3-macosx_10_11_universal2
    cp33-abi3-macosx_10_10_universal2
    cp33-abi3-macosx_10_9_universal2
    cp33-abi3-macosx_10_8_universal2
    cp33-abi3-macosx_10_7_universal2
    cp33-abi3-macosx_10_6_universal2
    cp33-abi3-macosx_10_5_universal2
    cp33-abi3-macosx_10_4_universal2
    cp32-abi3-macosx_14_0_arm64
    cp32-abi3-macosx_14_0_universal2
    cp32-abi3-macosx_13_0_arm64
    cp32-abi3-macosx_13_0_universal2
    cp32-abi3-macosx_12_0_arm64
    cp32-abi3-macosx_12_0_universal2
    cp32-abi3-macosx_11_0_arm64
    cp32-abi3-macosx_11_0_universal2
    cp32-abi3-macosx_10_16_universal2
    cp32-abi3-macosx_10_15_universal2
    cp32-abi3-macosx_10_14_universal2
    cp32-abi3-macosx_10_13_universal2
    cp32-abi3-macosx_10_12_universal2
    cp32-abi3-macosx_10_11_universal2
    cp32-abi3-macosx_10_10_universal2
    cp32-abi3-macosx_10_9_universal2
    cp32-abi3-macosx_10_8_universal2
    cp32-abi3-macosx_10_7_universal2
    cp32-abi3-macosx_10_6_universal2
    cp32-abi3-macosx_10_5_universal2
    cp32-abi3-macosx_10_4_universal2
    py39-none-macosx_14_0_arm64
    py39-none-macosx_14_0_universal2
    py39-none-macosx_13_0_arm64
    py39-none-macosx_13_0_universal2
    py39-none-macosx_12_0_arm64
    py39-none-macosx_12_0_universal2
    py39-none-macosx_11_0_arm64
    py39-none-macosx_11_0_universal2
    py39-none-macosx_10_16_universal2
    py39-none-macosx_10_15_universal2
    py39-none-macosx_10_14_universal2
    py39-none-macosx_10_13_universal2
    py39-none-macosx_10_12_universal2
    py39-none-macosx_10_11_universal2
    py39-none-macosx_10_10_universal2
    py39-none-macosx_10_9_universal2
    py39-none-macosx_10_8_universal2
    py39-none-macosx_10_7_universal2
    py39-none-macosx_10_6_universal2
    py39-none-macosx_10_5_universal2
    py39-none-macosx_10_4_universal2
    py3-none-macosx_14_0_arm64
    py3-none-macosx_14_0_universal2
    py3-none-macosx_13_0_arm64
    py3-none-macosx_13_0_universal2
    py3-none-macosx_12_0_arm64
    py3-none-macosx_12_0_universal2
    py3-none-macosx_11_0_arm64
    py3-none-macosx_11_0_universal2
    py3-none-macosx_10_16_universal2
    py3-none-macosx_10_15_universal2
    py3-none-macosx_10_14_universal2
    py3-none-macosx_10_13_universal2
    py3-none-macosx_10_12_universal2
    py3-none-macosx_10_11_universal2
    py3-none-macosx_10_10_universal2
    py3-none-macosx_10_9_universal2
    py3-none-macosx_10_8_universal2
    py3-none-macosx_10_7_universal2
    py3-none-macosx_10_6_universal2
    py3-none-macosx_10_5_universal2
    py3-none-macosx_10_4_universal2
    py38-none-macosx_14_0_arm64
    py38-none-macosx_14_0_universal2
    py38-none-macosx_13_0_arm64
    py38-none-macosx_13_0_universal2
    py38-none-macosx_12_0_arm64
    py38-none-macosx_12_0_universal2
    py38-none-macosx_11_0_arm64
    py38-none-macosx_11_0_universal2
    py38-none-macosx_10_16_universal2
    py38-none-macosx_10_15_universal2
    py38-none-macosx_10_14_universal2
    py38-none-macosx_10_13_universal2
    py38-none-macosx_10_12_universal2
    py38-none-macosx_10_11_universal2
    py38-none-macosx_10_10_universal2
    py38-none-macosx_10_9_universal2
    py38-none-macosx_10_8_universal2
    py38-none-macosx_10_7_universal2
    py38-none-macosx_10_6_universal2
    py38-none-macosx_10_5_universal2
    py38-none-macosx_10_4_universal2
    py37-none-macosx_14_0_arm64
    py37-none-macosx_14_0_universal2
    py37-none-macosx_13_0_arm64
    py37-none-macosx_13_0_universal2
    py37-none-macosx_12_0_arm64
    py37-none-macosx_12_0_universal2
    py37-none-macosx_11_0_arm64
    py37-none-macosx_11_0_universal2
    py37-none-macosx_10_16_universal2
    py37-none-macosx_10_15_universal2
    py37-none-macosx_10_14_universal2
    py37-none-macosx_10_13_universal2
    py37-none-macosx_10_12_universal2
    py37-none-macosx_10_11_universal2
    py37-none-macosx_10_10_universal2
    py37-none-macosx_10_9_universal2
    py37-none-macosx_10_8_universal2
    py37-none-macosx_10_7_universal2
    py37-none-macosx_10_6_universal2
    py37-none-macosx_10_5_universal2
    py37-none-macosx_10_4_universal2
    py36-none-macosx_14_0_arm64
    py36-none-macosx_14_0_universal2
    py36-none-macosx_13_0_arm64
    py36-none-macosx_13_0_universal2
    py36-none-macosx_12_0_arm64
    py36-none-macosx_12_0_universal2
    py36-none-macosx_11_0_arm64
    py36-none-macosx_11_0_universal2
    py36-none-macosx_10_16_universal2
    py36-none-macosx_10_15_universal2
    py36-none-macosx_10_14_universal2
    py36-none-macosx_10_13_universal2
    py36-none-macosx_10_12_universal2
    py36-none-macosx_10_11_universal2
    py36-none-macosx_10_10_universal2
    py36-none-macosx_10_9_universal2
    py36-none-macosx_10_8_universal2
    py36-none-macosx_10_7_universal2
    py36-none-macosx_10_6_universal2
    py36-none-macosx_10_5_universal2
    py36-none-macosx_10_4_universal2
    py35-none-macosx_14_0_arm64
    py35-none-macosx_14_0_universal2
    py35-none-macosx_13_0_arm64
    py35-none-macosx_13_0_universal2
    py35-none-macosx_12_0_arm64
    py35-none-macosx_12_0_universal2
    py35-none-macosx_11_0_arm64
    py35-none-macosx_11_0_universal2
    py35-none-macosx_10_16_universal2
    py35-none-macosx_10_15_universal2
    py35-none-macosx_10_14_universal2
    py35-none-macosx_10_13_universal2
    py35-none-macosx_10_12_universal2
    py35-none-macosx_10_11_universal2
    py35-none-macosx_10_10_universal2
    py35-none-macosx_10_9_universal2
    py35-none-macosx_10_8_universal2
    py35-none-macosx_10_7_universal2
    py35-none-macosx_10_6_universal2
    py35-none-macosx_10_5_universal2
    py35-none-macosx_10_4_universal2
    py34-none-macosx_14_0_arm64
    py34-none-macosx_14_0_universal2
    py34-none-macosx_13_0_arm64
    py34-none-macosx_13_0_universal2
    py34-none-macosx_12_0_arm64
    py34-none-macosx_12_0_universal2
    py34-none-macosx_11_0_arm64
    py34-none-macosx_11_0_universal2
    py34-none-macosx_10_16_universal2
    py34-none-macosx_10_15_universal2
    py34-none-macosx_10_14_universal2
    py34-none-macosx_10_13_universal2
    py34-none-macosx_10_12_universal2
    py34-none-macosx_10_11_universal2
    py34-none-macosx_10_10_universal2
    py34-none-macosx_10_9_universal2
    py34-none-macosx_10_8_universal2
    py34-none-macosx_10_7_universal2
    py34-none-macosx_10_6_universal2
    py34-none-macosx_10_5_universal2
    py34-none-macosx_10_4_universal2
    py33-none-macosx_14_0_arm64
    py33-none-macosx_14_0_universal2
    py33-none-macosx_13_0_arm64
    py33-none-macosx_13_0_universal2
    py33-none-macosx_12_0_arm64
    py33-none-macosx_12_0_universal2
    py33-none-macosx_11_0_arm64
    py33-none-macosx_11_0_universal2
    py33-none-macosx_10_16_universal2
    py33-none-macosx_10_15_universal2
    py33-none-macosx_10_14_universal2
    py33-none-macosx_10_13_universal2
    py33-none-macosx_10_12_universal2
    py33-none-macosx_10_11_universal2
    py33-none-macosx_10_10_universal2
    py33-none-macosx_10_9_universal2
    py33-none-macosx_10_8_universal2
    py33-none-macosx_10_7_universal2
    py33-none-macosx_10_6_universal2
    py33-none-macosx_10_5_universal2
    py33-none-macosx_10_4_universal2
    py32-none-macosx_14_0_arm64
    py32-none-macosx_14_0_universal2
    py32-none-macosx_13_0_arm64
    py32-none-macosx_13_0_universal2
    py32-none-macosx_12_0_arm64
    py32-none-macosx_12_0_universal2
    py32-none-macosx_11_0_arm64
    py32-none-macosx_11_0_universal2
    py32-none-macosx_10_16_universal2
    py32-none-macosx_10_15_universal2
    py32-none-macosx_10_14_universal2
    py32-none-macosx_10_13_universal2
    py32-none-macosx_10_12_universal2
    py32-none-macosx_10_11_universal2
    py32-none-macosx_10_10_universal2
    py32-none-macosx_10_9_universal2
    py32-none-macosx_10_8_universal2
    py32-none-macosx_10_7_universal2
    py32-none-macosx_10_6_universal2
    py32-none-macosx_10_5_universal2
    py32-none-macosx_10_4_universal2
    py31-none-macosx_14_0_arm64
    py31-none-macosx_14_0_universal2
    py31-none-macosx_13_0_arm64
    py31-none-macosx_13_0_universal2
    py31-none-macosx_12_0_arm64
    py31-none-macosx_12_0_universal2
    py31-none-macosx_11_0_arm64
    py31-none-macosx_11_0_universal2
    py31-none-macosx_10_16_universal2
    py31-none-macosx_10_15_universal2
    py31-none-macosx_10_14_universal2
    py31-none-macosx_10_13_universal2
    py31-none-macosx_10_12_universal2
    py31-none-macosx_10_11_universal2
    py31-none-macosx_10_10_universal2
    py31-none-macosx_10_9_universal2
    py31-none-macosx_10_8_universal2
    py31-none-macosx_10_7_universal2
    py31-none-macosx_10_6_universal2
    py31-none-macosx_10_5_universal2
    py31-none-macosx_10_4_universal2
    py30-none-macosx_14_0_arm64
    py30-none-macosx_14_0_universal2
    py30-none-macosx_13_0_arm64
    py30-none-macosx_13_0_universal2
    py30-none-macosx_12_0_arm64
    py30-none-macosx_12_0_universal2
    py30-none-macosx_11_0_arm64
    py30-none-macosx_11_0_universal2
    py30-none-macosx_10_16_universal2
    py30-none-macosx_10_15_universal2
    py30-none-macosx_10_14_universal2
    py30-none-macosx_10_13_universal2
    py30-none-macosx_10_12_universal2
    py30-none-macosx_10_11_universal2
    py30-none-macosx_10_10_universal2
    py30-none-macosx_10_9_universal2
    py30-none-macosx_10_8_universal2
    py30-none-macosx_10_7_universal2
    py30-none-macosx_10_6_universal2
    py30-none-macosx_10_5_universal2
    py30-none-macosx_10_4_universal2
    cp39-none-any
    py39-none-any
    py3-none-any
    py38-none-any
    py37-none-any
    py36-none-any
    py35-none-any
    py34-none-any
    py33-none-any
    py32-none-any
    py31-none-any
    py30-none-any
    "###
        );
    }
}
