//! Find a user requested python version/interpreter.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use once_cell::sync::Lazy;
use platform_host::Platform;
use regex::Regex;
use tracing::{info_span, instrument};
use uv_cache::Cache;

use crate::{Error, Interpreter};

/// ```text
/// -V:3.12          C:\Users\Ferris\AppData\Local\Programs\Python\Python312\python.exe
/// -V:3.8           C:\Users\Ferris\AppData\Local\Programs\Python\Python38\python.exe
/// ```
static PY_LIST_PATHS: Lazy<Regex> = Lazy::new(|| {
    // Without the `R` flag, paths have trailing \r
    Regex::new(r"(?mR)^ -(?:V:)?(\d).(\d+)-?(?:arm)?(?:\d*)\s*\*?\s*(.*)$").unwrap()
});

/// Find a python version/interpreter of a specific version.
///
/// Supported formats:
/// * `-p 3.10` searches for an installed Python 3.10 (`py --list-paths` on Windows, `python3.10` on
///   Linux/Mac). Specifying a patch version is not supported.
/// * `-p python3.10` or `-p python.exe` looks for a binary in `PATH`.
/// * `-p /home/ferris/.local/bin/python3.10` uses this exact Python.
///
/// When the user passes a patch version (e.g. 3.12.1), we currently search for a matching minor
/// version (e.g. `python3.12` on unix) and error when the version mismatches, as a binary with the
/// patch version (e.g. `python3.12.1`) is often not in `PATH` and we make the simplifying
/// assumption that the user has only this one patch version installed.
#[instrument]
pub fn find_requested_python(
    request: &str,
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    let versions = request
        .splitn(3, '.')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>();
    Ok(Some(if let Ok(versions) = versions {
        // `-p 3.10` or `-p 3.10.1`
        if cfg!(unix) {
            if let [_major, _minor, requested_patch] = versions.as_slice() {
                let formatted = PathBuf::from(format!("python{}.{}", versions[0], versions[1]));
                let Some(executable) = Interpreter::find_executable(&formatted)? else {
                    return Ok(None);
                };
                let interpreter = Interpreter::query(&executable, platform, cache)?;
                if interpreter.python_patch() != *requested_patch {
                    return Err(Error::PatchVersionMismatch(
                        executable,
                        request.to_string(),
                        interpreter.python_version().clone(),
                    ));
                }
                interpreter
            } else {
                let formatted = PathBuf::from(format!("python{request}"));
                let Some(executable) = Interpreter::find_executable(&formatted)? else {
                    return Ok(None);
                };
                Interpreter::query(&executable, platform, cache)?
            }
        } else if cfg!(windows) {
            if let Some(python_overwrite) = env::var_os("UV_TEST_PYTHON_PATH") {
                let executable_dir = env::split_paths(&python_overwrite).find(|path| {
                    path.as_os_str()
                        .to_str()
                        // Good enough since we control the bootstrap directory
                        .is_some_and(|path| path.contains(&format!("@{request}")))
                });
                return if let Some(path) = executable_dir {
                    let executable = path.join(if cfg!(unix) {
                        "python3"
                    } else if cfg!(windows) {
                        "python.exe"
                    } else {
                        unimplemented!("Only Windows and Unix are supported")
                    });
                    Ok(Some(Interpreter::query(&executable, platform, cache)?))
                } else {
                    Ok(None)
                };
            }

            match versions.as_slice() {
                [major] => {
                    let Some(executable) = installed_pythons_windows()?
                        .into_iter()
                        .find(|(major_, _minor, _path)| major_ == major)
                        .map(|(_, _, path)| path)
                    else {
                        return Ok(None);
                    };
                    Interpreter::query(&executable, platform, cache)?
                }
                [major, minor] => {
                    let Some(executable) = find_python_windows(*major, *minor)? else {
                        return Ok(None);
                    };
                    Interpreter::query(&executable, platform, cache)?
                }
                [major, minor, requested_patch] => {
                    let Some(executable) = find_python_windows(*major, *minor)? else {
                        return Ok(None);
                    };
                    let interpreter = Interpreter::query(&executable, platform, cache)?;
                    if interpreter.python_patch() != *requested_patch {
                        return Err(Error::PatchVersionMismatch(
                            executable,
                            request.to_string(),
                            interpreter.python_version().clone(),
                        ));
                    }
                    interpreter
                }
                _ => unreachable!(),
            }
        } else {
            unimplemented!("Only Windows and Unix are supported")
        }
    } else if !request.contains(std::path::MAIN_SEPARATOR) {
        // `-p python3.10`; Generally not used on windows because all Python are `python.exe`.
        let Some(executable) = Interpreter::find_executable(request)? else {
            return Ok(None);
        };
        Interpreter::query(&executable, platform, cache)?
    } else {
        // `-p /home/ferris/.local/bin/python3.10`
        let executable = fs_err::canonicalize(request)?;
        Interpreter::query(&executable, platform, cache)?
    }))
}

