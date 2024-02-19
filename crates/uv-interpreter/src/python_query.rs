//! Find a user requested python version/interpreter.

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

use tracing::instrument;

use platform_host::Platform;
use uv_cache::Cache;

use crate::{Error, Interpreter};

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
    if let Ok(versions) = versions {
        // `-p 3.10` or `-p 3.10.1`
        match versions.as_slice() {
            [requested_major] => find_python(
                PythonVersionSelector::Major(*requested_major),
                platform,
                cache,
            ),
            [major, minor] => find_python(
                PythonVersionSelector::MajorMinor(*major, *minor),
                platform,
                cache,
            ),
            [major, minor, requested_patch] => find_python(
                PythonVersionSelector::MajorMinorPatch(*major, *minor, *requested_patch),
                platform,
                cache,
            ),
            // SAFETY: Guaranteed by the Ok(versions) guard
            _ => unreachable!(),
        }
    } else if !request.contains(std::path::MAIN_SEPARATOR) {
        // `-p python3.10`; Generally not used on windows because all Python are `python.exe`.
        let Some(executable) = Interpreter::find_executable(request)? else {
            return Ok(None);
        };
        Ok(Some(Interpreter::query(&executable, platform, cache)?))
    } else {
        // `-p /home/ferris/.local/bin/python3.10`
        let executable = fs_err::canonicalize(request)?;
        Ok(Some(Interpreter::query(&executable, platform, cache)?))
    }
}

/// Pick a sensible default for the python a user wants when they didn't specify a version.
///
/// We prefer the test overwrite `UV_TEST_PYTHON_PATH` if it is set, otherwise `python3`/`python` or
/// `python.exe` respectively.
#[instrument]
pub fn find_default_python(platform: &Platform, cache: &Cache) -> Result<Interpreter, Error> {
    find_python(PythonVersionSelector::Default, platform, cache)?.ok_or(Error::NoPythonInstalled)
}

fn find_python(
    selector: PythonVersionSelector,
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    #[allow(non_snake_case)]
    let UV_TEST_PYTHON_PATH = env::var_os("UV_TEST_PYTHON_PATH");

    if UV_TEST_PYTHON_PATH.is_none() {
        // Use `py` to find the python installation on the system.
        #[cfg(windows)]
        match windows::py_list_paths(selector, platform, cache) {
            Ok(Some(interpreter)) => return Ok(Some(interpreter)),
            Ok(None) => {
                // No matching Python version found, continue searching PATH
            }
            Err(Error::PyList(error)) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        "`py` is not installed. Falling back to searching Python on the path"
                    );
                    // Continue searching for python installations on the path.
                }
            }
            Err(error) => return Err(error),
        }
    }

    let possible_names = selector.possible_names();
    let mut checked_installs = HashSet::new();

    #[allow(non_snake_case)]
    let PATH = UV_TEST_PYTHON_PATH
        .or(env::var_os("PATH"))
        .unwrap_or_default();

    for path in env::split_paths(&PATH) {
        for name in possible_names.iter().flatten() {
            if let Ok(paths) = which::which_in_global(&**name, Some(&path)) {
                for path in paths {
                    if checked_installs.contains(&path) {
                        continue;
                    }

                    #[cfg(windows)]
                    if windows::is_windows_store_shim(&path) {
                        continue;
                    }

                    let installation = PythonInstallation::Interpreter(Interpreter::query(
                        &path, platform, cache,
                    )?);

                    if let Some(interpreter) = installation.select(selector, platform, cache)? {
                        return Ok(Some(interpreter));
                    }

                    checked_installs.insert(path);
                }
            }
        }

        // Python's `venv` model doesn't have this case because they use the `sys.executable` by default
        // which is sufficient to support pyenv-windows. Unfortunately, we can't rely on the executing Python version.
        // That's why we explicitly search for a Python shim as last resort.
        #[cfg(windows)]
        if let Ok(shims) = which::which_in_global("python.bat", Some(&path)) {
            for shim in shims {
                let interpreter = match Interpreter::query(&shim, platform, cache) {
                    Ok(interpreter) => interpreter,
                    Err(error) => {
                        // Don't fail when querying the shim failed. E.g it's possible that no python version is selected
                        // in the shim in which case pyenv prints to stdout.
                        tracing::warn!("Failed to query python shim: {error}");
                        continue;
                    }
                };

                if let Some(interpreter) = PythonInstallation::Interpreter(interpreter)
                    .select(selector, platform, cache)?
                {
                    return Ok(Some(interpreter));
                }
            }
        }
    }

    Ok(None)
}

