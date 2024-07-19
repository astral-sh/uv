//! Find requested Python interpreters and query interpreters for information.
use thiserror::Error;

pub use crate::discovery::{
    find_python_installations, EnvironmentPreference, Error as DiscoveryError, PythonFetch,
    PythonNotFound, PythonPreference, PythonRequest, PythonSource, VersionRequest,
};
pub use crate::environment::PythonEnvironment;
pub use crate::implementation::ImplementationName;
pub use crate::installation::{PythonInstallation, PythonInstallationKey};
pub use crate::interpreter::{Error as InterpreterError, Interpreter};
pub use crate::pointer_size::PointerSize;
pub use crate::prefix::Prefix;
pub use crate::python_version::PythonVersion;
pub use crate::target::Target;
pub use crate::version_files::{
    request_from_version_file, requests_from_version_file, write_version_file,
    PYTHON_VERSIONS_FILENAME, PYTHON_VERSION_FILENAME,
};
pub use crate::virtualenv::{Error as VirtualEnvError, PyVenvConfiguration, VirtualEnvironment};

mod discovery;
pub mod downloads;
mod environment;
mod implementation;
mod installation;
mod interpreter;
pub mod managed;
pub mod platform;
mod pointer_size;
mod prefix;
mod py_launcher;
mod python_version;
mod target;
mod version_files;
mod virtualenv;
mod which;

#[cfg(not(test))]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::current_dir()
}

