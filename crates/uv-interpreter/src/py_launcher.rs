use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;
use tracing::info_span;

#[derive(Debug, Clone)]
pub(crate) struct PyListPath {
    pub(crate) major: u8,
    pub(crate) minor: u8,
    pub(crate) executable_path: PathBuf,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    StatusCode {
        message: String,
        exit_code: ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error("Failed to run `py --list-paths` to find Python installations.")]
    Io(#[source] io::Error),
    #[error("The `py` launcher could not be found.")]
    NotFound,
}

/// ```text
/// -V:3.12          C:\Users\Ferris\AppData\Local\Programs\Python\Python312\python.exe
/// -V:3.8           C:\Users\Ferris\AppData\Local\Programs\Python\Python38\python.exe
/// ```
static PY_LIST_PATHS: Lazy<Regex> = Lazy::new(|| {
    // Without the `R` flag, paths have trailing \r
    Regex::new(r"(?mR)^ -(?:V:)?(\d).(\d+)-?(?:arm)?\d*\s*\*?\s*(.*)$").unwrap()
});

/// Use the `py` launcher to find installed Python versions.
///
/// Calls `py --list-paths`.
pub(crate) fn py_list_paths() -> Result<Vec<PyListPath>, Error> {
    // konstin: The command takes 8ms on my machine.
    let output = info_span!("py_list_paths")
        .in_scope(|| Command::new("py").arg("--list-paths").output())
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::Io(err)
            }
        })?;

    // `py` sometimes prints "Installed Pythons found by py Launcher for Windows" to stderr which we ignore.
    if !output.status.success() {
        return Err(Error::StatusCode {
            message: format!(
                "Running `py --list-paths` failed with status {}",
                output.status
            ),
            exit_code: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    // Find the first python of the version we want in the list
    let stdout = String::from_utf8(output.stdout).map_err(|err| Error::StatusCode {
        message: format!("The stdout of `py --list-paths` isn't UTF-8 encoded: {err}"),
        exit_code: output.status,
        stdout: String::from_utf8_lossy(err.as_bytes()).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })?;

    Ok(PY_LIST_PATHS
        .captures_iter(&stdout)
        .filter_map(|captures| {
            let (_, [major, minor, path]) = captures.extract();
            if let (Some(major), Some(minor)) = (major.parse::<u8>().ok(), minor.parse::<u8>().ok())
            {
                Some(PyListPath {
                    major,
                    minor,
                    executable_path: PathBuf::from(path),
                })
            } else {
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use insta::assert_snapshot;
    use itertools::Itertools;

    use uv_cache::Cache;

    use crate::{find_requested_python, Error};

    fn format_err<T: Debug>(err: Result<T, Error>) -> String {
        anyhow::Error::new(err.unwrap_err())
            .chain()
            .join("\n  Caused by: ")
    }

    #[test]
    #[cfg_attr(not(windows), ignore)]
    fn no_such_python_path() {
        let result =
            find_requested_python(r"C:\does\not\exists\python3.12", &Cache::temp().unwrap())
                .unwrap()
                .ok_or(Error::RequestedPythonNotFound(
                    r"C:\does\not\exists\python3.12".to_string(),
                ));
        assert_snapshot!(
            format_err(result),
            @"Failed to locate Python interpreter at `C:\\does\\not\\exists\\python3.12`"
        );
    }
}
