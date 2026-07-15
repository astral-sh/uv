use std::env;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::instrument;

use uv_fs::tempfile_in;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::{Os, Platform};
use uv_static::EnvVars;
use uv_version::version;

const RUSTC_VERSION_TIMEOUT: Duration = Duration::from_millis(50);
const RUSTC_VERSION_POLL_INTERVAL: Duration = Duration::from_millis(2);

/// A wrapper for a background worker (in a thread) that queries `rustc --version`.
#[derive(Debug, Clone)]
pub struct RustcVersion(Arc<RustcVersionInner>);

#[derive(Debug)]
struct RustcVersionInner {
    result: OnceLock<Option<String>>,
    deadline: Instant,
}

impl RustcVersion {
    /// Spawn a background query for `rustc --version`.
    #[must_use]
    pub fn spawn() -> Self {
        let deadline = Instant::now() + RUSTC_VERSION_TIMEOUT;
        let inner = Arc::new(RustcVersionInner {
            result: OnceLock::new(),
            deadline,
        });
        let worker = Arc::clone(&inner);
        if thread::Builder::new()
            .name("uv-rustc-version".to_string())
            .spawn(move || {
                let version = Self::query_version(deadline);
                let _ = worker.result.set(version);
            })
            .is_err()
        {
            let _ = inner.result.set(None);
        }
        Self(inner)
    }

    /// Create an already-resolved `rustc` version.
    #[must_use]
    pub fn ready(version: String) -> Self {
        Self(Arc::new(RustcVersionInner {
            result: OnceLock::from(Some(version)),
            deadline: Instant::now(),
        }))
    }

