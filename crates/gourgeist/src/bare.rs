//! Create a bare virtualenv without any packages install

use std::env::consts::EXE_SUFFIX;
use std::io;
use std::io::{BufWriter, Write};

use camino::{FromPathBufError, Utf8Path, Utf8PathBuf};
use fs_err as fs;
use fs_err::File;
use tracing::info;

use uv_interpreter::Interpreter;

/// The bash activate scripts with the venv dependent paths patches out
const ACTIVATE_TEMPLATES: &[(&str, &str)] = &[
    ("activate", include_str!("activator/activate")),
    ("activate.csh", include_str!("activator/activate.csh")),
    ("activate.fish", include_str!("activator/activate.fish")),
    ("activate.nu", include_str!("activator/activate.nu")),
    ("activate.ps1", include_str!("activator/activate.ps1")),
    (
        "activate_this.py",
        include_str!("activator/activate_this.py"),
    ),
];
const VIRTUALENV_PATCH: &str = include_str!("_virtualenv.py");

/// Very basic `.cfg` file format writer.
fn write_cfg(f: &mut impl Write, data: &[(&str, String); 8]) -> io::Result<()> {
    for (key, value) in data {
        writeln!(f, "{key} = {value}")?;
    }
    Ok(())
}

/// Absolute paths of the virtualenv
#[derive(Debug)]
pub struct VenvPaths {
    /// The location of the virtualenv, e.g. `.venv`
    #[allow(unused)]
    pub root: Utf8PathBuf,

    /// The python interpreter.rs inside the virtualenv, on unix `.venv/bin/python`
    #[allow(unused)]
    pub interpreter: Utf8PathBuf,

    /// The directory with the scripts, on unix `.venv/bin`
    #[allow(unused)]
    pub bin: Utf8PathBuf,

    /// The site-packages directory where all the packages are installed to, on unix
    /// and python 3.11 `.venv/lib/python3.11/site-packages`
    #[allow(unused)]
    pub site_packages: Utf8PathBuf,
}

/// Write all the files that belong to a venv without any packages installed.
pub fn create_bare_venv(location: &Utf8Path, interpreter: &Interpreter) -> io::Result<VenvPaths> {
    // We have to canonicalize the interpreter path, otherwise the home is set to the venv dir instead of the real root.
    // This would make python-build-standalone fail with the encodings module not being found because its home is wrong.
    let base_python: Utf8PathBuf = fs_err::canonicalize(interpreter.sys_executable())?
        .try_into()
        .map_err(|err: FromPathBufError| err.into_io_error())?;

    // Validate the existing location.
    match location.metadata() {
        Ok(metadata) => {
            if metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("File exists at `{location}`"),
                ));
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
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("The directory `{location}` exists, but it's not a virtualenv"),
                    ));
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(location)?;
        }
        Err(err) => return Err(err),
    }

    // TODO(konstin): I bet on windows we'll have to strip the prefix again
    let location = location.canonicalize_utf8()?;
    let bin_name = if cfg!(unix) {
        "bin"
    } else if cfg!(windows) {
        "Scripts"
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };
    let bin_dir = location.join(bin_name);

    fs::write(location.join(".gitignore"), "*")?;

    // Different names for the python interpreter
    fs::create_dir(&bin_dir)?;
    let venv_python = bin_dir.join(format!("python{EXE_SUFFIX}"));
    // No symlinking on Windows, at least not on a regular non-dev non-admin Windows install.
    #[cfg(unix)]
    {
        use fs_err::os::unix::fs::symlink;

        symlink(&base_python, &venv_python)?;
        symlink(
            "python",
            bin_dir.join(format!("python{}", interpreter.python_major())),
        )?;
        symlink(
            "python",
            bin_dir.join(format!(
                "python{}.{}",
                interpreter.python_major(),
                interpreter.python_minor(),
            )),
        )?;
    }
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
            fs_err::copy(shim, bin_dir.join(python_exe))?;
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        compile_error!("Only Windows and Unix are supported")
    }

    // Add all the activate scripts for different shells
    // TODO(konstin): RELATIVE_SITE_PACKAGES is currently only the unix path. We should ensure that all launchers work
    // cross-platform.
    for (name, template) in ACTIVATE_TEMPLATES {
        let activator = template
            .replace("{{ VIRTUAL_ENV_DIR }}", location.as_str())
            .replace("{{ BIN_NAME }}", bin_name)
            .replace(
                "{{ RELATIVE_SITE_PACKAGES }}",
                &format!(
                    "../lib/python{}.{}/site-packages",
                    interpreter.python_major(),
                    interpreter.python_minor(),
                ),
            );
        fs::write(bin_dir.join(name), activator)?;
    }

    // pyvenv.cfg
    let python_home = if cfg!(unix) {
        // On Linux and Mac, Python is symlinked so the base home is the parent of the resolved-by-canonicalize path.
        base_python
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "The python interpreter needs to have a parent directory",
                )
            })?
            .to_string()
    } else if cfg!(windows) {
        // `virtualenv` seems to rely on the undocumented, private `sys._base_executable`. When I tried,
        // `sys.base_prefix` was the same as the parent of `sys._base_executable`, but a much simpler logic and
        // documented.
        // https://github.com/pypa/virtualenv/blob/d9fdf48d69f0d0ca56140cf0381edbb5d6fe09f5/src/virtualenv/discovery/py_info.py#L136-L156
        interpreter.base_prefix().display().to_string()
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };
    let pyvenv_cfg_data = &[
        ("home", python_home),
        ("implementation", "CPython".to_string()),
        (
            "version_info",
            interpreter.markers().python_version.string.clone(),
        ),
        ("gourgeist", env!("CARGO_PKG_VERSION").to_string()),
        // I wouldn't allow this option anyway
        ("include-system-site-packages", "false".to_string()),
        (
            "base-prefix",
            interpreter.base_prefix().to_string_lossy().to_string(),
        ),
        (
            "base-exec-prefix",
            interpreter.base_exec_prefix().to_string_lossy().to_string(),
        ),
        ("base-executable", base_python.to_string()),
    ];
    let mut pyvenv_cfg = BufWriter::new(File::create(location.join("pyvenv.cfg"))?);
    write_cfg(&mut pyvenv_cfg, pyvenv_cfg_data)?;
    drop(pyvenv_cfg);

    let site_packages = if cfg!(unix) {
        location
            .join("lib")
            .join(format!(
                "python{}.{}",
                interpreter.python_major(),
                interpreter.python_minor(),
            ))
            .join("site-packages")
    } else if cfg!(windows) {
        location.join("Lib").join("site-packages")
    } else {
        unimplemented!("Only Windows and Unix are supported")
    };
    fs::create_dir_all(&site_packages)?;
    // Install _virtualenv.py patch.
    // Frankly no idea what that does, i just copied it from virtualenv knowing that
    // distutils/setuptools will have their cursed reasons
    fs::write(site_packages.join("_virtualenv.py"), VIRTUALENV_PATCH)?;
    fs::write(site_packages.join("_virtualenv.pth"), "import _virtualenv")?;

    Ok(VenvPaths {
        root: location.to_path_buf(),
        interpreter: venv_python,
        bin: bin_dir,
        site_packages,
    })
}
