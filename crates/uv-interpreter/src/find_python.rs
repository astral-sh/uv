use std::borrow::Cow;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use tracing::{debug, instrument};

use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::normalize_path;

use crate::python_environment::{detect_python_executable, detect_virtual_env};
use crate::{Error, Interpreter, PythonVersion};

/// Find a Python of a specific version, a binary with a name or a path to a binary.
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
#[instrument(skip_all, fields(%request))]
pub fn find_requested_python(
    request: &str,
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    debug!("Starting interpreter discovery for Python @ `{request}`");
    let versions = request
        .splitn(3, '.')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>();
    if let Ok(versions) = versions {
        // `-p 3.10` or `-p 3.10.1`
        let selector = match versions.as_slice() {
            [requested_major] => PythonVersionSelector::Major(*requested_major),
            [major, minor] => PythonVersionSelector::MajorMinor(*major, *minor),
            [major, minor, requested_patch] => {
                PythonVersionSelector::MajorMinorPatch(*major, *minor, *requested_patch)
            }
            // SAFETY: Guaranteed by the Ok(versions) guard
            _ => unreachable!(),
        };
        find_python(selector, platform, cache)
    } else if !request.contains(std::path::MAIN_SEPARATOR) {
        // `-p python3.10`; Generally not used on windows because all Python are `python.exe`.
        let Some(executable) = find_executable(request)? else {
            return Ok(None);
        };
        Interpreter::query(executable, platform.clone(), cache).map(Some)
    } else {
        // `-p /home/ferris/.local/bin/python3.10`
        let executable = normalize_path(request);

        Interpreter::query(executable, platform.clone(), cache).map(Some)
    }
}

/// Pick a sensible default for the Python a user wants when they didn't specify a version.
///
/// We prefer the test overwrite `UV_TEST_PYTHON_PATH` if it is set, otherwise `python3`/`python` or
/// `python.exe` respectively.
#[instrument(skip_all)]
pub fn find_default_python(platform: &Platform, cache: &Cache) -> Result<Interpreter, Error> {
    debug!("Starting interpreter discovery for default Python");
    try_find_default_python(platform, cache)?.ok_or(if cfg!(windows) {
        Error::NoPythonInstalledWindows
    } else if cfg!(unix) {
        Error::NoPythonInstalledUnix
    } else {
        unreachable!("Only Unix and Windows are supported")
    })
}

/// Same as [`find_default_python`] but returns `None` if no python is found instead of returning an `Err`.
pub(crate) fn try_find_default_python(
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    find_python(PythonVersionSelector::Default, platform, cache)
}