    fn query_version(deadline: Instant) -> Option<String> {
        if Instant::now() >= deadline {
            return None;
        }
        // Capture stdout in a regular file instead of a pipe. Since we only read after waiting
        // for `rustc`, a pipe could deadlock if its buffer fills or hang if a descendant inherits
        // the write end and keeps it open.
        let mut output = tempfile_in(&env::temp_dir()).ok()?;
        let mut child = Command::new("rustc")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(output.reopen().ok()?)
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let status = loop {
            if Instant::now() >= deadline {
                Self::kill_and_reap(&mut child);
                return None;
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(_) => {
                    Self::kill_and_reap(&mut child);
                    return None;
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            thread::sleep(remaining.min(RUSTC_VERSION_POLL_INTERVAL));
        };
        if !status.success() {
            return None;
        }

        let output_file = output.as_file_mut();
        output_file.seek(SeekFrom::Start(0)).ok()?;
        let mut stdout = Vec::new();
        output_file.read_to_end(&mut stdout).ok()?;
        if Instant::now() >= deadline {
            return None;
        }
        Self::parse_version(&stdout)
    }

    fn kill_and_reap(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    fn parse_version(output: &[u8]) -> Option<String> {
        let output = output.strip_prefix(b"rustc ")?;
        let version_length = output
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(output.len());
        if version_length == 0 {
            return None;
        }
        std::str::from_utf8(&output[..version_length])
            .ok()
            .map(str::to_owned)
    }

    pub(crate) fn get(&self) -> Option<&str> {
        loop {
            if let Some(version) = self.0.result.get() {
                return version.as_deref();
            }
            let now = Instant::now();
            if now >= self.0.deadline {
                return self.0.result.get_or_init(|| None).as_deref();
            }
            thread::sleep(
                self.0
                    .deadline
                    .duration_since(now)
                    .min(RUSTC_VERSION_POLL_INTERVAL),
            );
        }
    }
}

#[derive(Serialize)]
struct Installer {
    name: Option<String>,
    version: Option<String>,
    subcommand: Option<Vec<String>>,
}

#[derive(Serialize)]
struct Implementation {
    name: Option<String>,
    version: Option<String>,
}

#[derive(Serialize)]
struct Libc {
    lib: Option<String>,
    version: Option<String>,
}

#[derive(Serialize)]
struct Distro {
    name: Option<String>,
    version: Option<String>,
    id: Option<String>,
    libc: Option<Libc>,
}

#[derive(Serialize)]
struct System {
    name: Option<String>,
    release: Option<String>,
}

/// Linehaul structs were derived from
/// <https://github.com/pypi/linehaul-cloud-function/blob/1.0.1/linehaul/ua/datastructures.py>.
/// For the sake of parity, the nullability of all the values was kept intact.
#[derive(Serialize)]
pub(crate) struct LineHaul {
    installer: Option<Installer>,
    python: Option<String>,
    implementation: Option<Implementation>,
    distro: Option<Distro>,
    system: Option<System>,
    cpu: Option<String>,
    openssl_version: Option<String>,
    setuptools_version: Option<String>,
    rustc_version: Option<String>,
    ci: Option<bool>,
}

/// Implements Linehaul information format as defined by
/// <https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/session.py#L109>.
/// This metadata is added to the user agent to enrich PyPI statistics.
impl LineHaul {
    /// Initializes Linehaul information based on PEP 508 markers.
    #[instrument(name = "linehaul", skip_all)]
    pub(crate) fn new(
        markers: Option<&MarkerEnvironment>,
        platform: Option<&Platform>,
        subcommand: Option<Vec<String>>,
        rustc_version: Option<&str>,
    ) -> Self {
        // https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/session.py#L87
        let looks_like_ci = [
            EnvVars::BUILD_BUILDID,
            EnvVars::BUILD_ID,
            EnvVars::CI,
            EnvVars::PIP_IS_CI,
        ]
        .iter()
        .find_map(|&var_name| env::var(var_name).ok().map(|_| true));

        let libc = match platform.map(Platform::os) {
            Some(Os::Manylinux { major, minor }) => Some(Libc {
                lib: Some("glibc".to_string()),
                version: Some(format!("{major}.{minor}")),
            }),
            Some(Os::Musllinux { major, minor }) => Some(Libc {
                lib: Some("musl".to_string()),
                version: Some(format!("{major}.{minor}")),
            }),
            _ => None,
        };

        // Build Distro as Linehaul expects.
        let distro: Option<Distro> = if cfg!(target_os = "linux") {
            // Gather distribution info from /etc/os-release.
            uv_platform::LinuxOsRelease::from_env().map(|info| Distro {
                // e.g., Jammy, Focal, etc.
                id: info.version_codename,
                // e.g., Ubuntu, Fedora, etc.
                name: info.name,
                // e.g., 22.04, etc.
                version: info.version_id,
                // e.g., glibc 2.38, musl 1.2
                libc,
            })
        } else if cfg!(target_os = "macos") {
            let version = match platform.map(Platform::os) {
                Some(Os::Macos { major, minor }) => Some(format!("{major}.{minor}")),
                _ => None,
            };
            Some(Distro {
                // N/A
                id: None,
                // pip hardcodes distro name to macOS.
                name: Some("macOS".to_string()),
                // Same as python's platform.mac_ver()[0].
                version,
                // N/A
                libc: None,
            })
        } else {
            // Always empty on Windows.
            None
        };

        Self {
            installer: Option::from(Installer {
                name: Some("uv".to_string()),
                version: Some(version().to_string()),
                subcommand,
            }),
            python: markers.map(|markers| markers.python_full_version().version.to_string()),
            implementation: Option::from(Implementation {
                name: markers.map(|markers| markers.platform_python_implementation().to_string()),
                version: markers.map(|markers| markers.python_full_version().version.to_string()),
            }),
            distro,
            system: Option::from(System {
                name: markers.map(|markers| markers.platform_system().to_string()),
                release: markers.map(|markers| markers.platform_release().to_string()),
            }),
            cpu: markers.map(|markers| markers.platform_machine().to_string()),
            // Should probably always be None in uv.
            openssl_version: None,
            // Should probably always be None in uv.
            setuptools_version: None,
            rustc_version: rustc_version.map(str::to_owned),
            ci: looks_like_ci,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RustcVersion;

    #[test]
    fn parse_rustc_version_output() {
        assert_eq!(
            RustcVersion::parse_version(b"rustc 1.90.0 (1159e78c4 2024-09-02)\n"),
            Some("1.90.0".to_string())
        );
        assert_eq!(
            RustcVersion::parse_version(b"rustc 1.90.0\n"),
            Some("1.90.0".to_string())
        );
        assert_eq!(RustcVersion::parse_version(b"rust 1.90.0\n"), None);
        assert_eq!(RustcVersion::parse_version(b"rustc\n1.90.0\n"), None);
        assert_eq!(RustcVersion::parse_version(b"rustc \n"), None);
        assert_eq!(RustcVersion::parse_version(b"rustc \xff\n"), None);
    }
}
