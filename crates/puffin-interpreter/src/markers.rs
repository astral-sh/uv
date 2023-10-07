use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result};

use pep508_rs::MarkerEnvironment;

/// Return the resolved [`MarkerEnvironment`] for the given Python executable.
pub(crate) fn detect_markers(python: impl AsRef<Path>) -> Result<MarkerEnvironment> {
    let output = call_python(python.as_ref(), ["-c", CAPTURE_MARKERS_SCRIPT])?;
    Ok(serde_json::from_slice::<MarkerEnvironment>(&output.stdout)?)
}

const CAPTURE_MARKERS_SCRIPT: &str = "
import os
import sys
import platform
import json
def format_full_version(info):
    version = '{0.major}.{0.minor}.{0.micro}'.format(info)
    kind = info.releaselevel
    if kind != 'final':
        version += kind[0] + str(info.serial)
    return version

if hasattr(sys, 'implementation'):
    implementation_version = format_full_version(sys.implementation.version)
    implementation_name = sys.implementation.name
else:
    implementation_version = '0'
    implementation_name = ''
bindings = {
    'implementation_name': implementation_name,
    'implementation_version': implementation_version,
    'os_name': os.name,
    'platform_machine': platform.machine(),
    'platform_python_implementation': platform.python_implementation(),
    'platform_release': platform.release(),
    'platform_system': platform.system(),
    'platform_version': platform.version(),
    'python_full_version': platform.python_version(),
    'python_version': '.'.join(platform.python_version_tuple()[:2]),
    'sys_platform': sys.platform,
}
json.dump(bindings, sys.stdout)
sys.stdout.flush()
";

/// Run a Python script and return its output.
fn call_python<I, S>(python: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(python)
        .args(args)
        .output()
        .context(format!("Failed to run `python` at: {:?}", &python))
}
