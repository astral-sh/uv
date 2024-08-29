//! Microsoft Store Pythons don't register themselves in the registry, so we have to look for them
//! in known locations.
//!
//! Effectively a port of <https://github.com/python/cpython/blob/58ce131037ecb34d506a613f21993cde2056f628/PC/launcher2.c#L1744>

use crate::py_launcher::WindowsPython;
use crate::PythonVersion;
use itertools::Either;
use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::debug;

#[derive(Debug)]
struct MicrosoftStorePython {
    family_name: &'static str,
    version: &'static str,
}

/// List of known Microsoft Store Pythons.
///
/// Copied from <https://github.com/python/cpython/blob/58ce131037ecb34d506a613f21993cde2056f628/PC/launcher2.c#L1963-L1985>,
/// please update when upstream changes.
const MICROSOFT_STORE_PYTHONS: &[MicrosoftStorePython] = &[
    // Releases made through the Store
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.13_qbz5n2kfra8p0",
        version: "3.13",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.12_qbz5n2kfra8p0",
        version: "3.12",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.11_qbz5n2kfra8p0",
        version: "3.11",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.10_qbz5n2kfra8p0",
        version: "3.10",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.9_qbz5n2kfra8p0",
        version: "3.9",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.8_qbz5n2kfra8p0",
        version: "3.8",
    },
    // Side-loadable releases
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.13_3847v3x7pw1km",
        version: "3.13",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.12_3847v3x7pw1km",
        version: "3.12",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.11_3847v3x7pw1km",
        version: "3.11",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.11_hd69rhyc2wevp",
        version: "3.11",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.10_3847v3x7pw1km",
        version: "3.10",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.10_hd69rhyc2wevp",
        version: "3.10",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.9_3847v3x7pw1km",
        version: "3.9",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.9_hd69rhyc2wevp",
        version: "3.9",
    },
    MicrosoftStorePython {
        family_name: "PythonSoftwareFoundation.Python.3.8_hd69rhyc2wevp",
        version: "3.8",
    },
];

/// Microsoft Store Pythons don't register themselves in the registry, so we have to look for them
/// in known locations.
///
/// Effectively a port of <https://github.com/python/cpython/blob/58ce131037ecb34d506a613f21993cde2056f628/PC/launcher2.c#L1744>
pub(crate) fn find_microsoft_store_pythons() -> impl Iterator<Item = WindowsPython> {
    let Ok(local_app_data) = env::var("LOCALAPPDATA") else {
        debug!("`LOCALAPPDATA` not set, ignoring Microsoft store Pythons");
        return Either::Left(std::iter::empty());
    };

    let windows_apps = PathBuf::from(local_app_data)
        .join("Microsoft")
        .join("WindowsApps");

    Either::Right(
        MICROSOFT_STORE_PYTHONS
            .iter()
            .map(move |store_python| {
                let path = windows_apps
                    .join(store_python.family_name)
                    .join("python.exe");
                WindowsPython {
                    path,
                    // All versions are constants, we know they are valid.
                    version: Some(PythonVersion::from_str(store_python.version).unwrap()),
                }
            })
            .filter(|windows_python| windows_python.path.is_file()),
    )
}
