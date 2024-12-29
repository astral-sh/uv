//! Create a virtual environment.

use std::borrow::Cow;
use std::env::consts::EXE_SUFFIX;
use std::io;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use fs_err as fs;
use fs_err::File;
use itertools::Itertools;
use tracing::{debug, warn};

use uv_fs::{cachedir, Simplified, CWD};
use uv_pypi_types::Scheme;
use uv_python::{Interpreter, VirtualEnvironment};
use uv_shell::escape_posix_for_single_quotes;
use uv_version::version;

use crate::{Error, Prompt};

/// Activation scripts for the environment, with dependent paths templated out.
const ACTIVATE_TEMPLATES: &[(&str, &str)] = &[
    ("activate", include_str!("activator/activate")),
    ("activate.csh", include_str!("activator/activate.csh")),
    ("activate.fish", include_str!("activator/activate.fish")),
    ("activate.nu", include_str!("activator/activate.nu")),
    ("activate.ps1", include_str!("activator/activate.ps1")),
    ("activate.bat", include_str!("activator/activate.bat")),
    ("deactivate.bat", include_str!("activator/deactivate.bat")),
    ("pydoc.bat", include_str!("activator/pydoc.bat")),
    (
        "activate_this.py",
        include_str!("activator/activate_this.py"),
    ),
];
const VIRTUALENV_PATCH: &str = include_str!("_virtualenv.py");

/// Very basic `.cfg` file format writer.
fn write_cfg(f: &mut impl Write, data: &[(String, String)]) -> io::Result<()> {
    for (key, value) in data {
        writeln!(f, "{key} = {value}")?;
    }
    Ok(())
}

