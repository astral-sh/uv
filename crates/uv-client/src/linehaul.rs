use std::env;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use uv_pep508::MarkerEnvironment;
use uv_platform_tags::{Os, Platform};
use uv_version::version;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Installer {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Implementation {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Libc {
    pub lib: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Distro {
    pub name: Option<String>,
    pub version: Option<String>,
    pub id: Option<String>,
    pub libc: Option<Libc>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct System {
    pub name: Option<String>,
    pub release: Option<String>,
}

/// Linehaul structs were derived from
/// <https://github.com/pypi/linehaul-cloud-function/blob/1.0.1/linehaul/ua/datastructures.py>.
/// For the sake of parity, the nullability of all the values was kept intact.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LineHaul {
    pub installer: Option<Installer>,
    pub python: Option<String>,
    pub implementation: Option<Implementation>,
    pub distro: Option<Distro>,
    pub system: Option<System>,
    pub cpu: Option<String>,
    pub openssl_version: Option<String>,
    pub setuptools_version: Option<String>,
    pub rustc_version: Option<String>,
    pub ci: Option<bool>,
}

/// Implements Linehaul information format as defined by
/// <https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/session.py#L109>.
/// This metadata is added to the user agent to enrich PyPI statistics.
impl LineHaul {
    /// Initializes Linehaul information based on PEP 508 markers.
    #[instrument(name = "linehaul", skip_all)]
    pub fn new(markers: &MarkerEnvironment, platform: Option<&Platform>) -> Self {
        // https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/session.py#L87
        let looks_like_ci = ["BUILD_BUILDID", "BUILD_ID", "CI", "PIP_IS_CI"]
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
            sys_info::linux_os_release().ok().map(|info| Distro {
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
            }),
            python: Some(markers.python_full_version().version.to_string()),
            implementation: Option::from(Implementation {
                name: Some(markers.platform_python_implementation().to_string()),
                version: Some(markers.python_full_version().version.to_string()),
            }),
            distro,
            system: Option::from(System {
                name: Some(markers.platform_system().to_string()),
                release: Some(markers.platform_release().to_string()),
            }),
            cpu: Some(markers.platform_machine().to_string()),
            // Should probably always be None in uv.
            openssl_version: None,
            // Should probably always be None in uv.
            setuptools_version: None,
            // Calling rustc --version is likely too slow.
            rustc_version: None,
            ci: looks_like_ci,
        }
    }
}