/// Find a Python version matching `selector`.
///
/// It searches for an existing installation in the following order:
/// * Search for the python binary in `PATH` (or `UV_TEST_PYTHON_PATH` if set). Visits each path and for each path resolves the
///   files in the following order:
///   * Major.Minor.Patch: `pythonx.y.z`, `pythonx.y`, `python.x`, `python`
///   * Major.Minor: `pythonx.y`, `pythonx`, `python`
///   * Major: `pythonx`, `python`
///   * Default: `python3`, `python`
///   * (windows): For each of the above, test for the existence of `python.bat` shim (pyenv-windows) last.
/// * (windows): Discover installations using `py --list-paths` (PEP514). Continue if `py` is not installed.
///
/// (Windows): Filter out the Windows store shim (Enabled in Settings/Apps/Advanced app settings/App execution aliases).
fn find_python(
    selector: PythonVersionSelector,
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    #[allow(non_snake_case)]
    let UV_TEST_PYTHON_PATH = env::var_os("UV_TEST_PYTHON_PATH");

    let use_override = UV_TEST_PYTHON_PATH.is_some();
    let possible_names = selector.possible_names();

    #[allow(non_snake_case)]
    let PATH = UV_TEST_PYTHON_PATH
        .or(env::var_os("PATH"))
        .unwrap_or_default();

    // We use `which` here instead of joining the paths ourselves because `which` checks for us if the python
    // binary is executable and exists. It also has some extra logic that handles inconsistent casing on Windows
    // and expands `~`.
    for path in env::split_paths(&PATH) {
        for name in possible_names.iter().flatten() {
            if let Ok(paths) = which::which_in_global(&**name, Some(&path)) {
                for path in paths {
                    #[cfg(windows)]
                    if windows::is_windows_store_shim(&path) {
                        continue;
                    }

                    let interpreter = match Interpreter::query(&path, platform.clone(), cache) {
                        Ok(interpreter) => interpreter,
                        Err(Error::Python2OrOlder) => {
                            if selector.major() <= Some(2) {
                                return Err(Error::Python2OrOlder);
                            }
                            // Skip over Python 2 or older installation when querying for a recent python installation.
                            debug!("Found a Python 2 installation that isn't supported by uv, skipping.");
                            continue;
                        }
                        Err(error) => return Err(error),
                    };

                    let installation = PythonInstallation::Interpreter(interpreter);

                    if let Some(interpreter) = installation.select(selector, platform, cache)? {
                        return Ok(Some(interpreter));
                    }
                }
            }
        }

        // Python's `venv` model doesn't have this case because they use the `sys.executable` by default
        // which is sufficient to support pyenv-windows. Unfortunately, we can't rely on the executing Python version.
        // That's why we explicitly search for a Python shim as last resort.
        if cfg!(windows) {
            if let Ok(shims) = which::which_in_global("python.bat", Some(&path)) {
                for shim in shims {
                    let interpreter = match Interpreter::query(&shim, platform.clone(), cache) {
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
    }

    if cfg!(windows) && !use_override {
        // Use `py` to find the python installation on the system.
        match windows::py_list_paths() {
            Ok(paths) => {
                for entry in paths {
                    let installation = PythonInstallation::PyListPath(entry);
                    if let Some(interpreter) = installation.select(selector, platform, cache)? {
                        return Ok(Some(interpreter));
                    }
                }
            }
            Err(Error::PyList(error)) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    debug!("`py` is not installed");
                }
            }
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

/// Find the Python interpreter in `PATH` matching the given name (e.g., `python3`, respecting
/// `UV_PYTHON_PATH`.
///
/// Returns `Ok(None)` if not found.
fn find_executable<R: AsRef<OsStr> + Into<OsString> + Copy>(
    requested: R,
) -> Result<Option<PathBuf>, Error> {
    #[allow(non_snake_case)]
    let UV_TEST_PYTHON_PATH = env::var_os("UV_TEST_PYTHON_PATH");

    let use_override = UV_TEST_PYTHON_PATH.is_some();

    #[allow(non_snake_case)]
    let PATH = UV_TEST_PYTHON_PATH
        .or(env::var_os("PATH"))
        .unwrap_or_default();

    // We use `which` here instead of joining the paths ourselves because `which` checks for us if the python
    // binary is executable and exists. It also has some extra logic that handles inconsistent casing on Windows
    // and expands `~`.
    for path in env::split_paths(&PATH) {
        let paths = match which::which_in_global(requested, Some(&path)) {
            Ok(paths) => paths,
            Err(which::Error::CannotFindBinaryPath) => continue,
            Err(err) => return Err(Error::WhichError(requested.into(), err)),
        };

        #[allow(clippy::never_loop)]
        for path in paths {
            #[cfg(windows)]
            if windows::is_windows_store_shim(&path) {
                continue;
            }

            return Ok(Some(path));
        }
    }

    if cfg!(windows) && !use_override {
        // Use `py` to find the python installation on the system.
        match windows::py_list_paths() {
            Ok(paths) => {
                for entry in paths {
                    // Ex) `--python python3.12.exe`
                    if entry.executable_path.file_name() == Some(requested.as_ref()) {
                        return Ok(Some(entry.executable_path));
                    }

                    // Ex) `--python python3.12`
                    if entry
                        .executable_path
                        .file_stem()
                        .is_some_and(|stem| stem == requested.as_ref())
                    {
                        return Ok(Some(entry.executable_path));
                    }
                }
            }
            Err(Error::PyList(error)) => {
                if error.kind() == std::io::ErrorKind::NotFound {
                    debug!("`py` is not installed");
                }
            }
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

#[derive(Debug, Clone)]
struct PyListPath {
    major: u8,
    minor: u8,
    executable_path: PathBuf,
}

#[derive(Debug, Clone)]
enum PythonInstallation {
    PyListPath(PyListPath),
    Interpreter(Interpreter),
}

impl PythonInstallation {
    fn major(&self) -> u8 {
        match self {
            Self::PyListPath(PyListPath { major, .. }) => *major,
            Self::Interpreter(interpreter) => interpreter.python_major(),
        }
    }

    fn minor(&self) -> u8 {
        match self {
            Self::PyListPath(PyListPath { minor, .. }) => *minor,
            Self::Interpreter(interpreter) => interpreter.python_minor(),
        }
    }

    /// Selects the interpreter if it matches the selector (version specification).
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
            Self::PyListPath(PyListPath {
                executable_path, ..
            }) => Interpreter::query(executable_path, platform.clone(), cache),
            Self::Interpreter(interpreter) => Ok(interpreter),
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
    fn possible_names(self) -> [Option<Cow<'static, str>>; 4] {
        let (python, python3, extension) = if cfg!(windows) {
            (
                Cow::Borrowed("python.exe"),
                Cow::Borrowed("python3.exe"),
                ".exe",
            )
        } else {
            (Cow::Borrowed("python"), Cow::Borrowed("python3"), "")
        };

        match self {
            Self::Default => [Some(python3), Some(python), None, None],
            Self::Major(major) => [
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
                None,
            ],
            Self::MajorMinor(major, minor) => [
                Some(Cow::Owned(format!("python{major}.{minor}{extension}"))),
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
                None,
            ],
            Self::MajorMinorPatch(major, minor, patch) => [
                Some(Cow::Owned(format!(
                    "python{major}.{minor}.{patch}{extension}",
                ))),
                Some(Cow::Owned(format!("python{major}.{minor}{extension}"))),
                Some(Cow::Owned(format!("python{major}{extension}"))),
                Some(python),
            ],
        }
    }

    fn major(self) -> Option<u8> {
        match self {
            Self::Default => None,
            Self::Major(major) => Some(major),
            Self::MajorMinor(major, _) => Some(major),
            Self::MajorMinorPatch(major, _, _) => Some(major),
        }
    }
}

/// Find a matching Python or any fallback Python.
///
/// If no Python version is provided, we will use the first available interpreter.
///
/// If a Python version is provided, we will first try to find an exact match. If
/// that cannot be found and a patch version was requested, we will look for a match
/// without comparing the patch version number. If that cannot be found, we fall back to
/// the first available version.
///
/// See [`Self::find_version`] for details on the precedence of Python lookup locations.
#[instrument(skip_all, fields(?python_version))]
pub fn find_best_python(
    python_version: Option<&PythonVersion>,
    platform: &Platform,
    cache: &Cache,
) -> Result<Interpreter, Error> {
    if let Some(python_version) = python_version {
        debug!(
            "Starting interpreter discovery for Python {}",
            python_version
        );
    } else {
        debug!("Starting interpreter discovery for active Python");
    }

    // First, check for an exact match (or the first available version if no Python version was provided)
    if let Some(interpreter) = find_version(python_version, platform, cache)? {
        return Ok(interpreter);
    }

    if let Some(python_version) = python_version {
        // If that fails, and a specific patch version was requested try again allowing a
        // different patch version
        if python_version.patch().is_some() {
            if let Some(interpreter) =
                find_version(Some(&python_version.without_patch()), platform, cache)?
            {
                return Ok(interpreter);
            }
        }
    }

    // If a Python version was requested but cannot be fulfilled, just take any version
    if let Some(interpreter) = find_version(None, platform, cache)? {
        return Ok(interpreter);
    }

    Err(Error::PythonNotFound)
}

/// Find a Python interpreter.
///
/// We check, in order, the following locations:
///
/// - `UV_DEFAULT_PYTHON`, which is set to the python interpreter when using `python -m uv`.
/// - `VIRTUAL_ENV` and `CONDA_PREFIX`
/// - A `.venv` folder
/// - If a python version is given: Search `PATH` and `py --list-paths`, see `find_python`
/// - `python3` (unix) or `python.exe` (windows)
///
/// If `UV_TEST_PYTHON_PATH` is set, we will not check for Python versions in the
/// global PATH, instead we will search using the provided path. Virtual environments
/// will still be respected.
///
/// If a version is provided and an interpreter cannot be found with the given version,
/// we will return [`None`].
fn find_version(
    python_version: Option<&PythonVersion>,
    platform: &Platform,
    cache: &Cache,
) -> Result<Option<Interpreter>, Error> {
    let version_matches = |interpreter: &Interpreter| -> bool {
        if let Some(python_version) = python_version {
            // If a patch version was provided, check for an exact match
            python_version.is_satisfied_by(interpreter)
        } else {
            // The version always matches if one was not provided
            true
        }
    };

    // Check if the venv Python matches.
    if let Some(venv) = detect_virtual_env()? {
        let executable = detect_python_executable(venv);
        let interpreter = Interpreter::query(executable, platform.clone(), cache)?;

        if version_matches(&interpreter) {
            return Ok(Some(interpreter));
        }
    };

    // Look for the requested version with by search for `python{major}.{minor}` in `PATH` on
    // Unix and `py --list-paths` on Windows.
    let interpreter = if let Some(python_version) = python_version {
        find_requested_python(&python_version.string, platform, cache)?
    } else {
        try_find_default_python(platform, cache)?
    };

    if let Some(interpreter) = interpreter {
        debug_assert!(version_matches(&interpreter));
        Ok(Some(interpreter))
    } else {
        Ok(None)
    }
}

mod windows {
    use std::path::PathBuf;
    use std::process::Command;

    use once_cell::sync::Lazy;
    use regex::Regex;
    use tracing::info_span;

    use crate::find_python::PyListPath;
    use crate::Error;

    /// ```text
    /// -V:3.12          C:\Users\Ferris\AppData\Local\Programs\Python\Python312\python.exe
    /// -V:3.8           C:\Users\Ferris\AppData\Local\Programs\Python\Python38\python.exe
    /// ```
    static PY_LIST_PATHS: Lazy<Regex> = Lazy::new(|| {
        // Without the `R` flag, paths have trailing \r
        Regex::new(r"(?mR)^ -(?:V:)?(\d).(\d+)-?(?:arm)?\d*\s*\*?\s*(.*)$").unwrap()
    });

    /// Run `py --list-paths` to find the installed pythons.
    ///
    /// The command takes 8ms on my machine.
    /// TODO(konstin): Implement <https://peps.python.org/pep-0514/> to read python installations from the registry instead.
    pub(super) fn py_list_paths() -> Result<Vec<PyListPath>, Error> {
        let output = info_span!("py_list_paths")
            .in_scope(|| Command::new("py").arg("--list-paths").output())
            .map_err(Error::PyList)?;

        // `py` sometimes prints "Installed Pythons found by py Launcher for Windows" to stderr which we ignore.
        if !output.status.success() {
            return Err(Error::PythonSubcommandOutput {
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
        let stdout =
            String::from_utf8(output.stdout).map_err(|err| Error::PythonSubcommandOutput {
                message: format!("The stdout of `py --list-paths` isn't UTF-8 encoded: {err}"),
                exit_code: output.status,
                stdout: String::from_utf8_lossy(err.as_bytes()).trim().to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            })?;

        Ok(PY_LIST_PATHS
            .captures_iter(&stdout)
            .filter_map(|captures| {
                let (_, [major, minor, path]) = captures.extract();
                if let (Some(major), Some(minor)) =
                    (major.parse::<u8>().ok(), minor.parse::<u8>().ok())
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

    /// On Windows we might encounter the Windows Store proxy shim (enabled in:
    /// Settings/Apps/Advanced app settings/App execution aliases). When Python is _not_ installed
    /// via the Windows Store, but the proxy shim is enabled, then executing `python.exe` or
    /// `python3.exe` will redirect to the Windows Store installer.
    ///
    /// We need to detect that these `python.exe` and `python3.exe` files are _not_ Python
    /// executables.
    ///
    /// This method is taken from Rye:
    ///
    /// > This is a pretty dumb way.  We know how to parse this reparse point, but Microsoft
    /// > does not want us to do this as the format is unstable.  So this is a best effort way.
    /// > we just hope that the reparse point has the python redirector in it, when it's not
    /// > pointing to a valid Python.
    ///
    /// See: <https://github.com/astral-sh/rye/blob/b0e9eccf05fe4ff0ae7b0250a248c54f2d780b4d/rye/src/cli/shim.rs#L108>
    #[cfg(windows)]
    pub(super) fn is_windows_store_shim(path: &std::path::Path) -> bool {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::prelude::OsStrExt;
        use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
        use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
        use winapi::um::ioapiset::DeviceIoControl;
        use winapi::um::winbase::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT};
        use winapi::um::winioctl::FSCTL_GET_REPARSE_POINT;
        use winapi::um::winnt::{FILE_ATTRIBUTE_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE};

        // The path must be absolute.
        if !path.is_absolute() {
            return false;
        }

        // The path must point to something like:
        //   `C:\Users\crmar\AppData\Local\Microsoft\WindowsApps\python3.exe`
        let mut components = path.components().rev();

        // Ex) `python.exe` or `python3.exe`
        if !components
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|component| component == "python.exe" || component == "python3.exe")
        {
            return false;
        }

        // Ex) `WindowsApps`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "WindowsApps")
        {
            return false;
        }

        // Ex) `Microsoft`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "Microsoft")
        {
            return false;
        }

        // The file is only relevant if it's a reparse point.
        let Ok(md) = fs_err::symlink_metadata(path) else {
            return false;
        };
        if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
            return false;
        }

        let mut path_encoded = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();

        // SAFETY: The path is null-terminated.
        #[allow(unsafe_code)]
        let reparse_handle = unsafe {
            CreateFileW(
                path_encoded.as_mut_ptr(),
                0,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };

        if reparse_handle == INVALID_HANDLE_VALUE {
            return false;
        }

        let mut buf = [0u16; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
        let mut bytes_returned = 0;

        // SAFETY: The buffer is large enough to hold the reparse point.
        #[allow(unsafe_code, clippy::cast_possible_truncation)]
        let success = unsafe {
            DeviceIoControl(
                reparse_handle,
                FSCTL_GET_REPARSE_POINT,
                std::ptr::null_mut(),
                0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32 * 2,
                &mut bytes_returned,
                std::ptr::null_mut(),
            ) != 0
        };

        // SAFETY: The handle is valid.
        #[allow(unsafe_code)]
        unsafe {
            CloseHandle(reparse_handle);
        }

        // If the operation failed, assume it's not a reparse point.
        if !success {
            return false;
        }

        let reparse_point = String::from_utf16_lossy(&buf[..bytes_returned as usize]);
        reparse_point.contains("\\AppInstallerPythonRedirector.exe")
    }

    #[cfg(test)]
    #[cfg(windows)]
    mod tests {
        use std::fmt::Debug;

        use insta::assert_snapshot;
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
                assert_snapshot!(
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
    use insta::assert_snapshot;
    use itertools::Itertools;

    use platform_host::Platform;
    use uv_cache::Cache;

    use crate::find_python::find_requested_python;
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
        assert_snapshot!(
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
        assert_snapshot!(
            format_err(result), @r###"
        failed to canonicalize path `/does/not/exists/python3.12`
          Caused by: No such file or directory (os error 2)
        "###);
    }
}