/// Pick a sensible default for the python a user wants when they didn't specify a version.
///
/// We prefer the test overwrite `UV_TEST_PYTHON_PATH` if it is set, otherwise `python3`/`python` or
/// `python.exe` respectively.
#[instrument]
pub fn find_default_python(platform: &Platform, cache: &Cache) -> Result<Interpreter, Error> {
    let current_dir = env::current_dir()?;
    let python = if cfg!(unix) {
        which::which_in("python3", env::var_os("UV_TEST_PYTHON_PATH"), current_dir)
            .or_else(|_| which::which("python3"))
            .or_else(|_| which::which("python"))
            .map_err(|_| Error::NoPythonInstalledUnix)?
    } else if cfg!(windows) {
        // TODO(konstin): Is that the right order, or should we look for `py --list-paths` first? With the current way
        // it works even if the python launcher is not installed.
        if let Ok(python) = which::which_in(
            "python.exe",
            env::var_os("UV_TEST_PYTHON_PATH"),
            current_dir,
        )
        .or_else(|_| which::which("python.exe"))
        {
            python
        } else {
            installed_pythons_windows()?
                .into_iter()
                .next()
                .ok_or(Error::NoPythonInstalledWindows)?
                .2
        }
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };
    let base_python = fs_err::canonicalize(python)?;
    let interpreter = Interpreter::query(&base_python, platform, cache)?;
    return Ok(interpreter);
}

/// Run `py --list-paths` to find the installed pythons.
///
/// The command takes 8ms on my machine. TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python
/// installations from the registry instead.
fn installed_pythons_windows() -> Result<Vec<(u8, u8, PathBuf)>, Error> {
    // TODO(konstin): We're not checking UV_TEST_PYTHON_PATH here, no test currently depends on it.

    // TODO(konstin): Special case the not found error
    let output = info_span!("py_list_paths")
        .in_scope(|| Command::new("py").arg("--list-paths").output())
        .map_err(Error::PyList)?;

    // There shouldn't be any output on stderr.
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(Error::PythonSubcommandOutput {
            message: format!(
                "Running `py --list-paths` failed with status {}",
                output.status
            ),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    // Find the first python of the version we want in the list
    let stdout =
        String::from_utf8(output.stdout.clone()).map_err(|err| Error::PythonSubcommandOutput {
            message: format!("The stdout of `py --list-paths` isn't UTF-8 encoded: {err}"),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })?;
    let pythons = PY_LIST_PATHS
        .captures_iter(&stdout)
        .filter_map(|captures| {
            let (_, [major, minor, path]) = captures.extract();
            Some((
                major.parse::<u8>().ok()?,
                minor.parse::<u8>().ok()?,
                PathBuf::from(path),
            ))
        })
        .collect();
    Ok(pythons)
}

pub(crate) fn find_python_windows(major: u8, minor: u8) -> Result<Option<PathBuf>, Error> {
    if let Some(python_overwrite) = env::var_os("UV_TEST_PYTHON_PATH") {
        let executable_dir = env::split_paths(&python_overwrite).find(|path| {
            path.as_os_str()
                .to_str()
                // Good enough since we control the bootstrap directory
                .is_some_and(|path| path.contains(&format!("@{major}.{minor}")))
        });
        return Ok(executable_dir.map(|path| {
            path.join(if cfg!(unix) {
                "python3"
            } else if cfg!(windows) {
                "python.exe"
            } else {
                unimplemented!("Only Windows and Unix are supported")
            })
        }));
    }

    Ok(installed_pythons_windows()?
        .into_iter()
        .find(|(major_, minor_, _path)| *major_ == major && *minor_ == minor)
        .map(|(_, _, path)| path))
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use insta::assert_display_snapshot;
    #[cfg(unix)]
    use insta::assert_snapshot;
    use itertools::Itertools;
    use platform_host::Platform;
    use uv_cache::Cache;

    use crate::python_query::find_requested_python;
    use crate::Error;

    fn format_err<T: Debug>(err: Result<T, Error>) -> String {
        anyhow::Error::new(err.unwrap_err())
            .chain()
            .join("\n  Caused by: ")
    }

    #[test]
    #[cfg(unix)]
    fn no_such_python_version() {
        let request = "3.1000";
        let result = find_requested_python(
            request,
            &Platform::current().unwrap(),
            &Cache::temp().unwrap(),
        )
        .unwrap()
        .ok_or(Error::NoSuchPython(request.to_string()));
        assert_snapshot!(
            format_err(result),
            @"No Python 3.1000 In `PATH`. Is Python 3.1000 installed?"
        );
    }

    #[test]
    #[cfg(unix)]
    fn no_such_python_binary() {
        let request = "python3.1000";
        let result = find_requested_python(
            request,
            &Platform::current().unwrap(),
            &Cache::temp().unwrap(),
        )
        .unwrap()
        .ok_or(Error::NoSuchPython(request.to_string()));
        assert_display_snapshot!(
            format_err(result),
            @"No Python python3.1000 In `PATH`. Is Python python3.1000 installed?"
        );
    }

    #[cfg(unix)]
    #[test]
    fn no_such_python_path() {
        let result = find_requested_python(
            "/does/not/exists/python3.12",
            &Platform::current().unwrap(),
            &Cache::temp().unwrap(),
        );
        assert_display_snapshot!(
            format_err(result), @r###"
        failed to canonicalize path `/does/not/exists/python3.12`
          Caused by: No such file or directory (os error 2)
        "###);
    }

    #[cfg(windows)]
    #[test]
    fn no_such_python_path() {
        let result = find_requested_python(
            r"C:\does\not\exists\python3.12",
            &Platform::current().unwrap(),
            &Cache::temp().unwrap(),
        );
        insta::with_settings!({
            filters => vec![
                // The exact message is host language dependent
                (r"Caused by: .* \(os error 3\)", "Caused by: The system cannot find the path specified. (os error 3)")
            ]
        }, {
            assert_display_snapshot!(
                format_err(result), @r###"
        failed to canonicalize path `C:\does\not\exists\python3.12`
          Caused by: The system cannot find the path specified. (os error 3)
        "###);
        });
    }
}
