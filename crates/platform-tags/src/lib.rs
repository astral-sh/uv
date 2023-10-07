use puffin_platform::{Arch, Os, Platform, PlatformError};

/// A set of compatible tags for a given Python version and platform, in
/// (`python_tag`, `abi_tag`, `platform_tag`) format.
#[derive(Debug)]
pub struct Tags(Vec<(String, String, String)>);

impl Tags {
    /// Returns the compatible tags for the given Python version and platform.
    pub fn from_env(platform: &Platform, python_version: (u8, u8)) -> Result<Self, PlatformError> {
        let platform_tags = compatible_tags(platform)?;

        let mut tags = Vec::with_capacity(5 * platform_tags.len());

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
        Ok(Self(tags))
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String, String)> {
        self.0.iter()
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
