//! Discover Python installations in the Windows registry following PEP 514.

use std::cmp::Ordering;
use std::path::PathBuf;
use std::str::FromStr;

use tracing::debug;
use windows::Win32::System::Registry::{KEY_WOW64_32KEY, KEY_WOW64_64KEY};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};

use uv_python_types::PythonVersion;

/// A Python interpreter found in the Windows registry through PEP 514 or from a known Microsoft
/// Store path.
///
/// There are a lot more (optional) fields defined in PEP 514, but we only care about path and
/// version here, for everything else we probe with a Python script.
#[derive(Debug, Clone)]
pub(crate) struct WindowsPython {
    pub(crate) path: PathBuf,
    pub(crate) version: Option<PythonVersion>,
}

/// Find all Pythons registered in the Windows registry following PEP 514.
pub(crate) fn registry_pythons() -> Result<Vec<WindowsPython>, windows::core::Error> {
    let mut registry_pythons = Vec::new();
    // Prefer `HKEY_CURRENT_USER` over `HKEY_LOCAL_MACHINE`.
    // By default, a 64-bit program does not see a 32-bit global (HKLM) installation of Python in
    // the registry (https://github.com/astral-sh/uv/issues/11217). To work around this, we manually
    // request both 32-bit and 64-bit access. The flags have no effect on 32-bit
    // (https://stackoverflow.com/a/12796797/3549270).
    for (root_key, access_modifier) in [
        (CURRENT_USER, None),
        (LOCAL_MACHINE, Some(KEY_WOW64_64KEY)),
        (LOCAL_MACHINE, Some(KEY_WOW64_32KEY)),
    ] {
        let mut open_options = root_key.options();
        open_options.read();
        if let Some(access_modifier) = access_modifier {
            open_options.access(access_modifier.0);
        }
        let Ok(key_python) = open_options.open(r"Software\Python") else {
            continue;
        };
        for company in key_python.keys()? {
            // Reserved name according to the PEP.
            if company == "PyLauncher" {
                continue;
            }
            let Ok(company_key) = key_python.open(&company) else {
                // Ignore invalid entries
                continue;
            };
            for tag in company_key.keys()? {
                let tag_key = company_key.open(&tag)?;

                if let Some(registry_python) = read_registry_entry(&company, &tag, &tag_key) {
                    registry_pythons.push(registry_python);
                }
            }
        }
    }

    // The registry has no natural ordering, so we're processing the latest version first.
    registry_pythons.sort_by(|a, b| {
        match (&a.version, &b.version) {
            // Place entries with a version before those without a version.
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            // We want the highest version on top, which is the inverse from the regular order. The
            // path is an arbitrary but stable tie-breaker.
            (Some(version_a), Some(version_b)) => {
                version_a.cmp(version_b).reverse().then(a.path.cmp(&b.path))
            }
            // Sort the entries without a version arbitrarily, but stable (by path).
            (None, None) => a.path.cmp(&b.path),
        }
    });

    Ok(registry_pythons)
}

fn read_registry_entry(company: &str, tag: &str, tag_key: &Key) -> Option<WindowsPython> {
    // `ExecutablePath` is mandatory for executable Pythons.
    let Ok(executable_path) = tag_key
        .open("InstallPath")
        .and_then(|install_path| install_path.get_value("ExecutablePath"))
        .and_then(String::try_from)
    else {
        debug!(
            r"Python interpreter in the registry is not executable: `Software\Python\{}\{}",
            company, tag
        );
        return None;
    };

    // `SysVersion` is optional.
    let version = tag_key
        .get_value("SysVersion")
        .and_then(String::try_from)
        .ok()
        .and_then(|s| match PythonVersion::from_str(&s) {
            Ok(version) => Some(version),
            Err(err) => {
                debug!(
                    "Skipping Python interpreter ({executable_path}) \
                    with invalid registry version {s}: {err}",
                );
                None
            }
        });

    Some(WindowsPython {
        path: PathBuf::from(executable_path),
        version,
    })
}
