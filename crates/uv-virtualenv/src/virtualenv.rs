//! Create a virtual environment.

use std::env::consts::EXE_SUFFIX;
use std::io;
use std::io::{BufWriter, Write};
use std::path::Path;

use fs_err as fs;
use fs_err::File;
use itertools::Itertools;
use tracing::info;

use pypi_types::Scheme;
use uv_fs::{cachedir, Simplified, CWD};
use uv_python::{Interpreter, VirtualEnvironment};
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
pub(crate) fn create(
    location: &Path,
    interpreter: &Interpreter,
    prompt: Prompt,
    system_site_packages: bool,
    allow_existing: bool,
    relocatable: bool,
) -> Result<VirtualEnvironment, Error> {
    // Determine the base Python executable; that is, the Python executable that should be
    // considered the "base" for the virtual environment. This is typically the Python executable
    // from the [`Interpreter`]; however, if the interpreter is a virtual environment itself, then
    // the base Python executable is the Python executable of the interpreter's base interpreter.
    let base_python = if cfg!(unix) {
        // On Unix, follow symlinks to resolve the base interpreter, since the Python executable in
        // a virtual environment is a symlink to the base interpreter.
        uv_fs::canonicalize_executable(interpreter.sys_executable())?
    } else if cfg!(windows) {
        // On Windows, follow `virtualenv`. If we're in a virtual environment, use
        // `sys._base_executable` if it exists; if not, use `sys.base_prefix`. For example, with
        // Python installed from the Windows Store, `sys.base_prefix` is slightly "incorrect".
        //
        // If we're _not_ in a virtual environment, use the interpreter's executable, since it's
        // already a "system Python". We canonicalize the path to ensure that it's real and
        // consistent, though we don't expect any symlinks on Windows.
        if interpreter.is_virtualenv() {
            if let Some(base_executable) = interpreter.sys_base_executable() {
                base_executable.to_path_buf()
            } else {
                // Assume `python.exe`, though the exact executable name is never used (below) on
                // Windows, only its parent directory.
                interpreter.sys_base_prefix().join("python.exe")
            }
        } else {
            interpreter.sys_executable().to_path_buf()
        }
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };

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
                    info!("Allowing existing directory");
                } else if location.join("pyvenv.cfg").is_file() {
                    info!("Removing existing directory");
                    fs::remove_dir_all(location)?;
                    fs::create_dir_all(location)?;
                } else if location
                    .read_dir()
                    .is_ok_and(|mut dir| dir.next().is_none())
                {
                    info!("Ignoring empty directory");
                } else {
                    return Err(Error::Io(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "The directory `{}` exists, but it's not a virtualenv",
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

    let location = location.canonicalize()?;

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
                // Extremely verbose, but should cover all major POSIX shells,
                // as well as platforms where `readlink` does not implement `-f`.
                r#"'"$(dirname -- "$(CDPATH= cd -- "$(dirname -- ${BASH_SOURCE[0]:-${(%):-%x}})" && echo "$PWD")")"'"#
            }
            (true, "activate.bat") => r"%~dp0..",
            (true, "activate.fish") => {
                r#"'"$(dirname -- "$(cd "$(dirname -- "$(status -f)")"; and pwd)")"'"#
            }
            // Note:
            // * relocatable activate scripts appear not to be possible in csh and nu shell
            // * `activate.ps1` is already relocatable by default.
            _ => {
                // SAFETY: `unwrap` is guaranteed to succeed because `location` is an `Utf8PathBuf`.
                location.simplified().to_str().unwrap()
            }
        };

        let activator = template
            .replace("{{ VIRTUAL_ENV_DIR }}", virtual_env_dir)
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
        (
            "relocatable".to_string(),
            if relocatable {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ];

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
    fn launcher(self) -> &'static str {
        match self {
            WindowsExecutable::Python => "venvlauncher.exe",
            WindowsExecutable::PythonMajor => "venvlauncher.exe",
            WindowsExecutable::PythonMajorMinor => "venvlauncher.exe",
            WindowsExecutable::Pythonw => "venvwlauncher.exe",
            // From 3.13 on these should replace the `python.exe` and `pythonw.exe` shims.
            // These are not relevant as of now for PyPy as it doesn't yet support Python 3.13.
            WindowsExecutable::PyPy => "venvlauncher.exe",
            WindowsExecutable::PyPyMajor => "venvlauncher.exe",
            WindowsExecutable::PyPyMajorMinor => "venvlauncher.exe",
            WindowsExecutable::PyPyw => "venvwlauncher.exe",
            WindowsExecutable::PyPyMajorMinorw => "venvwlauncher.exe",
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
        .join(executable.launcher());
    match fs_err::copy(shim, scripts.join(executable.exe(interpreter))) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
    }

    // Third priority: on Conda at least, we can look for the launcher shim next to
    // the Python executable itself.
    let shim = base_python.with_file_name(executable.launcher());
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
