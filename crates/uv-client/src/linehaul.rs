use std::env;

use serde::Serialize;
use tracing::instrument;

use uv_pep508::MarkerEnvironment;
use uv_platform_tags::{Os, Platform};
use uv_static::EnvVars;
use uv_version::version;

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
            // Calling rustc --version is likely too slow.
            rustc_version: None,
            ci: looks_like_ci,
        }
    }
}
