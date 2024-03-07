//! Create a bare virtualenv without any packages install

use std::env;
use std::env::consts::EXE_SUFFIX;
use std::io;
use std::io::{BufWriter, Write};
use std::path::Path;

use fs_err as fs;
use fs_err::File;
use pypi_types::Scheme;
use tracing::info;

use uv_fs::Simplified;
use uv_interpreter::{Interpreter, Virtualenv};

use crate::{Error, Prompt};

/// The bash activate scripts with the venv dependent paths patches out
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

/// Write all the files that belong to a venv without any packages installed.
pub fn create_bare_venv(
    location: &Path,
    interpreter: &Interpreter,
    prompt: Prompt,
    system_site_packages: bool,
    extra_cfg: Vec<(String, String)>,
) -> Result<Virtualenv, Error> {
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
            if let Some(base_executable) = interpreter.base_executable() {
                base_executable.to_path_buf()
            } else {
                // Assume `python.exe`, though the exact executable name is never used (below) on
                // Windows, only its parent directory.
                interpreter.base_prefix().join("python.exe")
            }
        } else {
            uv_fs::canonicalize_executable(interpreter.sys_executable())?
        }
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };

    // Validate the existing location.
    match location.metadata() {
        Ok(metadata) => {
            if metadata.is_file() {
                return Err(Error::IO(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("File exists at `{}`", location.simplified_display()),
                )));
            } else if metadata.is_dir() {
                if location.join("pyvenv.cfg").is_file() {
                    info!("Removing existing directory");
                    fs::remove_dir_all(location)?;
                    fs::create_dir_all(location)?;
                } else if location
                    .read_dir()
                    .is_ok_and(|mut dir| dir.next().is_none())
                {
                    info!("Ignoring empty directory");
                } else {
                    return Err(Error::IO(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "The directory `{}` exists, but it's not a virtualenv",
                            location.simplified_display()
                        ),
                    )));
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(location)?;
        }
        Err(err) => return Err(Error::IO(err)),
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
        Prompt::CurrentDirectoryName => env::current_dir()?
            .file_name()
            .map(|name| name.to_string_lossy().to_string()),
        Prompt::Static(value) => Some(value),
        Prompt::None => None,
    };

    // Add the CACHEDIR.TAG.
    cachedir::ensure_tag(&location)?;

    // Create a `.gitignore` file to ignore all files in the venv.
    fs::write(location.join(".gitignore"), "*")?;

    // Different names for the python interpreter
    fs::create_dir(&scripts)?;
    let executable = scripts.join(format!("python{EXE_SUFFIX}"));

    #[cfg(unix)]
    {
        use fs_err::os::unix::fs::symlink;

        symlink(&base_python, &executable)?;
        symlink(
            "python",
            scripts.join(format!("python{}", interpreter.python_major())),
        )?;
        symlink(
            "python",
            scripts.join(format!(
                "python{}.{}",
                interpreter.python_major(),
                interpreter.python_minor(),
            )),
        )?;
    }

    // No symlinking on Windows, at least not on a regular non-dev non-admin Windows install.
    #[cfg(windows)]
    {
        // https://github.com/python/cpython/blob/d457345bbc6414db0443819290b04a9a4333313d/Lib/venv/__init__.py#L261-L267
        // https://github.com/pypa/virtualenv/blob/d9fdf48d69f0d0ca56140cf0381edbb5d6fe09f5/src/virtualenv/create/via_global_ref/builtin/cpython/cpython3.py#L78-L83
        // There's two kinds of applications on windows: Those that allocate a console (python.exe) and those that
        // don't because they use window(s) (pythonw.exe).
        for python_exe in ["python.exe", "pythonw.exe"] {
            let shim = interpreter
                .stdlib()
                .join("venv")
                .join("scripts")
                .join("nt")
                .join(python_exe);
            match fs_err::copy(shim, scripts.join(python_exe)) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    let launcher = match python_exe {
                        "python.exe" => "venvwlauncher.exe",
                        "pythonw.exe" => "venvwlauncher.exe",
                        _ => unreachable!(),
                    };

                    // If `python.exe` doesn't exist, try the `venvlaucher.exe` shim.
                    let shim = interpreter
                        .stdlib()
                        .join("venv")
                        .join("scripts")
                        .join("nt")
                        .join(launcher);

                    // If the `venvwlauncher.exe` shim doesn't exist, then on Conda at least, we
                    // can look for it next to the Python executable itself.
                    match fs_err::copy(shim, scripts.join(python_exe)) {
                        Ok(_) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {
                            let shim = base_python.with_file_name(launcher);
                            fs_err::copy(shim, scripts.join(python_exe))?;
                        }
                        Err(err) => {
                            return Err(err.into());
                        }
                    }
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        compile_error!("Only Windows and Unix are supported")
    }

    // Add all the activate scripts for different shells
    for (name, template) in ACTIVATE_TEMPLATES {
        let relative_site_packages = pathdiff::diff_paths(
            &interpreter.virtualenv().purelib,
            &interpreter.virtualenv().scripts,
        )
        .expect("Failed to calculate relative path to site-packages");
        let activator = template
            .replace(
                "{{ VIRTUAL_ENV_DIR }}",
                // SAFETY: `unwrap` is guaranteed to succeed because `location` is an `Utf8PathBuf`.
                location.simplified().to_str().unwrap(),
            )
            .replace("{{ BIN_NAME }}", bin_name)
            .replace(
                "{{ VIRTUAL_PROMPT }}",
                prompt.as_deref().unwrap_or_default(),
            )
            .replace(
                "{{ RELATIVE_SITE_PACKAGES }}",
                relative_site_packages.simplified().to_str().unwrap(),
            );
        fs::write(scripts.join(name), activator)?;
    }

    // Per PEP 405, the Python `home` is the parent directory of the interpreter.
    let python_home = base_python
        .parent()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "The Python interpreter needs to have a parent directory",
            )
        })?
        .simplified_display()
        .to_string();

    // Validate extra_cfg
    let reserved_keys = [
        "home",
        "implementation",
        "version_info",
        "include-system-site-packages",
        "base-prefix",
        "base-exec-prefix",
        "base-executable",
        "prompt",
    ];
    for (key, _) in &extra_cfg {
        if reserved_keys.contains(&key.as_str()) {
            return Err(Error::ReservedConfigKey(key.to_string()));
        }
    }

    let mut pyvenv_cfg_data: Vec<(String, String)> = vec![
        ("home".to_string(), python_home),
        (
            "implementation".to_string(),
            interpreter.markers().platform_python_implementation.clone(),
        ),
        (
            "version_info".to_string(),
            interpreter.markers().python_full_version.string.clone(),
        ),
        (
            "include-system-site-packages".to_string(),
            if system_site_packages {
                "true".to_string()
            } else {
                "false".to_string()
            },
        ),
    ]
    .into_iter()
    .chain(extra_cfg)
    .collect();

    if let Some(prompt) = prompt {
        pyvenv_cfg_data.push(("prompt".to_string(), prompt));
    }

    let mut pyvenv_cfg = BufWriter::new(File::create(location.join("pyvenv.cfg"))?);
    write_cfg(&mut pyvenv_cfg, &pyvenv_cfg_data)?;
    drop(pyvenv_cfg);

    // Construct the path to the `site-packages` directory.
    let site_packages = location.join(&interpreter.virtualenv().purelib);

    // Populate `site-packages` with a `_virtualenv.py` file.
    fs::create_dir_all(&site_packages)?;
    fs::write(site_packages.join("_virtualenv.py"), VIRTUALENV_PATCH)?;
    fs::write(site_packages.join("_virtualenv.pth"), "import _virtualenv")?;

    Ok(Virtualenv {
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