/// Create a [`VirtualEnvironment`] at the given location.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) fn create(
    location: &Path,
    interpreter: &Interpreter,
    prompt: Prompt,
    system_site_packages: bool,
    allow_existing: bool,
    relocatable: bool,
    seed: bool,
) -> Result<VirtualEnvironment, Error> {
    // Determine the base Python executable; that is, the Python executable that should be
    // considered the "base" for the virtual environment. This is typically the Python executable
    // from the [`Interpreter`]; however, if the interpreter is a virtual environment itself, then
    // the base Python executable is the Python executable of the interpreter's base interpreter.
    let base_executable = interpreter
        .sys_base_executable()
        .unwrap_or(interpreter.sys_executable());
    let base_python = if cfg!(unix) && interpreter.is_standalone() {
        // In `python-build-standalone`, a symlinked interpreter will return its own executable path
        // as `sys._base_executable`. Using the symlinked path as the base Python executable can be
        // incorrect, since it could cause `home` to point to something that is _not_ a Python
        // installation. Specifically, if the interpreter _itself_ is symlinked to an arbitrary
        // location, we need to fully resolve it to the actual Python executable; however, if the
        // entire standalone interpreter is symlinked, then we can use the symlinked path.
        //
        // We emulate CPython's `getpath.py` to ensure that the base executable results in a valid
        // Python prefix when converted into the `home` key for `pyvenv.cfg`.
        match find_base_python(
            base_executable,
            interpreter.python_major(),
            interpreter.python_minor(),
            interpreter.variant().suffix(),
        ) {
            Ok(path) => path,
            Err(err) => {
                warn!("Failed to find base Python executable: {err}");
                uv_fs::canonicalize_executable(base_executable)?
            }
        }
    } else {
        std::path::absolute(base_executable)?
    };

    debug!(
        "Using base executable for virtual environment: {}",
        base_python.display()
    );

    // Validate the existing location.
    match location.metadata() {
        Ok(metadata) => {
            if metadata.is_file() {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("File exists at `{}`", location.user_display()),
                )));
            } else if metadata.is_dir() {
                if allow_existing {
                    debug!("Allowing existing directory");
                } else if location.join("pyvenv.cfg").is_file() {
                    debug!("Removing existing directory");

                    // On Windows, if the current executable is in the directory, guard against
                    // self-deletion.
                    #[cfg(windows)]
                    if let Ok(itself) = std::env::current_exe() {
                        let target = std::path::absolute(location)?;
                        if itself.starts_with(&target) {
                            debug!("Detected self-delete of executable: {}", itself.display());
                            self_replace::self_delete_outside_path(location)?;
                        }
                    }

                    fs::remove_dir_all(location)?;
                    fs::create_dir_all(location)?;
                } else if location
                    .read_dir()
                    .is_ok_and(|mut dir| dir.next().is_none())
                {
                    debug!("Ignoring empty directory");
                } else {
                    return Err(Error::Io(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "The directory `{}` exists, but it's not a virtual environment",
                            location.user_display()
                        ),
                    )));
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(location)?;
        }
        Err(err) => return Err(Error::Io(err)),
    }

    let location = std::path::absolute(location)?;

    let bin_name = if cfg!(unix) {
        "bin"
    } else if cfg!(windows) {
        "Scripts"
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };
    let scripts = location.join(&interpreter.virtualenv().scripts);
    let prompt = match prompt {
        Prompt::CurrentDirectoryName => CWD
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        Prompt::Static(value) => Some(value),
        Prompt::None => None,
    };

    // Add the CACHEDIR.TAG.
    cachedir::ensure_tag(&location)?;

    // Create a `.gitignore` file to ignore all files in the venv.
    fs::write(location.join(".gitignore"), "*")?;

    // Per PEP 405, the Python `home` is the parent directory of the interpreter.
    let python_home = base_python.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "The Python interpreter needs to have a parent directory",
        )
    })?;

    // Different names for the python interpreter
    fs::create_dir_all(&scripts)?;
    let executable = scripts.join(format!("python{EXE_SUFFIX}"));

    #[cfg(unix)]
    {
        uv_fs::replace_symlink(&base_python, &executable)?;
        uv_fs::replace_symlink(
            "python",
            scripts.join(format!("python{}", interpreter.python_major())),
        )?;
        uv_fs::replace_symlink(
            "python",
            scripts.join(format!(
                "python{}.{}",
                interpreter.python_major(),
                interpreter.python_minor(),
            )),
        )?;

        if interpreter.markers().implementation_name() == "pypy" {
            uv_fs::replace_symlink(
                "python",
                scripts.join(format!("pypy{}", interpreter.python_major())),
            )?;
            uv_fs::replace_symlink("python", scripts.join("pypy"))?;
        }

        if interpreter.markers().implementation_name() == "graalpy" {
            uv_fs::replace_symlink("python", scripts.join("graalpy"))?;
        }
    }

    // No symlinking on Windows, at least not on a regular non-dev non-admin Windows install.
    if cfg!(windows) {
        copy_launcher_windows(
            WindowsExecutable::Python,
            interpreter,
            &base_python,
            &scripts,
            python_home,
        )?;

        if interpreter.markers().implementation_name() == "graalpy" {
            copy_launcher_windows(
                WindowsExecutable::GraalPy,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PythonMajor,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
        } else {
            copy_launcher_windows(
                WindowsExecutable::Pythonw,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
        }

        if interpreter.markers().implementation_name() == "pypy" {
            copy_launcher_windows(
                WindowsExecutable::PythonMajor,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PythonMajorMinor,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PyPy,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PyPyMajor,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PyPyMajorMinor,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PyPyw,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
            copy_launcher_windows(
                WindowsExecutable::PyPyMajorMinorw,
                interpreter,
                &base_python,
                &scripts,
                python_home,
            )?;
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        compile_error!("Only Windows and Unix are supported")
    }

    // Add all the activate scripts for different shells
    for (name, template) in ACTIVATE_TEMPLATES {
        let path_sep = if cfg!(windows) { ";" } else { ":" };

        let relative_site_packages = [
            interpreter.virtualenv().purelib.as_path(),
            interpreter.virtualenv().platlib.as_path(),
        ]
        .iter()
        .dedup()
        .map(|path| {
            pathdiff::diff_paths(path, &interpreter.virtualenv().scripts)
                .expect("Failed to calculate relative path to site-packages")
        })
        .map(|path| path.simplified().to_str().unwrap().replace('\\', "\\\\"))
        .join(path_sep);

        let virtual_env_dir = match (relocatable, name.to_owned()) {
            (true, "activate") => {
                r#"'"$(dirname -- "$(dirname -- "$(realpath -- "$SCRIPT_PATH")")")"'"#.to_string()
            }
            (true, "activate.bat") => r"%~dp0..".to_string(),
            (true, "activate.fish") => {
                r#"'"$(dirname -- "$(cd "$(dirname -- "$(status -f)")"; and pwd)")"'"#.to_string()
            }
            // Note:
            // * relocatable activate scripts appear not to be possible in csh and nu shell
            // * `activate.ps1` is already relocatable by default.
            _ => escape_posix_for_single_quotes(location.simplified().to_str().unwrap()),
        };

        let activator = template
            .replace("{{ VIRTUAL_ENV_DIR }}", &virtual_env_dir)
            .replace("{{ BIN_NAME }}", bin_name)
            .replace(
                "{{ VIRTUAL_PROMPT }}",
                prompt.as_deref().unwrap_or_default(),
            )
            .replace("{{ PATH_SEP }}", path_sep)
            .replace("{{ RELATIVE_SITE_PACKAGES }}", &relative_site_packages);
        fs::write(scripts.join(name), activator)?;
    }

    let mut pyvenv_cfg_data: Vec<(String, String)> = vec![
        (
            "home".to_string(),
            python_home.simplified_display().to_string(),
        ),
        (
            "implementation".to_string(),
            interpreter
                .markers()
                .platform_python_implementation()
                .to_string(),
        ),
        ("uv".to_string(), version().to_string()),
        (
            "version_info".to_string(),
            interpreter.markers().python_full_version().string.clone(),
        ),
        (
            "include-system-site-packages".to_string(),
            if system_site_packages {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ];

    if relocatable {
        pyvenv_cfg_data.push(("relocatable".to_string(), "true".to_string()));
    }

    if seed {
        pyvenv_cfg_data.push(("seed".to_string(), "true".to_string()));
    }

    if let Some(prompt) = prompt {
        pyvenv_cfg_data.push(("prompt".to_string(), prompt));
    }

    if cfg!(windows) && interpreter.markers().implementation_name() == "graalpy" {
        pyvenv_cfg_data.push((
            "venvlauncher_command".to_string(),
            python_home
                .join("graalpy.exe")
                .simplified_display()
                .to_string(),
        ));
    }

    let mut pyvenv_cfg = BufWriter::new(File::create(location.join("pyvenv.cfg"))?);
    write_cfg(&mut pyvenv_cfg, &pyvenv_cfg_data)?;
    drop(pyvenv_cfg);

    // Construct the path to the `site-packages` directory.
    let site_packages = location.join(&interpreter.virtualenv().purelib);
    fs::create_dir_all(&site_packages)?;

    // If necessary, create a symlink from `lib64` to `lib`.
    // See: https://github.com/python/cpython/blob/b228655c227b2ca298a8ffac44d14ce3d22f6faa/Lib/venv/__init__.py#L135C11-L135C16
    #[cfg(unix)]
    if interpreter.pointer_size().is_64()
        && interpreter.markers().os_name() == "posix"
        && interpreter.markers().sys_platform() != "darwin"
    {
        match std::os::unix::fs::symlink("lib", location.join("lib64")) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    // Populate `site-packages` with a `_virtualenv.py` file.
    fs::write(site_packages.join("_virtualenv.py"), VIRTUALENV_PATCH)?;
    fs::write(site_packages.join("_virtualenv.pth"), "import _virtualenv")?;

    Ok(VirtualEnvironment {
        scheme: Scheme {
            purelib: location.join(&interpreter.virtualenv().purelib),
            platlib: location.join(&interpreter.virtualenv().platlib),
            scripts: location.join(&interpreter.virtualenv().scripts),
            data: location.join(&interpreter.virtualenv().data),
            include: location.join(&interpreter.virtualenv().include),
        },
        root: location,
        executable,
    })
}

#[derive(Debug, Copy, Clone)]
enum WindowsExecutable {
    /// The `python.exe` executable (or `venvlauncher.exe` launcher shim).
    Python,
    /// The `python3.exe` executable (or `venvlauncher.exe` launcher shim).
    PythonMajor,
    /// The `python3.<minor>.exe` executable (or `venvlauncher.exe` launcher shim).
    PythonMajorMinor,
    /// The `pythonw.exe` executable (or `venvwlauncher.exe` launcher shim).
    Pythonw,
    /// The `pypy.exe` executable.
    PyPy,
    /// The `pypy3.exe` executable.
    PyPyMajor,
    /// The `pypy3.<minor>.exe` executable.
    PyPyMajorMinor,
    /// The `pypyw.exe` executable.
    PyPyw,
    /// The `pypy3.<minor>w.exe` executable.
    PyPyMajorMinorw,
    // The `graalpy.exe` executable
    GraalPy,
}

impl WindowsExecutable {
    /// The name of the Python executable.
    fn exe(self, interpreter: &Interpreter) -> String {
        match self {
            WindowsExecutable::Python => String::from("python.exe"),
            WindowsExecutable::PythonMajor => {
                format!("python{}.exe", interpreter.python_major())
            }
            WindowsExecutable::PythonMajorMinor => {
                format!(
                    "python{}.{}.exe",
                    interpreter.python_major(),
                    interpreter.python_minor()
                )
            }
            WindowsExecutable::Pythonw => String::from("pythonw.exe"),
            WindowsExecutable::PyPy => String::from("pypy.exe"),
            WindowsExecutable::PyPyMajor => {
                format!("pypy{}.exe", interpreter.python_major())
            }
            WindowsExecutable::PyPyMajorMinor => {
                format!(
                    "pypy{}.{}.exe",
                    interpreter.python_major(),
                    interpreter.python_minor()
                )
            }
            WindowsExecutable::PyPyw => String::from("pypyw.exe"),
            WindowsExecutable::PyPyMajorMinorw => {
                format!(
                    "pypy{}.{}w.exe",
                    interpreter.python_major(),
                    interpreter.python_minor()
                )
            }
            WindowsExecutable::GraalPy => String::from("graalpy.exe"),
        }
    }

    /// The name of the launcher shim.
    fn launcher(self, interpreter: &Interpreter) -> &'static str {
        match self {
            Self::Python | Self::PythonMajor | Self::PythonMajorMinor
                if interpreter.gil_disabled() =>
            {
                "venvlaunchert.exe"
            }
            Self::Python | Self::PythonMajor | Self::PythonMajorMinor => "venvlauncher.exe",
            Self::Pythonw if interpreter.gil_disabled() => "venvwlaunchert.exe",
            Self::Pythonw => "venvwlauncher.exe",
            // From 3.13 on these should replace the `python.exe` and `pythonw.exe` shims.
            // These are not relevant as of now for PyPy as it doesn't yet support Python 3.13.
            Self::PyPy | Self::PyPyMajor | Self::PyPyMajorMinor => "venvlauncher.exe",
            Self::PyPyw | Self::PyPyMajorMinorw => "venvwlauncher.exe",
            WindowsExecutable::GraalPy => "venvlauncher.exe",
        }
    }
}

/// <https://github.com/python/cpython/blob/d457345bbc6414db0443819290b04a9a4333313d/Lib/venv/__init__.py#L261-L267>
/// <https://github.com/pypa/virtualenv/blob/d9fdf48d69f0d0ca56140cf0381edbb5d6fe09f5/src/virtualenv/create/via_global_ref/builtin/cpython/cpython3.py#L78-L83>
///
/// There's two kinds of applications on windows: Those that allocate a console (python.exe)
/// and those that don't because they use window(s) (pythonw.exe).
fn copy_launcher_windows(
    executable: WindowsExecutable,
    interpreter: &Interpreter,
    base_python: &Path,
    scripts: &Path,
    python_home: &Path,
) -> Result<(), Error> {
    // First priority: the `python.exe` and `pythonw.exe` shims.
    let shim = interpreter
        .stdlib()
        .join("venv")
        .join("scripts")
        .join("nt")
        .join(executable.exe(interpreter));
    match fs_err::copy(shim, scripts.join(executable.exe(interpreter))) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    // Second priority: the `venvlauncher.exe` and `venvwlauncher.exe` shims.
    // These are equivalent to the `python.exe` and `pythonw.exe` shims, which were
    // renamed in Python 3.13.
    let shim = interpreter
        .stdlib()
        .join("venv")
        .join("scripts")
        .join("nt")
        .join(executable.launcher(interpreter));
    match fs_err::copy(shim, scripts.join(executable.exe(interpreter))) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    // Third priority: on Conda at least, we can look for the launcher shim next to
    // the Python executable itself.
    let shim = base_python.with_file_name(executable.launcher(interpreter));
    match fs_err::copy(shim, scripts.join(executable.exe(interpreter))) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    // Fourth priority: if the launcher shim doesn't exist, assume this is
    // an embedded Python. Copy the Python executable itself, along with
    // the DLLs, `.pyd` files, and `.zip` files in the same directory.
    match fs_err::copy(
        base_python.with_file_name(executable.exe(interpreter)),
        scripts.join(executable.exe(interpreter)),
    ) {
        Ok(_) => {
            // Copy `.dll` and `.pyd` files from the top-level, and from the
            // `DLLs` subdirectory (if it exists).
            for directory in [
                python_home,
                interpreter.sys_base_prefix().join("DLLs").as_path(),
            ] {
                let entries = match fs_err::read_dir(directory) {
                    Ok(read_dir) => read_dir,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        continue;
                    }
                    Err(err) => {
                        return Err(err.into());
                    }
                };
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("dll") || ext.eq_ignore_ascii_case("pyd")
                    }) {
                        if let Some(file_name) = path.file_name() {
                            fs_err::copy(&path, scripts.join(file_name))?;
                        }
                    }
                }
            }

            // Copy `.zip` files from the top-level.
            match fs_err::read_dir(python_home) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry?;
                        let path = entry.path();
                        if path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                        {
                            if let Some(file_name) = path.file_name() {
                                fs_err::copy(&path, scripts.join(file_name))?;
                            }
                        }
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(err.into());
                }
            };

            return Ok(());
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    Err(Error::NotFound(base_python.user_display().to_string()))
}

/// Find the Python executable that should be considered the "base" for a virtual environment.
///
/// Assumes that the provided executable is that of a standalone Python interpreter.
///
/// The strategy here mimics that of `getpath.py`: we search up the ancestor path to determine
/// whether a given executable will convert into a valid Python prefix; if not, we resolve the
/// symlink and try again.
///
/// This ensures that:
///
/// 1. We avoid using symlinks to arbitrary locations as the base Python executable. For example,
///    if a user symlinks a Python _executable_ to `/Users/user/foo`, we want to avoid using
///    `/Users/user` as `home`, since it's not a Python installation, and so the relevant libraries
///    and headers won't be found when it's used as the executable directory.
///    See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L367-L400>
///
/// 2. We use the "first" resolved symlink that _is_ a valid Python prefix, and thereby preserve
///    symlinks. For example, if a user symlinks a Python _installation_ to `/Users/user/foo`, such
///    that `/Users/user/foo/bin/python` is the resulting executable, we want to use `/Users/user/foo`
///    as `home`, rather than resolving to the symlink target. Concretely, this allows users to
///    symlink patch versions (like `cpython-3.12.6-macos-aarch64-none`) to minor version aliases
///    (like `cpython-3.12-macos-aarch64-none`) and preserve those aliases in the resulting virtual
///    environments.
///
/// See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L591-L594>
fn find_base_python(
    executable: &Path,
    major: u8,
    minor: u8,
    suffix: &str,
) -> Result<PathBuf, io::Error> {
    /// Returns `true` if `path` is the root directory.
    fn is_root(path: &Path) -> bool {
        let mut components = path.components();
        components.next() == Some(std::path::Component::RootDir) && components.next().is_none()
    }

    /// Determining whether `dir` is a valid Python prefix by searching for a "landmark".
    ///
    /// See: <https://github.com/python/cpython/blob/a03efb533a58fd13fb0cc7f4a5c02c8406a407bd/Modules/getpath.py#L183>
    fn is_prefix(dir: &Path, major: u8, minor: u8, suffix: &str) -> bool {
        if cfg!(windows) {
            dir.join("Lib").join("os.py").is_file()
        } else {
            dir.join("lib")
                .join(format!("python{major}.{minor}{suffix}"))
                .join("os.py")
                .is_file()
        }
    }

    let mut executable = Cow::Borrowed(executable);

    loop {
        debug!(
            "Assessing Python executable as base candidate: {}",
            executable.display()
        );

        // Determine whether this executable will produce a valid `home` for a virtual environment.
        for prefix in executable.ancestors().take_while(|path| !is_root(path)) {
            if is_prefix(prefix, major, minor, suffix) {
                return Ok(executable.into_owned());
            }
        }

        // If not, resolve the symlink.
        let resolved = fs_err::read_link(&executable)?;

        // If the symlink is relative, resolve it relative to the executable.
        let resolved = if resolved.is_relative() {
            if let Some(parent) = executable.parent() {
                parent.join(resolved)
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Symlink has no parent directory",
                ));
            }
        } else {
            resolved
        };

        // Normalize the resolved path.
        let resolved = uv_fs::normalize_absolute_path(&resolved)?;

        executable = Cow::Owned(resolved);
    }
}
