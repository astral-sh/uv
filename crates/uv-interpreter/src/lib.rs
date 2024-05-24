//! Find requested Python interpreters and query interpreters for information.
use thiserror::Error;

pub use crate::discovery::{
    find_best_interpreter, find_default_interpreter, find_interpreter, Error as DiscoveryError,
    InterpreterNotFound, InterpreterRequest, InterpreterSource, SourceSelector, SystemPython,
    VersionRequest,
};
pub use crate::environment::PythonEnvironment;
pub use crate::interpreter::Interpreter;
pub use crate::pointer_size::PointerSize;
pub use crate::python_version::PythonVersion;
pub use crate::target::Target;
pub use crate::virtualenv::{Error as VirtualEnvError, PyVenvConfiguration, VirtualEnvironment};

mod discovery;
mod environment;
mod implementation;
mod interpreter;
pub mod managed;
pub mod platform;
mod pointer_size;
mod py_launcher;
mod python_version;
mod target;
mod virtualenv;

#[cfg(not(test))]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::current_dir()
}

#[cfg(test)]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::var_os("PWD")
        .map(std::path::PathBuf::from)
        .map(Result::Ok)
        .unwrap_or(std::env::current_dir())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    VirtualEnv(#[from] virtualenv::Error),

    #[error(transparent)]
    Query(#[from] interpreter::Error),

    #[error(transparent)]
    Discovery(#[from] discovery::Error),

    #[error(transparent)]
    PyLauncher(#[from] py_launcher::Error),

    #[error(transparent)]
    NotFound(#[from] discovery::InterpreterNotFound),
}

// The mock interpreters are not valid on Windows so we don't have unit test coverage there
// TODO(zanieb): We should write a mock interpreter script that works on Windows
#[cfg(all(test, unix))]
mod tests {
    use anyhow::Result;
    use indoc::{formatdoc, indoc};
    use std::{
        env,
        ffi::OsString,
        path::{Path, PathBuf},
        str::FromStr,
    };
    use temp_env::with_vars;
    use test_log::test;

    use assert_fs::{prelude::*, TempDir};
    use uv_cache::Cache;

    use crate::{
        discovery::{self, DiscoveredInterpreter, InterpreterRequest, VersionRequest},
        find_best_interpreter, find_default_interpreter, find_interpreter,
        implementation::ImplementationName,
        virtualenv::virtualenv_python_executable,
        Error, InterpreterNotFound, InterpreterSource, PythonEnvironment, PythonVersion,
        SourceSelector, SystemPython,
    };

    /// Create a fake Python interpreter executable which returns fixed metadata mocking our interpreter
    /// query script output.
    fn create_mock_interpreter(
        path: &Path,
        version: &PythonVersion,
        implementation: ImplementationName,
        system: bool,
    ) -> Result<()> {
        let json = indoc! {r##"
            {
                "result": "success",
                "platform": {
                    "os": {
                        "name": "manylinux",
                        "major": 2,
                        "minor": 38
                    },
                    "arch": "x86_64"
                },
                "markers": {
                    "implementation_name": "{IMPLEMENTATION}",
                    "implementation_version": "{FULL_VERSION}",
                    "os_name": "posix",
                    "platform_machine": "x86_64",
                    "platform_python_implementation": "{IMPLEMENTATION}",
                    "platform_release": "6.5.0-13-generic",
                    "platform_system": "Linux",
                    "platform_version": "#13-Ubuntu SMP PREEMPT_DYNAMIC Fri Nov  3 12:16:05 UTC 2023",
                    "python_full_version": "{FULL_VERSION}",
                    "python_version": "{VERSION}",
                    "sys_platform": "linux"
                },
                "base_exec_prefix": "/home/ferris/.pyenv/versions/{FULL_VERSION}",
                "base_prefix": "/home/ferris/.pyenv/versions/{FULL_VERSION}",
                "prefix": "{PREFIX}",
                "sys_executable": "{PATH}",
                "sys_path": [
                    "/home/ferris/.pyenv/versions/{FULL_VERSION}/lib/python{VERSION}/lib/python{VERSION}",
                    "/home/ferris/.pyenv/versions/{FULL_VERSION}/lib/python{VERSION}/site-packages"
                ],
                "stdlib": "/home/ferris/.pyenv/versions/{FULL_VERSION}/lib/python{VERSION}",
                "scheme": {
                    "data": "/home/ferris/.pyenv/versions/{FULL_VERSION}",
                    "include": "/home/ferris/.pyenv/versions/{FULL_VERSION}/include",
                    "platlib": "/home/ferris/.pyenv/versions/{FULL_VERSION}/lib/python{VERSION}/site-packages",
                    "purelib": "/home/ferris/.pyenv/versions/{FULL_VERSION}/lib/python{VERSION}/site-packages",
                    "scripts": "/home/ferris/.pyenv/versions/{FULL_VERSION}/bin"
                },
                "virtualenv": {
                    "data": "",
                    "include": "include",
                    "platlib": "lib/python{VERSION}/site-packages",
                    "purelib": "lib/python{VERSION}/site-packages",
                    "scripts": "bin"
                },
                "pointer_size": "64",
                "gil_disabled": true
            }
        "##};

        let json = if system {
            json.replace("{PREFIX}", "/home/ferris/.pyenv/versions/{FULL_VERSION}")
        } else {
            json.replace("{PREFIX}", "/home/ferris/projects/uv/.venv")
        };

        let json = json
            .replace(
                "{PATH}",
                path.to_str().expect("Path can be represented as string"),
            )
            .replace("{FULL_VERSION}", &version.to_string())
            .replace("{VERSION}", &version.without_patch().to_string())
            .replace("{IMPLEMENTATION}", implementation.as_str());

        fs_err::write(
            path,
            formatdoc! {r##"
            #!/bin/bash
            echo '{json}'
            "##},
        )?;

        fs_err::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o770))?;

        Ok(())
    }

    /// Create a mock Python 2 interpreter executable which returns a fixed error message mocking
    /// invocation of Python 2 with the `-I` flag as done by our query script.
    fn create_mock_python2_interpreter(path: &Path) -> Result<()> {
        let output = indoc! { r"
            Unknown option: -I
            usage: /usr/bin/python [option] ... [-c cmd | -m mod | file | -] [arg] ...
            Try `python -h` for more information.
        "};

        fs_err::write(
            path,
            formatdoc! {r##"
            #!/bin/bash
            echo '{output}' 1>&2
            "##},
        )?;

        fs_err::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o770))?;

        Ok(())
    }

    /// Create child directories in a temporary directory.
    fn create_children<P: AsRef<Path>>(tempdir: &TempDir, names: &[P]) -> Result<Vec<PathBuf>> {
        let paths: Vec<PathBuf> = names
            .iter()
            .map(|name| tempdir.child(name).to_path_buf())
            .collect();
        for path in &paths {
            fs_err::create_dir_all(path)?;
        }
        Ok(paths)
    }

    /// Create fake Python interpreters the given Python versions.
    ///
    /// Returns a search path for the mock interpreters.
    fn simple_mock_interpreters(tempdir: &TempDir, versions: &[&'static str]) -> Result<OsString> {
        let kinds: Vec<_> = versions
            .iter()
            .map(|version| (true, ImplementationName::default(), "python", *version))
            .collect();
        mock_interpreters(tempdir, kinds.as_slice())
    }

    /// Create fake Python interpreters the given Python implementations and versions.
    ///
    /// Returns a search path for the mock interpreters.
    fn mock_interpreters(
        tempdir: &TempDir,
        kinds: &[(bool, ImplementationName, &'static str, &'static str)],
    ) -> Result<OsString> {
        let names: Vec<OsString> = (0..kinds.len())
            .map(|i| OsString::from(i.to_string()))
            .collect();
        let paths = create_children(tempdir, names.as_slice())?;
        for (path, (system, implementation, executable, version)) in
            itertools::zip_eq(&paths, kinds)
        {
            let python = format!("{executable}{}", std::env::consts::EXE_SUFFIX);
            create_mock_interpreter(
                &path.join(python),
                &PythonVersion::from_str(version).unwrap(),
                *implementation,
                *system,
            )?;
        }
        Ok(env::join_paths(paths)?)
    }

    /// Create a mock virtual environment in the given directory.
    ///
    /// Returns the path to the virtual environment.
    fn mock_venv(tempdir: &TempDir, version: &'static str) -> Result<PathBuf> {
        let venv = tempdir.child(".venv");
        let executable = virtualenv_python_executable(&venv);
        fs_err::create_dir_all(
            executable
                .parent()
                .expect("A Python executable path should always have a parent"),
        )?;
        create_mock_interpreter(
            &executable,
            &PythonVersion::from_str(version).expect("A valid Python version is used for tests"),
            ImplementationName::default(),
            false,
        )?;
        venv.child("pyvenv.cfg").touch()?;
        Ok(venv.to_path_buf())
    }

    /// Create a mock conda prefix in the given directory.
    ///
    /// These are like virtual environments but they look like system interpreters because `prefix` and `base_prefix` are equal.
    ///
    /// Returns the path to the environment.
    fn mock_conda_prefix(tempdir: &TempDir, version: &'static str) -> Result<PathBuf> {
        let env = tempdir.child("conda");
        let executable = virtualenv_python_executable(&env);
        fs_err::create_dir_all(
            executable
                .parent()
                .expect("A Python executable path should always have a parent"),
        )?;
        create_mock_interpreter(
            &executable,
            &PythonVersion::from_str(version).expect("A valid Python version is used for tests"),
            ImplementationName::default(),
            true,
        )?;
        env.child("pyvenv.cfg").touch()?;
        Ok(env.to_path_buf())
    }

    #[test]
    fn find_default_interpreter_empty_path() -> Result<()> {
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                ("PATH", Some("")),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                    matches!(
                        result,
                        Ok(Err(InterpreterNotFound::NoPythonInstallation(..)))
                    ),
                    "With an empty path, no Python installation should be detected got {result:?}"
                );
            },
        );

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                ("PATH", None::<OsString>),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                    matches!(
                        result,
                        Ok(Err(InterpreterNotFound::NoPythonInstallation(..)))
                    ),
                    "With an unset path, no Python installation should be detected; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_default_interpreter_invalid_executable() -> Result<()> {
        let cache = Cache::temp()?;
        let tempdir = TempDir::new()?;
        let python = tempdir.child(format!("python{}", std::env::consts::EXE_SUFFIX));
        python.touch()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                ("PATH", Some(tempdir.path().as_os_str())),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                matches!(
                    result,
                    Ok(Err(InterpreterNotFound::NoPythonInstallation(..)))
                ),
                "With an invalid Python executable, no Python installation should be detected; got {result:?}"
            );
            },
        );

        Ok(())
    }

    #[test]
    fn find_default_interpreter_valid_executable() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let python = tempdir.child(format!("python{}", std::env::consts::EXE_SUFFIX));
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                ("PATH", Some(tempdir.path().as_os_str())),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find the valid executable; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_default_interpreter_valid_executable_after_invalid() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let children = create_children(
            &tempdir,
            &["query-parse-error", "not-executable", "good", "empty"],
        )?;

        // Just an empty file
        tempdir
            .child("not-executable")
            .child(format!("python{}", std::env::consts::EXE_SUFFIX))
            .touch()?;

        // An executable file with a bad response
        #[cfg(unix)]
        fs_err::write(
            tempdir
                .child("query-parse-error")
                .child(format!("python{}", std::env::consts::EXE_SUFFIX)),
            formatdoc! {r##"
            #!/bin/bash
            echo 'foo'
            "##},
        )?;
        fs_err::set_permissions(
            tempdir
                .child("query-parse-error")
                .child(format!("python{}", std::env::consts::EXE_SUFFIX))
                .path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o770),
        )?;

        // An interpreter
        let python = tempdir
            .child("good")
            .child(format!("python{}", std::env::consts::EXE_SUFFIX));
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(env::join_paths(
                        [tempdir.child("missing").as_os_str()]
                            .into_iter()
                            .chain(children.iter().map(|child| child.as_os_str())),
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should skip the bad executable in favor of the good one; got {result:?}"
                );
                assert_eq!(
                    result.unwrap().unwrap().interpreter().sys_executable(),
                    python.path()
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_default_interpreter_only_python2_executable() -> Result<()> {
        let tempdir = TempDir::new()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let cache = Cache::temp()?;
        let python = tempdir.child(format!("python{}", std::env::consts::EXE_SUFFIX));
        create_mock_python2_interpreter(&python)?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                ("PATH", Some(tempdir.path().as_os_str())),
                ("PWD", Some(pwd.path().as_os_str())),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                matches!(
                    result,
                    Ok(Err(InterpreterNotFound::NoPythonInstallation(..)))
                ),
                // TODO(zanieb): We could improve the error handling to hint this to the user
                "If only Python 2 is available, we should not find an interpreter; got {result:?}"
            );
            },
        );

        Ok(())
    }

    #[test]
    fn find_default_interpreter_skip_python2_executable() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("bad").create_dir_all()?;
        tempdir.child("good").create_dir_all()?;
        let python2 = tempdir
            .child("bad")
            .child(format!("python{}", std::env::consts::EXE_SUFFIX));
        create_mock_python2_interpreter(&python2)?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;

        let python3 = tempdir
            .child("good")
            .child(format!("python{}", std::env::consts::EXE_SUFFIX));
        create_mock_interpreter(
            &python3,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(env::join_paths([
                        tempdir.child("bad").as_os_str(),
                        tempdir.child("good").as_os_str(),
                    ])?),
                ),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let result = find_default_interpreter(&cache);
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should skip the Python 2 installation and find the Python 3 interpreter; got {result:?}"
                );
                assert_eq!(
                    result.unwrap().unwrap().interpreter().sys_executable(),
                    python3.path()
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_system_python_allowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (false, ImplementationName::CPython, "python", "3.10.0"),
                            (true, ImplementationName::CPython, "python", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::Any,
                    SystemPython::Allowed,
                    &SourceSelector::All,
                    &cache,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    result.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "Should find the first interpreter regardless of system"
                );
            },
        );

        // Reverse the order of the virtual environment and system
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::CPython, "python", "3.10.0"),
                            (false, ImplementationName::CPython, "python", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::Any,
                    SystemPython::Allowed,
                    &SourceSelector::All,
                    &cache,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    result.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "Should find the first interpreter regardless of system"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_system_python_required() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (false, ImplementationName::CPython, "python", "3.10.0"),
                            (true, ImplementationName::CPython, "python", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::Any,
                    SystemPython::Required,
                    &SourceSelector::All,
                    &cache,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    result.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "Should skip the virtual environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_system_python_disallowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::CPython, "python", "3.10.0"),
                            (false, ImplementationName::CPython, "python", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::Any,
                    SystemPython::Disallowed,
                    &SourceSelector::All,
                    &cache,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    result.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "Should skip the system Python"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_version_minor() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let sources = SourceSelector::All;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::parse("3.11"),
                    SystemPython::Allowed,
                    &sources,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    &result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.2",
                    "We should find the correct interpreter for the request"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_version_patch() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let sources = SourceSelector::All;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::parse("3.11.2"),
                    SystemPython::Allowed,
                    &sources,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.2",
                    "We should find the correct interpreter for the request"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_version_minor_no_match() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let sources = SourceSelector::All;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::parse("3.9"),
                    SystemPython::Allowed,
                    &sources,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Err(InterpreterNotFound::NoMatchingVersion(
                            _,
                            VersionRequest::MajorMinor(3, 9)
                        )))
                    ),
                    "We should not find an interpreter; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_interpreter_version_patch_no_match() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let sources = SourceSelector::All;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_interpreter(
                    &InterpreterRequest::parse("3.11.9"),
                    SystemPython::Allowed,
                    &sources,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Err(InterpreterNotFound::NoMatchingVersion(
                            _,
                            VersionRequest::MajorMinorPatch(3, 11, 9)
                        )))
                    ),
                    "We should not find an interpreter; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_version_patch_exact() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.11.9"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    &InterpreterRequest::parse("3.11.9"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.9",
                    "We should prefer the exact match"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_version_patch_fallback() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    &InterpreterRequest::parse("3.11.9"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.2",
                    "We should fallback to the matching minor version"
                );
            },
        );

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(
                        &tempdir,
                        &["3.10.1", "3.11.2", "3.11.8", "3.12.3"],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    &InterpreterRequest::parse("3.11.9"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.2",
                    "We fallback to the first matching minor version, not the closest patch"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_skips_broken_active_environment() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.12.0")?;
        // Delete the `pyvenv.cfg` to "break" the environment
        fs_err::remove_file(venv.join("pyvenv.cfg"))?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.11.1", "3.12.3"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    // TODO(zanieb): Consider moving this test to `PythonEnvironment::find` instead
                    &InterpreterRequest::parse("3.12"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::ActiveEnvironment,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.12.0",
                    "We should prefer the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_skips_source_without_match() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.12.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    // TODO(zanieb): Consider moving this test to `PythonEnvironment::find` instead
                    &InterpreterRequest::parse("3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::SearchPath,
                            interpreter: _
                        }))
                    ),
                    "We should skip the active environment in favor of the requested version; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.10.1",
                    "We should prefer the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_returns_to_earlier_source_on_fallback() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.10.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.3"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    // TODO(zanieb): Consider moving this test to `PythonEnvironment::find` instead
                    &InterpreterRequest::parse("3.10.2"),
                    crate::SystemPython::Allowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::ActiveEnvironment,
                            interpreter: _
                        }))
                    ),
                    "We should prefer to the active environment after relaxing; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.10.0",
                    "We should prefer the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_best_interpreter_virtualenv_used_if_system_not_allowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.11.1")?;

        // Matching minor version
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.11.2"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.clone().into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    // Request the search path Python with a matching minor
                    &InterpreterRequest::parse("3.11.2"),
                    crate::SystemPython::Disallowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::ActiveEnvironment,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.1",
                    "We should use the active environment"
                );
            },
        );

        // Matching major version
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.11.2", "3.10.0"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = find_best_interpreter(
                    // Request the search path Python with a matching minor
                    &InterpreterRequest::parse("3.10.2"),
                    crate::SystemPython::Disallowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Ok(Ok(DiscoveredInterpreter {
                            source: InterpreterSource::ActiveEnvironment,
                            interpreter: _
                        }))
                    ),
                    "We should find an interpreter; got {result:?}"
                );
                assert_eq!(
                    result
                        .unwrap()
                        .unwrap()
                        .interpreter()
                        .python_full_version()
                        .to_string(),
                    "3.11.1",
                    "We should use the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_active_environment() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.12.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.11.2"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Allowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.0",
                    "We should prefer the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_conda_prefix() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let conda_prefix = mock_conda_prefix(&tempdir, "3.12.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.11.2"])?),
                ),
                ("CONDA_PREFIX", Some(conda_prefix.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    // Note this environment is not treated as a system interpreter
                    PythonEnvironment::find(None, SystemPython::Disallowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.0",
                    "We should allow the conda environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_conda_prefix_and_virtualenv() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let generic = mock_venv(&tempdir, "3.12.0")?;
        let conda = mock_conda_prefix(&tempdir, "3.12.1")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.2", "3.11.3"])?),
                ),
                ("CONDA_PREFIX", Some(conda.into())),
                ("VIRTUAL_ENV", Some(generic.into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    // Note this environment is not treated as a system interpreter
                    PythonEnvironment::find(None, SystemPython::Disallowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.0",
                    "We should prefer the active environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_discovered_environment() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let _venv = mock_venv(&tempdir, "3.12.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.11.2"])?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Allowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.0",
                    "We should prefer the discovered environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_parent_interpreter() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let venv = mock_venv(&tempdir, "3.12.0")?;
        let python = tempdir.child("python").to_path_buf();
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::CPython,
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.12.2", "3.12.3"])?),
                ),
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(python.into())),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Allowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.1",
                    "We should prefer parent interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_parent_interpreter_system_explicit() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let venv = mock_venv(&tempdir, "3.12.0")?;
        let python = tempdir.child("python").to_path_buf();
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::CPython,
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.12.2", "3.12.3"])?),
                ),
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(python.into())),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Explicit, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.1",
                    "We prefer the parent interpreter even though it is system"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_parent_interpreter_system_disallowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let venv = mock_venv(&tempdir, "3.12.0")?;
        let python = tempdir.child("python").to_path_buf();
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::CPython,
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.12.2", "3.12.3"])?),
                ),
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(python.into())),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Disallowed, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.0",
                    "We find the virtual environment Python because the system is explicitly not allowed"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_from_parent_interpreter_system_required() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let venv = mock_venv(&tempdir, "3.12.0")?;
        let python = tempdir.child("python").to_path_buf();
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::CPython,
            false,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.12.2", "3.12.3"])?),
                ),
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(python.into())),
                ("VIRTUAL_ENV", Some(venv.into())),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Required, &cache)
                        .expect("An environment is found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.2",
                    "We should skip the parent interpreter since its in a virtual environment"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_active_environment_skipped_if_system_required() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        let venv = mock_venv(&tempdir, "3.12.0")?;

        // Without a request
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.11.2"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.clone().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(None, crate::SystemPython::Required, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "We should skip the active environment"
                );
            },
        );

        // With a requested version
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.12.2"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.clone().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("3.12"), crate::SystemPython::Required, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.12.2",
                    "We should skip the active environment"
                );
            },
        );

        // Request a patch version that cannot be found
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.12.2"])?),
                ),
                ("VIRTUAL_ENV", Some(venv.clone().into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result =
                    PythonEnvironment::find(Some("3.12.3"), crate::SystemPython::Required, &cache);
                assert!(
                    result.is_err(),
                    "We should not find an environment; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_fails_if_no_virtualenv_and_system_not_allowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None),
                ("UV_BOOTSTRAP_DIR", None),
                (
                    "PATH",
                    Some(simple_mock_interpreters(&tempdir, &["3.10.1", "3.11.2"])?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = PythonEnvironment::find(None, crate::SystemPython::Disallowed, &cache);
                assert!(
                    result.is_err(),
                    "We should not find an environment; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_allows_name_in_working_directory() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        let python = tempdir.join("foobar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("foobar"), crate::SystemPython::Allowed, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the `foobar` executable"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_allows_relative_file_path() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("foo").create_dir_all()?;
        let python = tempdir.child("foo").join("bar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("./foo/bar"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the `bar` interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_allows_absolute_file_path() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("foo").create_dir_all()?;
        let python = tempdir.child("foo").join("bar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some(python.to_str().expect("Test path is valid unicode")),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the `bar` interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_allows_venv_directory_path() -> Result<()> {
        let tempdir = TempDir::new()?;
        // Create a separate pwd to avoid ancestor discovery of the venv
        let pwd = TempDir::new()?;
        let cache = Cache::temp()?;
        let venv = mock_venv(&tempdir, "3.10.0")?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some(venv.to_str().expect("Test path is valid unicode")),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the venv interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_allows_file_path_with_system_explicit() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("foo").create_dir_all()?;
        let python = tempdir.child("foo").join("bar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some(python.to_str().expect("Test path is valid unicode")),
                    crate::SystemPython::Explicit,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_does_not_allow_file_path_with_system_disallowed() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("foo").create_dir_all()?;
        let python = tempdir.child("foo").join("bar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result = PythonEnvironment::find(
                    Some(python.to_str().expect("Test path is valid unicode")),
                    crate::SystemPython::Disallowed,
                    &cache,
                );
                assert!(
                    matches!(
                        result,
                        Err(Error::Discovery(discovery::Error::SourceNotSelected(
                            _,
                            InterpreterSource::ProvidedPath
                        )))
                    ),
                    "We should complain that provided paths are not allowed; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_treats_missing_file_path_as_file() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;
        tempdir.child("foo").create_dir_all()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", None),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let result: std::prelude::v1::Result<PythonEnvironment, crate::Error> =
                    PythonEnvironment::find(
                        Some("./foo/bar"),
                        crate::SystemPython::Allowed,
                        &cache,
                    );
                assert!(
                    matches!(
                        result,
                        Err(Error::NotFound(InterpreterNotFound::FileNotFound(_)))
                    ),
                    "We should not find the file; got {result:?}"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_executable_name_in_search_path() -> Result<()> {
        let tempdir = TempDir::new()?;
        let pwd = tempdir.child("pwd");
        pwd.create_dir_all()?;
        let cache = Cache::temp()?;
        let python = tempdir.join("foobar");
        create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", Some(tempdir.path().into())),
                ("PWD", Some(pwd.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("foobar"), crate::SystemPython::Required, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                    "We should find the `foobar` executable"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[(true, ImplementationName::PyPy, "pypy", "3.10.1")],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("pypy"), crate::SystemPython::Allowed, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "We should find the pypy interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy_request_ignores_cpython() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::CPython, "python", "3.10.0"),
                            (true, ImplementationName::PyPy, "pypy", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("pypy"), crate::SystemPython::Allowed, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "We should skip the CPython interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy_request_skips_wrong_versions() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        // We should prefer the `pypy` executable with the requested version
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::PyPy, "pypy", "3.9"),
                            (true, ImplementationName::PyPy, "pypy", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment =
                    PythonEnvironment::find(Some("pypy3.10"), crate::SystemPython::Allowed, &cache)
                        .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "We should skip the first interpreter"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy_finds_executable_with_version_name() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        // We should find executables that include the version number
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::PyPy, "pypy3.9", "3.10.0"), // We don't consider this one because of the executable name
                            (true, ImplementationName::PyPy, "pypy3.10", "3.10.1"),
                            (true, ImplementationName::PyPy, "pypy", "3.10.2"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("pypy@3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                    "We should find the one with the requested version"
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy_prefers_executable_with_implementation_name() -> Result<()> {
        let tempdir = TempDir::new()?;
        let cache = Cache::temp()?;

        // We should prefer `pypy` executables over `python` executables even if they are both pypy
        create_mock_interpreter(
            &tempdir.path().join("python"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        create_mock_interpreter(
            &tempdir.path().join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", Some(tempdir.path().into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("pypy@3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                );
            },
        );

        // But we should not prefer `pypy` executables over `python` executables that
        // appear earlier in the search path
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                (
                    "PATH",
                    Some(mock_interpreters(
                        &tempdir,
                        &[
                            (true, ImplementationName::PyPy, "python", "3.10.0"),
                            (true, ImplementationName::PyPy, "pypy", "3.10.1"),
                        ],
                    )?),
                ),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("pypy@3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                );
            },
        );

        Ok(())
    }

    #[test]
    fn find_environment_pypy_prefers_executable_with_version() -> Result<()> {
        let cache = Cache::temp()?;

        // We should prefer executables with the version number over those with implementation names
        let tempdir = TempDir::new()?;
        create_mock_interpreter(
            &tempdir.path().join("pypy3.10"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        create_mock_interpreter(
            &tempdir.path().join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", Some(tempdir.path().into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("pypy@3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.0",
                );
            },
        );

        // But we'll prefer an implementation name executable over a generic name with a version
        let tempdir = TempDir::new()?;
        create_mock_interpreter(
            &tempdir.path().join("python3.10"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        create_mock_interpreter(
            &tempdir.path().join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        with_vars(
            [
                ("UV_TEST_PYTHON_PATH", None::<OsString>),
                ("PATH", Some(tempdir.path().into())),
                ("PWD", Some(tempdir.path().into())),
            ],
            || {
                let environment = PythonEnvironment::find(
                    Some("pypy@3.10"),
                    crate::SystemPython::Allowed,
                    &cache,
                )
                .expect("Environment should be found");
                assert_eq!(
                    environment.interpreter().python_full_version().to_string(),
                    "3.10.1",
                );
            },
        );

        Ok(())
    }
}
