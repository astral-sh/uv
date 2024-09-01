use crate::PythonVersion;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::debug;
use windows_registry::{Key, Value, CURRENT_USER, LOCAL_MACHINE};

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

/// Adding `windows_registry::Value::into_string()`.
fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::String(string) => Some(string),
        Value::Bytes(bytes) => String::from_utf8(bytes.clone()).ok(),
        Value::U32(_) | Value::U64(_) | Value::MultiString(_) | Value::Unknown(_) => None,
    }
}

/// Find all Pythons registered in the Windows registry following PEP 514.
pub(crate) fn registry_pythons() -> Result<Vec<WindowsPython>, windows_result::Error> {
    let mut registry_pythons = Vec::new();
    for root_key in [CURRENT_USER, LOCAL_MACHINE] {
        let Ok(key_python) = root_key.open(r"Software\Python") else {
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
        // Highest version first (reverse), but entries without version at the bottom (regular
        // order).
        if let (Some(version_a), Some(version_b)) = (&a.version, &b.version) {
            version_a.cmp(version_b).reverse().then(a.path.cmp(&b.path))
        } else {
            a.version
                .as_ref()
                .map(|version| &***version)
                .cmp(&b.version.as_ref().map(|version| &***version))
                .then(a.path.cmp(&b.path))
        }
    });

    Ok(registry_pythons)
}

fn read_registry_entry(company: &str, tag: &str, tag_key: &Key) -> Option<WindowsPython> {
    // `ExecutablePath` is mandatory for executable Pythons.
    let Some(executable_path) = tag_key
        .open("InstallPath")
        .and_then(|install_path| install_path.get_value("ExecutablePath"))
        .ok()
        .and_then(value_to_string)
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
        .ok()
        .and_then(|value| match value {
            Value::String(s) => Some(s),
            _ => None,
        })
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