#[cfg(test)]
pub(crate) fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::var_os("PWD")
        .map(std::path::PathBuf::from)
        .map(Ok)
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
    ManagedPython(#[from] managed::Error),

    #[error(transparent)]
    Download(#[from] downloads::Error),

    // TODO(zanieb) We might want to ensure this is always wrapped in another type
    #[error(transparent)]
    KeyError(#[from] installation::PythonInstallationKeyError),

    #[error(transparent)]
    MissingPython(#[from] PythonNotFound),

    #[error(transparent)]
    MissingEnvironment(#[from] environment::EnvironmentNotFound),
}

// The mock interpreters are not valid on Windows so we don't have unit test coverage there
// TODO(zanieb): We should write a mock interpreter script that works on Windows
#[cfg(all(test, unix))]
mod tests {
    use std::{
        env,
        ffi::{OsStr, OsString},
        path::{Path, PathBuf},
        str::FromStr,
    };

    use anyhow::Result;
    use assert_fs::{fixture::ChildPath, prelude::*, TempDir};
    use indoc::{formatdoc, indoc};
    use temp_env::with_vars;
    use test_log::test;

    use uv_cache::Cache;

    use crate::{
        discovery::{
            find_best_python_installation, find_python_installation, EnvironmentPreference,
        },
        PythonPreference,
    };
    use crate::{
        implementation::ImplementationName, installation::PythonInstallation,
        managed::ManagedPythonInstallations, virtualenv::virtualenv_python_executable,
        PythonNotFound, PythonRequest, PythonSource, PythonVersion,
    };

    struct TestContext {
        tempdir: TempDir,
        cache: Cache,
        installations: ManagedPythonInstallations,
        search_path: Option<Vec<PathBuf>>,
        workdir: ChildPath,
    }

    impl TestContext {
        fn new() -> Result<Self> {
            let tempdir = TempDir::new()?;
            let workdir = tempdir.child("workdir");
            workdir.create_dir_all()?;

            Ok(Self {
                tempdir,
                cache: Cache::temp()?,
                installations: ManagedPythonInstallations::temp()?,
                search_path: None,
                workdir,
            })
        }

        /// Clear the search path.
        fn reset_search_path(&mut self) {
            self.search_path = None;
        }

        /// Add a directory to the search path.
        fn add_to_search_path(&mut self, path: PathBuf) {
            match self.search_path.as_mut() {
                Some(paths) => paths.push(path),
                None => self.search_path = Some(vec![path]),
            };
        }

        /// Create a new directory and add it to the search path.
        fn new_search_path_directory(&mut self, name: impl AsRef<Path>) -> Result<ChildPath> {
            let child = self.tempdir.child(name);
            child.create_dir_all()?;
            self.add_to_search_path(child.to_path_buf());
            Ok(child)
        }

        fn run<F, R>(&self, closure: F) -> R
        where
            F: FnOnce() -> R,
        {
            self.run_with_vars(&[], closure)
        }

        fn run_with_vars<F, R>(&self, vars: &[(&str, Option<&OsStr>)], closure: F) -> R
        where
            F: FnOnce() -> R,
        {
            let path = self
                .search_path
                .as_ref()
                .map(|paths| env::join_paths(paths).unwrap());

            let mut run_vars = vec![
                // Ensure `PATH` is used
                ("UV_TEST_PYTHON_PATH", None),
                // Ignore active virtual environments (i.e. that the dev is using)
                ("VIRTUAL_ENV", None),
                ("PATH", path.as_deref()),
                // Use the temporary python directory
                (
                    "UV_PYTHON_INSTALL_DIR",
                    Some(self.installations.root().as_os_str()),
                ),
                // Set a working directory
                ("PWD", Some(self.workdir.path().as_os_str())),
            ];
            for (key, value) in vars {
                run_vars.push((key, *value));
            }
            with_vars(&run_vars, closure)
        }

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
                        "sys_base_exec_prefix": "/home/ferris/.pyenv/versions/{FULL_VERSION}",
                        "sys_base_prefix": "/home/ferris/.pyenv/versions/{FULL_VERSION}",
                        "sys_prefix": "{PREFIX}",
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
                .replace("{IMPLEMENTATION}", (&implementation).into());

            fs_err::create_dir_all(path.parent().unwrap())?;
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
        fn new_search_path_directories(
            &mut self,
            names: &[impl AsRef<Path>],
        ) -> Result<Vec<ChildPath>> {
            let paths = names
                .iter()
                .map(|name| self.new_search_path_directory(name))
                .collect::<Result<Vec<_>>>()?;
            Ok(paths)
        }

        /// Create fake Python interpreters the given Python versions.
        ///
        /// Adds them to the test context search path.
        fn add_python_to_workdir(&self, name: &str, version: &str) -> Result<()> {
            TestContext::create_mock_interpreter(
                self.workdir.child(name).as_ref(),
                &PythonVersion::from_str(version).expect("Test uses valid version"),
                ImplementationName::default(),
                true,
            )
        }

        /// Create fake Python interpreters the given Python versions.
        ///
        /// Adds them to the test context search path.
        fn add_python_versions(&mut self, versions: &[&'static str]) -> Result<()> {
            let interpreters: Vec<_> = versions
                .iter()
                .map(|version| (true, ImplementationName::default(), "python", *version))
                .collect();
            self.add_python_interpreters(interpreters.as_slice())
        }

        /// Create fake Python interpreters the given Python implementations and versions.
        ///
        /// Adds them to the test context search path.
        fn add_python_interpreters(
            &mut self,
            kinds: &[(bool, ImplementationName, &'static str, &'static str)],
        ) -> Result<()> {
            // Generate a "unique" folder name for each interpreter
            let names: Vec<OsString> = kinds
                .iter()
                .map(|(system, implementation, name, version)| {
                    OsString::from_str(&format!("{system}-{implementation}-{name}-{version}"))
                        .unwrap()
                })
                .collect();
            let paths = self.new_search_path_directories(names.as_slice())?;
            for (path, (system, implementation, executable, version)) in
                itertools::zip_eq(&paths, kinds)
            {
                let python = format!("{executable}{}", env::consts::EXE_SUFFIX);
                Self::create_mock_interpreter(
                    &path.join(python),
                    &PythonVersion::from_str(version).unwrap(),
                    *implementation,
                    *system,
                )?;
            }
            Ok(())
        }

        /// Create a mock virtual environment at the given directory
        fn mock_venv(path: impl AsRef<Path>, version: &'static str) -> Result<()> {
            let executable = virtualenv_python_executable(path.as_ref());
            fs_err::create_dir_all(
                executable
                    .parent()
                    .expect("A Python executable path should always have a parent"),
            )?;
            TestContext::create_mock_interpreter(
                &executable,
                &PythonVersion::from_str(version)
                    .expect("A valid Python version is used for tests"),
                ImplementationName::default(),
                false,
            )?;
            ChildPath::new(path.as_ref().join("pyvenv.cfg")).touch()?;
            Ok(())
        }

        /// Create a mock conda prefix at the given directory.
        ///
        /// These are like virtual environments but they look like system interpreters because `prefix` and `base_prefix` are equal.
        fn mock_conda_prefix(path: impl AsRef<Path>, version: &'static str) -> Result<()> {
            let executable = virtualenv_python_executable(&path);
            fs_err::create_dir_all(
                executable
                    .parent()
                    .expect("A Python executable path should always have a parent"),
            )?;
            TestContext::create_mock_interpreter(
                &executable,
                &PythonVersion::from_str(version)
                    .expect("A valid Python version is used for tests"),
                ImplementationName::default(),
                true,
            )?;
            ChildPath::new(path.as_ref().join("pyvenv.cfg")).touch()?;
            Ok(())
        }
    }

    #[test]
    fn find_python_empty_path() -> Result<()> {
        let mut context = TestContext::new()?;

        context.search_path = Some(vec![]);
        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        });
        assert!(
            matches!(result, Ok(Err(PythonNotFound { .. }))),
            "With an empty path, no Python installation should be detected got {result:?}"
        );

        context.search_path = None;
        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        });
        assert!(
            matches!(result, Ok(Err(PythonNotFound { .. }))),
            "With an unset path, no Python installation should be detected got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_unexecutable_file() -> Result<()> {
        let mut context = TestContext::new()?;
        context
            .new_search_path_directory("path")?
            .child(format!("python{}", env::consts::EXE_SUFFIX))
            .touch()?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        });
        assert!(
            matches!(
                result,
                Ok(Err(PythonNotFound { .. }))
            ),
            "With an non-executable Python, no Python installation should be detected; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_valid_executable() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.12.1"])?;

        let interpreter = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        })??;
        assert!(
            matches!(
                interpreter,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should find the valid executable; got {interpreter:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_valid_executable_after_invalid() -> Result<()> {
        let mut context = TestContext::new()?;
        let children = context.new_search_path_directories(&[
            "query-parse-error",
            "not-executable",
            "empty",
            "good",
        ])?;

        // An executable file with a bad response
        #[cfg(unix)]
        fs_err::write(
            children[0].join(format!("python{}", env::consts::EXE_SUFFIX)),
            formatdoc! {r##"
            #!/bin/bash
            echo 'foo'
            "##},
        )?;
        fs_err::set_permissions(
            children[0].join(format!("python{}", env::consts::EXE_SUFFIX)),
            std::os::unix::fs::PermissionsExt::from_mode(0o770),
        )?;

        // A non-executable file
        ChildPath::new(children[1].join(format!("python{}", env::consts::EXE_SUFFIX))).touch()?;

        // An empty directory at `children[2]`

        // An good interpreter!
        let python_path = children[3].join(format!("python{}", env::consts::EXE_SUFFIX));
        TestContext::create_mock_interpreter(
            &python_path,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        })??;
        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should skip the bad executables in favor of the good one; got {python:?}"
        );
        assert_eq!(python.interpreter().sys_executable(), python_path);

        Ok(())
    }

    #[test]
    fn find_python_only_python2_executable() -> Result<()> {
        let mut context = TestContext::new()?;
        let python = context
            .new_search_path_directory("python2")?
            .child(format!("python{}", env::consts::EXE_SUFFIX));
        TestContext::create_mock_python2_interpreter(&python)?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            // TODO(zanieb): We could improve the error handling to hint this to the user
            "If only Python 2 is available, we should not find a python; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_skip_python2_executable() -> Result<()> {
        let mut context = TestContext::new()?;

        let python2 = context
            .new_search_path_directory("python2")?
            .child(format!("python{}", env::consts::EXE_SUFFIX));
        TestContext::create_mock_python2_interpreter(&python2)?;

        let python3 = context
            .new_search_path_directory("python3")?
            .child(format!("python{}", env::consts::EXE_SUFFIX));
        TestContext::create_mock_interpreter(
            &python3,
            &PythonVersion::from_str("3.12.1").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::default(),
                &context.cache,
            )
        })??;
        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should skip the Python 2 installation and find the Python 3 interpreter; got {python:?}"
        );
        assert_eq!(python.interpreter().sys_executable(), python3.path());

        Ok(())
    }

    #[test]
    fn find_python_system_python_allowed() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (false, ImplementationName::CPython, "python", "3.10.0"),
            (true, ImplementationName::CPython, "python", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "Should find the first interpreter regardless of system"
        );

        // Reverse the order of the virtual environment and system
        context.reset_search_path();
        context.add_python_interpreters(&[
            (true, ImplementationName::CPython, "python", "3.10.1"),
            (false, ImplementationName::CPython, "python", "3.10.0"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "Should find the first interpreter regardless of system"
        );

        Ok(())
    }

    #[test]
    fn find_python_system_python_required() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (false, ImplementationName::CPython, "python", "3.10.0"),
            (true, ImplementationName::CPython, "python", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "Should skip the virtual environment"
        );

        Ok(())
    }

    #[test]
    fn find_python_system_python_disallowed() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::CPython, "python", "3.10.0"),
            (false, ImplementationName::CPython, "python", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "Should skip the system Python"
        );

        Ok(())
    }

    #[test]
    fn find_python_version_minor() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2", "3.12.3"])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("3.11"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should find a python; got {python:?}"
        );
        assert_eq!(
            &python.interpreter().python_full_version().to_string(),
            "3.11.2",
            "We should find the correct interpreter for the request"
        );

        Ok(())
    }

    #[test]
    fn find_python_version_patch() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.3", "3.11.2", "3.12.3"])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("3.11.2"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should find a python; got {python:?}"
        );
        assert_eq!(
            &python.interpreter().python_full_version().to_string(),
            "3.11.2",
            "We should find the correct interpreter for the request"
        );

        Ok(())
    }

    #[test]
    fn find_python_version_minor_no_match() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2", "3.12.3"])?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("3.9"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find a python; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_version_patch_no_match() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2", "3.12.3"])?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("3.11.9"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find a python; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_best_python_version_patch_exact() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2", "3.11.4", "3.11.3", "3.12.5"])?;

        let python = context.run(|| {
            find_best_python_installation(
                &PythonRequest::parse("3.11.3"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should find a python; got {python:?}"
        );
        assert_eq!(
            &python.interpreter().python_full_version().to_string(),
            "3.11.3",
            "We should prefer the exact request"
        );

        Ok(())
    }

    #[test]
    fn find_best_python_version_patch_fallback() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2", "3.11.4", "3.11.3", "3.12.5"])?;

        let python = context.run(|| {
            find_best_python_installation(
                &PythonRequest::parse("3.11.11"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should find a python; got {python:?}"
        );
        assert_eq!(
            &python.interpreter().python_full_version().to_string(),
            "3.11.2",
            "We should fallback to the first matching minor"
        );

        Ok(())
    }

    #[test]
    fn find_best_python_skips_source_without_match() -> Result<()> {
        let mut context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.12.0")?;
        context.add_python_versions(&["3.10.1"])?;

        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_best_python_installation(
                    &PythonRequest::parse("3.10"),
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::SearchPath,
                    interpreter: _
                }
            ),
            "We should skip the active environment in favor of the requested version; got {python:?}"
        );

        Ok(())
    }

    #[test]
    fn find_best_python_returns_to_earlier_source_on_fallback() -> Result<()> {
        let mut context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.10.1")?;
        context.add_python_versions(&["3.10.3"])?;

        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_best_python_installation(
                    &PythonRequest::parse("3.10.2"),
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert!(
            matches!(
                python,
                PythonInstallation {
                    source: PythonSource::ActiveEnvironment,
                    interpreter: _
                }
            ),
            "We should prefer the active environment after relaxing; got {python:?}"
        );
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should prefer the active environment"
        );

        Ok(())
    }

    #[test]
    fn find_python_from_active_python() -> Result<()> {
        let context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.12.0")?;

        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should prefer the active environment"
        );

        Ok(())
    }

    #[test]
    fn find_python_from_conda_prefix() -> Result<()> {
        let context = TestContext::new()?;
        let condaenv = context.tempdir.child("condaenv");
        TestContext::mock_conda_prefix(&condaenv, "3.12.0")?;

        let python =
            context.run_with_vars(&[("CONDA_PREFIX", Some(condaenv.as_os_str()))], || {
                // Note this python is not treated as a system interpreter
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::OnlyVirtual,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should allow the active conda python"
        );

        Ok(())
    }

    #[test]
    fn find_python_from_conda_prefix_and_virtualenv() -> Result<()> {
        let context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.12.0")?;
        let condaenv = context.tempdir.child("condaenv");
        TestContext::mock_conda_prefix(&condaenv, "3.12.1")?;

        let python = context.run_with_vars(
            &[
                ("VIRTUAL_ENV", Some(venv.as_os_str())),
                ("CONDA_PREFIX", Some(condaenv.as_os_str())),
            ],
            || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should prefer the non-conda python"
        );

        // Put a virtual environment in the working directory
        let venv = context.workdir.child(".venv");
        TestContext::mock_venv(venv, "3.12.2")?;
        let python =
            context.run_with_vars(&[("CONDA_PREFIX", Some(condaenv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.1",
            "We should prefer the conda python over inactive virtual environments"
        );

        Ok(())
    }

    #[test]
    fn find_python_from_discovered_python() -> Result<()> {
        let mut context = TestContext::new()?;

        // Create a virtual environment in a parent of the workdir
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(venv, "3.12.0")?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should find the python"
        );

        // Add some system versions to ensure we don't use those
        context.add_python_versions(&["3.12.1", "3.12.2"])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;

        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should prefer the discovered virtual environment over available system versions"
        );

        Ok(())
    }

    #[test]
    fn find_python_skips_broken_active_python() -> Result<()> {
        let context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.12.0")?;

        // Delete the pyvenv cfg to break the virtualenv
        fs_err::remove_file(venv.join("pyvenv.cfg"))?;

        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            // TODO(zanieb): We should skip this python, why don't we?
            "We should prefer the active environment"
        );

        Ok(())
    }

    #[test]
    fn find_python_from_parent_interpreter() -> Result<()> {
        let mut context = TestContext::new()?;

        let parent = context.tempdir.child("python").to_path_buf();
        TestContext::create_mock_interpreter(
            &parent,
            &PythonVersion::from_str("3.12.0").unwrap(),
            ImplementationName::CPython,
            // Note we mark this as a system interpreter instead of a virtual environment
            true,
        )?;

        let python = context.run_with_vars(
            &[("UV_INTERNAL__PARENT_INTERPRETER", Some(parent.as_os_str()))],
            || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should find the parent interpreter"
        );

        // Parent interpreters are preferred over virtual environments and system interpreters
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.12.2")?;
        context.add_python_versions(&["3.12.3"])?;
        let python = context.run_with_vars(
            &[
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(parent.as_os_str())),
                ("VIRTUAL_ENV", Some(venv.as_os_str())),
            ],
            || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should prefer the parent interpreter"
        );

        // Test with `EnvironmentPreference::ExplicitSystem`
        let python = context.run_with_vars(
            &[
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(parent.as_os_str())),
                ("VIRTUAL_ENV", Some(venv.as_os_str())),
            ],
            || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::ExplicitSystem,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should prefer the parent interpreter"
        );

        // Test with `EnvironmentPreference::OnlyVirtual`
        let python = context.run_with_vars(
            &[
                ("UV_INTERNAL__PARENT_INTERPRETER", Some(parent.as_os_str())),
                ("VIRTUAL_ENV", Some(venv.as_os_str())),
            ],
            || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::OnlyVirtual,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.2",
            "We find the virtual environment Python because a system is explicitly not allowed"
        );

        Ok(())
    }

    #[test]
    fn find_python_active_python_skipped_if_system_required() -> Result<()> {
        let mut context = TestContext::new()?;
        let venv = context.tempdir.child(".venv");
        TestContext::mock_venv(&venv, "3.9.0")?;
        context.add_python_versions(&["3.10.0", "3.11.1", "3.12.2"])?;

        // Without a specific request
        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::Any,
                    EnvironmentPreference::OnlySystem,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should skip the active environment"
        );

        // With a requested minor version
        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::parse("3.12"),
                    EnvironmentPreference::OnlySystem,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.2",
            "We should skip the active environment"
        );

        // With a patch version that cannot be python
        let result = context.run_with_vars(&[("VIRTUAL_ENV", Some(venv.as_os_str()))], || {
            find_python_installation(
                &PythonRequest::parse("3.12.3"),
                EnvironmentPreference::OnlySystem,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            result.is_err(),
            "We should not find an python; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_fails_if_no_virtualenv_and_system_not_allowed() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_versions(&["3.10.1", "3.11.2"])?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::OnlyVirtual,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find an python; got {result:?}"
        );

        // With an invalid virtual environment variable
        let result = context.run_with_vars(
            &[("VIRTUAL_ENV", Some(context.tempdir.as_os_str()))],
            || {
                find_python_installation(
                    &PythonRequest::parse("3.12.3"),
                    EnvironmentPreference::OnlySystem,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            },
        )?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find an python; got {result:?}"
        );
        Ok(())
    }

    #[test]
    fn find_python_allows_name_in_working_directory() -> Result<()> {
        let context = TestContext::new()?;
        context.add_python_to_workdir("foobar", "3.10.0")?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("foobar"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the named executable"
        );

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find it without a specific request"
        );

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("3.10.0"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find it via a matching version request"
        );

        Ok(())
    }

    #[test]
    fn find_python_allows_relative_file_path() -> Result<()> {
        let mut context = TestContext::new()?;
        let python = context.workdir.child("foo").join("bar");
        TestContext::create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("./foo/bar"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the `bar` executable"
        );

        context.add_python_versions(&["3.11.1"])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("./foo/bar"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should prefer the `bar` executable over the system and virtualenvs"
        );

        Ok(())
    }

    #[test]
    fn find_python_allows_absolute_file_path() -> Result<()> {
        let mut context = TestContext::new()?;
        let python_path = context.tempdir.child("foo").join("bar");
        TestContext::create_mock_interpreter(
            &python_path,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(python_path.to_str().unwrap()),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the `bar` executable"
        );

        // With `EnvironmentPreference::ExplicitSystem`
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(python_path.to_str().unwrap()),
                EnvironmentPreference::ExplicitSystem,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should allow the `bar` executable with explicit system"
        );

        // With `EnvironmentPreference::OnlyVirtual`
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(python_path.to_str().unwrap()),
                EnvironmentPreference::OnlyVirtual,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should allow the `bar` executable and verify it is virtual"
        );

        context.add_python_versions(&["3.11.1"])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(python_path.to_str().unwrap()),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should prefer the `bar` executable over the system and virtualenvs"
        );

        Ok(())
    }

    #[test]
    fn find_python_allows_venv_directory_path() -> Result<()> {
        let mut context = TestContext::new()?;

        let venv = context.tempdir.child("foo").child(".venv");
        TestContext::mock_venv(&venv, "3.10.0")?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("../foo/.venv"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the relative venv path"
        );

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(venv.to_str().unwrap()),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the absolute venv path"
        );

        // We should allow it to be a directory that _looks_ like a virtual environment.
        let python_path = context.tempdir.child("bar").join("bin").join("python");
        TestContext::create_mock_interpreter(
            &python_path,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(context.tempdir.child("bar").to_str().unwrap()),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the executable in the directory"
        );

        let other_venv = context.tempdir.child("foobar").child(".venv");
        TestContext::mock_venv(&other_venv, "3.11.1")?;
        context.add_python_versions(&["3.12.2"])?;
        let python =
            context.run_with_vars(&[("VIRTUAL_ENV", Some(other_venv.as_os_str()))], || {
                find_python_installation(
                    &PythonRequest::parse(venv.to_str().unwrap()),
                    EnvironmentPreference::Any,
                    PythonPreference::OnlySystem,
                    &context.cache,
                )
            })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should prefer the requested directory over the system and active virtual environments"
        );

        Ok(())
    }

    #[test]
    fn find_python_treats_missing_file_path_as_file() -> Result<()> {
        let context = TestContext::new()?;
        context.workdir.child("foo").create_dir_all()?;

        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("./foo/bar"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not find the file; got {result:?}"
        );

        Ok(())
    }

    #[test]
    fn find_python_executable_name_in_search_path() -> Result<()> {
        let mut context = TestContext::new()?;
        let python = context.tempdir.child("foo").join("bar");
        TestContext::create_mock_interpreter(
            &python,
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::default(),
            true,
        )?;
        context.add_to_search_path(context.tempdir.child("foo").to_path_buf());

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("bar"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should find the `bar` executable"
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy() -> Result<()> {
        let mut context = TestContext::new()?;

        context.add_python_interpreters(&[(true, ImplementationName::PyPy, "pypy", "3.10.0")])?;
        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not the pypy interpreter if not named `python` or requested; got {result:?}"
        );

        // But we should find it
        context.reset_search_path();
        context.add_python_interpreters(&[(true, ImplementationName::PyPy, "python", "3.10.1")])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should find the pypy interpreter if it's the only one"
        );

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should find the pypy interpreter if it's requested"
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy_request_ignores_cpython() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::CPython, "python", "3.10.0"),
            (true, ImplementationName::PyPy, "pypy", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should skip the CPython interpreter"
        );

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should take the first interpreter without a specific request"
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy_request_skips_wrong_versions() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::PyPy, "pypy", "3.9"),
            (true, ImplementationName::PyPy, "pypy", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should skip the first interpreter"
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy_finds_executable_with_version_name() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::PyPy, "pypy3.9", "3.10.0"), // We don't consider this one because of the executable name
            (true, ImplementationName::PyPy, "pypy3.10", "3.10.1"),
            (true, ImplementationName::PyPy, "pypy", "3.10.2"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should find the requested interpreter version"
        );

        Ok(())
    }

    #[test]
    fn find_python_all_minors() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::CPython, "python", "3.10.0"),
            (true, ImplementationName::CPython, "python3", "3.10.0"),
            (true, ImplementationName::CPython, "python3.12", "3.12.0"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse(">= 3.11"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.12.0",
            "We should find matching minor version even if they aren't called `python` or `python3`"
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy_prefers_executable_with_implementation_name() -> Result<()> {
        let mut context = TestContext::new()?;

        // We should prefer `pypy` executables over `python` executables in the same directory
        // even if they are both pypy
        TestContext::create_mock_interpreter(
            &context.tempdir.join("python"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        context.add_to_search_path(context.tempdir.to_path_buf());

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
        );

        // But `python` executables earlier in the search path will take precedence
        context.reset_search_path();
        context.add_python_interpreters(&[
            (true, ImplementationName::PyPy, "python", "3.10.2"),
            (true, ImplementationName::PyPy, "pypy", "3.10.3"),
        ])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.2",
        );

        Ok(())
    }

    #[test]
    fn find_python_pypy_prefers_executable_with_version() -> Result<()> {
        let mut context = TestContext::new()?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("pypy3.10"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        context.add_to_search_path(context.tempdir.to_path_buf());

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should prefer executables with the version number over those with implementation names"
        );

        let mut context = TestContext::new()?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("python3.10"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("pypy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::PyPy,
            true,
        )?;
        context.add_to_search_path(context.tempdir.to_path_buf());

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("pypy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should prefer an implementation name executable over a generic name with a version"
        );

        Ok(())
    }

    #[test]
    fn find_python_graalpy() -> Result<()> {
        let mut context = TestContext::new()?;

        context.add_python_interpreters(&[(
            true,
            ImplementationName::GraalPy,
            "graalpy",
            "3.10.0",
        )])?;
        let result = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })?;
        assert!(
            matches!(result, Err(PythonNotFound { .. })),
            "We should not the graalpy interpreter if not named `python` or requested; got {result:?}"
        );

        // But we should find it
        context.reset_search_path();
        context.add_python_interpreters(&[(
            true,
            ImplementationName::GraalPy,
            "python",
            "3.10.1",
        )])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should find the graalpy interpreter if it's the only one"
        );

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("graalpy"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should find the graalpy interpreter if it's requested"
        );

        Ok(())
    }

    #[test]
    fn find_python_graalpy_request_ignores_cpython() -> Result<()> {
        let mut context = TestContext::new()?;
        context.add_python_interpreters(&[
            (true, ImplementationName::CPython, "python", "3.10.0"),
            (true, ImplementationName::GraalPy, "graalpy", "3.10.1"),
        ])?;

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("graalpy"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
            "We should skip the CPython interpreter"
        );

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::Any,
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.0",
            "We should take the first interpreter without a specific request"
        );

        Ok(())
    }

    #[test]
    fn find_python_graalpy_prefers_executable_with_implementation_name() -> Result<()> {
        let mut context = TestContext::new()?;

        // We should prefer `graalpy` executables over `python` executables in the same directory
        // even if they are both graalpy
        TestContext::create_mock_interpreter(
            &context.tempdir.join("python"),
            &PythonVersion::from_str("3.10.0").unwrap(),
            ImplementationName::GraalPy,
            true,
        )?;
        TestContext::create_mock_interpreter(
            &context.tempdir.join("graalpy"),
            &PythonVersion::from_str("3.10.1").unwrap(),
            ImplementationName::GraalPy,
            true,
        )?;
        context.add_to_search_path(context.tempdir.to_path_buf());

        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("graalpy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.1",
        );

        // But `python` executables earlier in the search path will take precedence
        context.reset_search_path();
        context.add_python_interpreters(&[
            (true, ImplementationName::GraalPy, "python", "3.10.2"),
            (true, ImplementationName::GraalPy, "graalpy", "3.10.3"),
        ])?;
        let python = context.run(|| {
            find_python_installation(
                &PythonRequest::parse("graalpy@3.10"),
                EnvironmentPreference::Any,
                PythonPreference::OnlySystem,
                &context.cache,
            )
        })??;
        assert_eq!(
            python.interpreter().python_full_version().to_string(),
            "3.10.2",
        );

        Ok(())
    }
}
