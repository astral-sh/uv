//! Host system information (OS type, kernel release, distro metadata).

use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

/// The operating system type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsType {
    /// Linux, with the kernel type from `/proc/sys/kernel/ostype` (typically `"Linux"`).
    Linux(String),
    /// macOS / Darwin.
    Darwin,
    /// Windows NT.
    WindowsNt,
}

impl OsType {
    /// Returns the operating system type for the current host.
    ///
    /// Returns `None` on unsupported platforms.
    pub fn from_env() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            fs_err::read_to_string("/proc/sys/kernel/ostype")
                .ok()
                .map(|s| Self::Linux(s.trim().to_string()))
        }
        #[cfg(target_os = "macos")]
        {
            Some(Self::Darwin)
        }
        #[cfg(target_os = "windows")]
        {
            Some(Self::WindowsNt)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            None
        }
    }
}

impl fmt::Display for OsType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Linux(os_type) => f.write_str(os_type),
            Self::Darwin => f.write_str("Darwin"),
            Self::WindowsNt => f.write_str("Windows_NT"),
        }
    }
}

/// The OS kernel release version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsRelease {
    /// Unix kernel release from `uname -r` (e.g., `"6.8.0-90-generic"`).
    Unix(String),
    /// Windows build number from the registry (e.g., `"22631"`).
    Windows(String),
}

impl OsRelease {
    /// Returns the OS kernel release for the current host.
    ///
    /// Returns `None` on unsupported platforms or if the release cannot be read.
    pub fn from_env() -> Option<Self> {
        #[cfg(unix)]
        {
            let uname = rustix::system::uname();
            let release = uname.release().to_str().ok()?;
            Some(Self::Unix(release.to_string()))
        }
        #[cfg(windows)]
        {
            let key = windows_registry::LOCAL_MACHINE
                .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
                .ok()?;
            Some(Self::Windows(key.get_string("CurrentBuildNumber").ok()?))
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }
}

impl fmt::Display for OsRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(release) => f.write_str(release),
            Self::Windows(build) => f.write_str(build),
        }
    }
}

/// Parsed fields from `/etc/os-release`.
#[derive(Debug, Clone, Default)]
pub struct LinuxOsRelease {
    /// Distribution name (e.g., `"Ubuntu"`).
    pub name: Option<String>,
    /// Version identifier (e.g., `"22.04"`).
    pub version_id: Option<String>,
    /// Version codename (e.g., `"jammy"`).
    pub version_codename: Option<String>,
}

impl LinuxOsRelease {
    /// Reads and parses `/etc/os-release` on Linux, falling back to `/usr/lib/os-release`.
    ///
    /// See <https://www.freedesktop.org/software/systemd/man/latest/os-release.html>.
    ///
    /// Returns `None` on non-Linux platforms or if the file cannot be read.
    pub fn from_env() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            let content = fs_err::read_to_string("/etc/os-release")
                .or_else(|_| fs_err::read_to_string("/usr/lib/os-release"))
                .ok()?;
            Some(content.parse().unwrap())
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
}

impl FromStr for LinuxOsRelease {
    type Err = Infallible;

    /// Parse the contents of an os-release file (KEY=VALUE format, optionally quoted).
    fn from_str(contents: &str) -> Result<Self, Self::Err> {
        let mut release = Self::default();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = unquote(value);
            match key {
                "NAME" => release.name = Some(value.to_string()),
                "VERSION_ID" => release.version_id = Some(value.to_string()),
                "VERSION_CODENAME" => release.version_codename = Some(value.to_string()),
                _ => {}
            }
        }
        Ok(release)
    }
}

/// Strip matching single or double quotes from a value.
fn unquote(s: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(quote).and_then(|s| s.strip_suffix(quote)) {
            return inner;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::*;

    #[test]
    fn test_parse_os_release_ubuntu() {
        let contents = "\
NAME=\"Ubuntu\"
VERSION_ID=\"22.04\"
VERSION_CODENAME=jammy
ID=ubuntu
";
        let release: LinuxOsRelease = contents.parse().unwrap();
        assert_debug_snapshot!(release, @r#"
        LinuxOsRelease {
            name: Some(
                "Ubuntu",
            ),
            version_id: Some(
                "22.04",
            ),
            version_codename: Some(
                "jammy",
            ),
        }
        "#);
    }

    #[test]
    fn test_parse_os_release_empty() {
        let release: LinuxOsRelease = "".parse().unwrap();
        assert_eq!(release.name, None);
        assert_eq!(release.version_id, None);
        assert_eq!(release.version_codename, None);
    }

    #[test]
    fn test_parse_os_release_comments_and_blanks() {
        let contents = "\
# This is a comment

NAME='Fedora Linux'
VERSION_ID=40
";
        let release: LinuxOsRelease = contents.parse().unwrap();
        assert_eq!(release.name.as_deref(), Some("Fedora Linux"));
        assert_eq!(release.version_id.as_deref(), Some("40"));
        assert_eq!(release.version_codename, None);
    }

    #[test]
    fn test_unquote() {
        assert_eq!(unquote("\"hello\""), "hello");
        assert_eq!(unquote("'hello'"), "hello");
        assert_eq!(unquote("hello"), "hello");
        assert_eq!(unquote("\"\""), "");
        assert_eq!(unquote(""), "");
    }

    #[test]
    fn test_os_type_returns_value() {
        let os_type =
            OsType::from_env().expect("OsType should be available on supported platforms");
        #[cfg(target_os = "linux")]
        assert!(matches!(os_type, OsType::Linux(_)));
        #[cfg(target_os = "macos")]
        assert_eq!(os_type, OsType::Darwin);
        #[cfg(target_os = "windows")]
        assert_eq!(os_type, OsType::WindowsNt);
    }

    #[test]
    fn test_os_release_returns_value() {
        let os_release =
            OsRelease::from_env().expect("OsRelease should be available on supported platforms");
        #[cfg(unix)]
        assert!(matches!(os_release, OsRelease::Unix(_)));
        #[cfg(windows)]
        assert!(matches!(os_release, OsRelease::Windows(_)));
    }
}