#[derive(Debug, Clone)]
enum PythonInstallation {
    // Used in the windows implementation.
    #[allow(dead_code)]
    PyListPath {
        major: u8,
        minor: u8,
        executable_path: PathBuf,
    },
    Interpreter(Interpreter),
}

impl PythonInstallation {
    fn major(&self) -> u8 {
        match self {
            PythonInstallation::PyListPath { major, .. } => *major,
            PythonInstallation::Interpreter(interpreter) => interpreter.python_major(),
        }
    }

    fn minor(&self) -> u8 {
        match self {
            PythonInstallation::PyListPath { minor, .. } => *minor,
            PythonInstallation::Interpreter(interpreter) => interpreter.python_minor(),
        }
    }

    /// Selects the interpreter if it matches the selector (version specification).
    ///
    fn select(
        self,
        selector: PythonVersionSelector,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Option<Interpreter>, Error> {
        let selected = match selector {
            PythonVersionSelector::Default => true,
            PythonVersionSelector::Major(major) => self.major() == major,

            PythonVersionSelector::MajorMinor(major, minor) => {
                self.major() == major && self.minor() == minor
            }

            PythonVersionSelector::MajorMinorPatch(major, minor, requested_patch) => {
                let interpreter = self.into_interpreter(platform, cache)?;
                return Ok(
                    if major == interpreter.python_major()
                        && minor == interpreter.python_minor()
                        && requested_patch == interpreter.python_patch()
                    {
                        Some(interpreter)
                    } else {
                        None
                    },
                );
            }
        };

        if selected {
            self.into_interpreter(platform, cache).map(Some)
        } else {
            Ok(None)
        }
    }

    pub(super) fn into_interpreter(
        self,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Interpreter, Error> {
        match self {
            PythonInstallation::PyListPath {
                executable_path, ..
            } => Interpreter::query(&executable_path, platform, cache),
            PythonInstallation::Interpreter(interpreter) => Ok(interpreter),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum PythonVersionSelector {
    Default,
    Major(u8),
    MajorMinor(u8, u8),
    MajorMinorPatch(u8, u8, u8),
}

impl PythonVersionSelector {
    fn possible_names(self) -> [Option<std::borrow::Cow<'static, str>>; 4] {
        let (python, python3, extension) = if cfg!(windows) {
            (
                std::borrow::Cow::Borrowed("python.exe"),
                std::borrow::Cow::Borrowed("python3.exe"),
                ".exe",
            )
        } else {
            (
                std::borrow::Cow::Borrowed("python"),
                std::borrow::Cow::Borrowed("python3"),
                "",
            )
        };

        match self {
            PythonVersionSelector::Default => [Some(python3), Some(python), None, None],
            PythonVersionSelector::Major(major) => [
                Some(std::borrow::Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
                None,
            ],
            PythonVersionSelector::MajorMinor(major, minor) => [
                Some(std::borrow::Cow::Owned(format!(
                    "python{major}.{minor}{extension}"
                ))),
                Some(std::borrow::Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
            ],
            PythonVersionSelector::MajorMinorPatch(major, minor, patch) => [
                Some(std::borrow::Cow::Owned(format!(
                    "python{major}.{minor}.{patch}{extension}",
                ))),
                Some(std::borrow::Cow::Owned(format!(
                    "python{major}.{minor}{extension}"
                ))),
                Some(std::borrow::Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
            ],
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::process::Command;

    use once_cell::sync::Lazy;
    use regex::Regex;
    use tracing::info_span;

    use platform_host::Platform;
    use uv_cache::Cache;

    use crate::python_query::{PythonInstallation, PythonVersionSelector};
    use crate::{Error, Interpreter};

    /// ```text
    /// -V:3.12          C:\Users\Ferris\AppData\Local\Programs\Python\Python312\python.exe
    /// -V:3.8           C:\Users\Ferris\AppData\Local\Programs\Python\Python38\python.exe
    /// ```
    static PY_LIST_PATHS: Lazy<Regex> = Lazy::new(|| {
        // Without the `R` flag, paths have trailing \r
        Regex::new(r"(?mR)^ -(?:V:)?(\d).(\d+)-?(?:arm)?(?:\d*)\s*\*?\s*(.*)$").unwrap()
    });

    /// Run `py --list-paths` to find the installed pythons.
    ///
    /// The command takes 8ms on my machine.
    /// TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python installations from the registry instead.
    pub(super) fn py_list_paths(
        selector: PythonVersionSelector,
        platform: &Platform,
        cache: &Cache,
    ) -> Result<Option<Interpreter>, Error> {
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
            String::from_utf8(output.stdout).map_err(|err| Error::PythonSubcommandOutput {
                message: format!("The stdout of `py --list-paths` isn't UTF-8 encoded: {err}"),
                stdout: String::from_utf8_lossy(err.as_bytes()).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            })?;

        for captures in PY_LIST_PATHS.captures_iter(&stdout) {
            let (_, [major, minor, path]) = captures.extract();

            if let (Some(major), Some(minor)) = (major.parse::<u8>().ok(), minor.parse::<u8>().ok())
            {
                let installation = PythonInstallation::PyListPath {
                    major,
                    minor,
                    executable_path: PathBuf::from(path),
                };

                if let Some(interpreter) = installation.select(selector, platform, cache)? {
                    return Ok(Some(interpreter));
                }
            }
        }

        Ok(None)
    }

    /// On Windows we might encounter the windows store proxy shim (Enabled in Settings/Apps/Advanced app settings/App execution aliases).
    /// This requires quite a bit of custom logic to figure out what this thing does.
    ///
    /// This is a pretty dumb way.  We know how to parse this reparse point, but Microsoft
    /// does not want us to do this as the format is unstable.  So this is a best effort way.
    /// we just hope that the reparse point has the python redirector in it, when it's not
    /// pointing to a valid Python.
    #[allow(unsafe_code)]
    pub(super) fn is_windows_store_shim(path: &std::path::Path) -> bool {
        path.to_str().map_or(false, |path| {
            path.ends_with("Local\\Microsoft\\WindowsApps\\python.exe")
                || path.ends_with("Local\\Microsoft\\WindowsApps\\python3.exe")
        })
    }

    #[cfg(test)]
    #[cfg(windows)]
    mod tests {
        use std::fmt::Debug;

        use insta::assert_display_snapshot;
        use itertools::Itertools;

        use platform_host::Platform;
        use uv_cache::Cache;

        use crate::{find_requested_python, Error};

        fn format_err<T: Debug>(err: Result<T, Error>) -> String {
            anyhow::Error::new(err.unwrap_err())
                .chain()
                .join("\n  Caused by: ")
        }

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
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use insta::assert_display_snapshot;
    #[cfg(unix)]
    use insta::assert_snapshot;
    use itertools::Itertools;

    use platform_host::Platform;
    use uv_cache::Cache;

    use crate::python_query::find_requested_python;
    use crate::Error;

    fn format_err<T: std::fmt::Debug>(err: Result<T, Error>) -> String {
        anyhow::Error::new(err.unwrap_err())
            .chain()
            .join("\n  Caused by: ")
    }

    #[test]
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
}
