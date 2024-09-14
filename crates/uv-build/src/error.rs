use crate::PythonRunnerOutput;
use itertools::Itertools;
use pep440_rs::Version;
use pep508_rs::PackageName;
use regex::Regex;
use std::env;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::LazyLock;
use thiserror::Error;
use tracing::error;
use uv_configuration::BuildOutput;
use uv_fs::Simplified;

/// e.g. `pygraphviz/graphviz_wrap.c:3020:10: fatal error: graphviz/cgraph.h: No such file or directory`
static MISSING_HEADER_RE_GCC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r".*\.(?:c|c..|h|h..):\d+:\d+: fatal error: (.*\.(?:h|h..)): No such file or directory",
    )
    .unwrap()
});

/// e.g. `pygraphviz/graphviz_wrap.c:3023:10: fatal error: 'graphviz/cgraph.h' file not found`
static MISSING_HEADER_RE_CLANG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r".*\.(?:c|c..|h|h..):\d+:\d+: fatal error: '(.*\.(?:h|h..))' file not found")
        .unwrap()
});

/// e.g. `pygraphviz/graphviz_wrap.c(3023): fatal error C1083: Cannot open include file: 'graphviz/cgraph.h': No such file or directory`
static MISSING_HEADER_RE_MSVC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r".*\.(?:c|c..|h|h..)\(\d+\): fatal error C1083: Cannot open include file: '(.*\.(?:h|h..))': No such file or directory")
        .unwrap()
});

/// e.g. `/usr/bin/ld: cannot find -lncurses: No such file or directory`
static LD_NOT_FOUND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/usr/bin/ld: cannot find -l([a-zA-Z10-9]+): No such file or directory").unwrap()
});

/// e.g. `error: invalid command 'bdist_wheel'`
static WHEEL_NOT_FOUND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"error: invalid command 'bdist_wheel'").unwrap());

/// e.g. `ModuleNotFoundError: No module named 'torch'`
static TORCH_NOT_FOUND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ModuleNotFoundError: No module named 'torch'").unwrap());

/// e.g. `ModuleNotFoundError: No module named 'distutils'`
static DISTUTILS_NOT_FOUND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ModuleNotFoundError: No module named 'distutils'").unwrap());

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{} does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory", _0.simplified_display())]
    InvalidSourceDist(PathBuf),
    #[error("Invalid `pyproject.toml`")]
    InvalidPyprojectTomlSyntax(#[from] toml_edit::TomlError),
    #[error("`pyproject.toml` does not match the required schema. When the `[project]` table is present, `project.name` must be present and non-empty.")]
    InvalidPyprojectTomlSchema(#[from] toml_edit::de::Error),
    #[error("Editable installs with setup.py legacy builds are unsupported, please specify a build backend in pyproject.toml")]
    EditableSetupPy,
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] anyhow::Error),
    #[error("Failed to create temporary virtualenv")]
    Virtualenv(#[from] uv_virtualenv::Error),
    #[error("Failed to run `{0}`")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    BuildBackendOutput {
        message: String,
        exit_code: ExitStatus,
        stdout: String,
        stderr: String,
    },
    /// Nudge the user towards installing the missing dev library
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    MissingHeaderOutput {
        message: String,
        exit_code: ExitStatus,
        stdout: String,
        stderr: String,
        #[source]
        missing_header_cause: MissingHeaderCause,
    },
    #[error("{message} with {exit_code}")]
    BuildBackend {
        message: String,
        exit_code: ExitStatus,
    },
    #[error("{message} with {exit_code}")]
    MissingHeader {
        message: String,
        exit_code: ExitStatus,
        #[source]
        missing_header_cause: MissingHeaderCause,
    },
    #[error("Failed to build PATH for build script")]
    BuildScriptPath(#[source] env::JoinPathsError),
}

#[derive(Debug)]
enum MissingLibrary {
    Header(String),
    Linker(String),
    BuildDependency(String),
    DeprecatedModule(String, Version),
}

#[derive(Debug, Error)]
pub struct MissingHeaderCause {
    missing_library: MissingLibrary,
    package_name: Option<PackageName>,
    package_version: Option<Version>,
    version_id: Option<String>,
}

impl Display for MissingHeaderCause {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.missing_library {
            MissingLibrary::Header(header) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "This error likely indicates that you need to install a library that provides \"{header}\" for {package_name}@{package_version}",
                    )
                } else if let Some(version_id) = &self.version_id {
                    write!(
                        f,
                        "This error likely indicates that you need to install a library that provides \"{header}\" for {version_id}",
                    )
                } else {
                    write!(
                        f,
                        "This error likely indicates that you need to install a library that provides \"{header}\"",
                    )
                }
            }
            MissingLibrary::Linker(library) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for {library} for {package_name}@{package_version} (e.g., lib{library}-dev)",
                    )
                } else if let Some(version_id) = &self.version_id {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for {library} for {version_id} (e.g., lib{library}-dev)",
                    )
                } else {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for {library} (e.g., lib{library}-dev)",
                    )
                }
            }
            MissingLibrary::BuildDependency(package) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "This error likely indicates that {package_name}@{package_version} depends on {package}, but doesn't declare it as a build dependency. If {package_name} is a first-party package, consider adding {package} to its `build-system.requires`. Otherwise, `uv pip install {package}` into the environment and re-run with `--no-build-isolation`."
                    )
                } else if let Some(version_id) = &self.version_id {
                    write!(
                        f,
                        "This error likely indicates that {version_id} depends on {package}, but doesn't declare it as a build dependency. If {version_id} is a first-party package, consider adding {package} to its `build-system.requires`. Otherwise, `uv pip install {package}` into the environment and re-run with `--no-build-isolation`.",
                    )
                } else {
                    write!(
                        f,
                        "This error likely indicates that a package depends on {package}, but doesn't declare it as a build dependency. If the package is a first-party package, consider adding {package} to its `build-system.requires`. Otherwise, `uv pip install {package}` into the environment and re-run with `--no-build-isolation`.",
                    )
                }
            }
            MissingLibrary::DeprecatedModule(package, version) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "{package} was removed from the standard library in Python {version}. Consider adding a constraint (like `{package_name} >{package_version}`) to avoid building a version of {package_name} that depends on {package}.",
                    )
                } else {
                    write!(
                        f,
                        "{package} was removed from the standard library in Python {version}. Consider adding a constraint to avoid building a package-version that depends on {package}.",
                    )
                }
            }
        }
    }
}

impl Error {
    /// Construct an [`Error`] from the output of a failed command.
    pub(crate) fn from_command_output(
        message: String,
        output: &PythonRunnerOutput,
        level: BuildOutput,
        name: Option<&PackageName>,
        version: Option<&Version>,
        version_id: Option<&str>,
    ) -> Self {
        // In the cases I've seen it was the 5th and 3rd last line (see test case), 10 seems like a reasonable cutoff.
        let missing_library = output.stderr.iter().rev().take(10).find_map(|line| {
            if let Some((_, [header])) = MISSING_HEADER_RE_GCC
                .captures(line.trim())
                .or(MISSING_HEADER_RE_CLANG.captures(line.trim()))
                .or(MISSING_HEADER_RE_MSVC.captures(line.trim()))
                .map(|c| c.extract())
            {
                Some(MissingLibrary::Header(header.to_string()))
            } else if let Some((_, [library])) =
                LD_NOT_FOUND_RE.captures(line.trim()).map(|c| c.extract())
            {
                Some(MissingLibrary::Linker(library.to_string()))
            } else if WHEEL_NOT_FOUND_RE.is_match(line.trim()) {
                Some(MissingLibrary::BuildDependency("wheel".to_string()))
            } else if TORCH_NOT_FOUND_RE.is_match(line.trim()) {
                Some(MissingLibrary::BuildDependency("torch".to_string()))
            } else if DISTUTILS_NOT_FOUND_RE.is_match(line.trim()) {
                Some(MissingLibrary::DeprecatedModule(
                    "distutils".to_string(),
                    Version::new([3, 12]),
                ))
            } else {
                None
            }
        });

        if let Some(missing_library) = missing_library {
            return match level {
                BuildOutput::Stderr => Self::MissingHeader {
                    message,
                    exit_code: output.status,
                    missing_header_cause: MissingHeaderCause {
                        missing_library,
                        package_name: name.cloned(),
                        package_version: version.cloned(),
                        version_id: version_id.map(ToString::to_string),
                    },
                },
                BuildOutput::Debug => Self::MissingHeaderOutput {
                    message,
                    exit_code: output.status,
                    stdout: output.stdout.iter().join("\n"),
                    stderr: output.stderr.iter().join("\n"),
                    missing_header_cause: MissingHeaderCause {
                        missing_library,
                        package_name: name.cloned(),
                        package_version: version.cloned(),
                        version_id: version_id.map(ToString::to_string),
                    },
                },
            };
        }

        match level {
            BuildOutput::Stderr => Self::BuildBackend {
                message,
                exit_code: output.status,
            },
            BuildOutput::Debug => Self::BuildBackendOutput {
                message,
                exit_code: output.status,
                stdout: output.stdout.iter().join("\n"),
                stderr: output.stderr.iter().join("\n"),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{Error, PythonRunnerOutput};
    use indoc::indoc;
    use pep440_rs::Version;
    use pep508_rs::PackageName;
    use std::process::ExitStatus;
    use std::str::FromStr;
    use uv_configuration::BuildOutput;

    #[test]
    fn missing_header() {
        let output = PythonRunnerOutput {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: indoc!(r"
                running bdist_wheel
                running build
                [...]
                creating build/temp.linux-x86_64-cpython-39/pygraphviz
                gcc -Wno-unused-result -Wsign-compare -DNDEBUG -g -fwrapv -O3 -Wall -DOPENSSL_NO_SSL3 -fPIC -DSWIG_PYTHON_STRICT_BYTE_CHAR -I/tmp/.tmpy6vVes/.venv/include -I/home/konsti/.pyenv/versions/3.9.18/include/python3.9 -c pygraphviz/graphviz_wrap.c -o build/temp.linux-x86_64-cpython-39/pygraphviz/graphviz_wrap.o
                "
            ).lines().map(ToString::to_string).collect(),
            stderr: indoc!(r#"
                warning: no files found matching '*.png' under directory 'doc'
                warning: no files found matching '*.txt' under directory 'doc'
                [...]
                no previously-included directories found matching 'doc/build'
                pygraphviz/graphviz_wrap.c:3020:10: fatal error: graphviz/cgraph.h: No such file or directory
                 3020 | #include "graphviz/cgraph.h"
                      |          ^~~~~~~~~~~~~~~~~~~
                compilation terminated.
                error: command '/usr/bin/gcc' failed with exit code 1
                "#
            ).lines().map(ToString::to_string).collect(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            BuildOutput::Debug,
            None,
            None,
            Some("pygraphviz-1.11"),
        );
        assert!(matches!(err, Error::MissingHeaderOutput { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = err.to_string().replace("exit status: ", "exit code: ");
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py with exit code: 0
        --- stdout:
        running bdist_wheel
        running build
        [...]
        creating build/temp.linux-x86_64-cpython-39/pygraphviz
        gcc -Wno-unused-result -Wsign-compare -DNDEBUG -g -fwrapv -O3 -Wall -DOPENSSL_NO_SSL3 -fPIC -DSWIG_PYTHON_STRICT_BYTE_CHAR -I/tmp/.tmpy6vVes/.venv/include -I/home/konsti/.pyenv/versions/3.9.18/include/python3.9 -c pygraphviz/graphviz_wrap.c -o build/temp.linux-x86_64-cpython-39/pygraphviz/graphviz_wrap.o
        --- stderr:
        warning: no files found matching '*.png' under directory 'doc'
        warning: no files found matching '*.txt' under directory 'doc'
        [...]
        no previously-included directories found matching 'doc/build'
        pygraphviz/graphviz_wrap.c:3020:10: fatal error: graphviz/cgraph.h: No such file or directory
         3020 | #include "graphviz/cgraph.h"
              |          ^~~~~~~~~~~~~~~~~~~
        compilation terminated.
        error: command '/usr/bin/gcc' failed with exit code 1
        ---
        "###);
        insta::assert_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @r###"This error likely indicates that you need to install a library that provides "graphviz/cgraph.h" for pygraphviz-1.11"###
        );
    }

    #[test]
    fn missing_linker_library() {
        let output = PythonRunnerOutput {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: Vec::new(),
            stderr: indoc!(
                r"
               1099 |     n = strlen(p);
                    |         ^~~~~~~~~
               /usr/bin/ld: cannot find -lncurses: No such file or directory
               collect2: error: ld returned 1 exit status
               error: command '/usr/bin/x86_64-linux-gnu-gcc' failed with exit code 1"
            )
            .lines()
            .map(ToString::to_string)
            .collect(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            BuildOutput::Debug,
            None,
            None,
            Some("pygraphviz-1.11"),
        );
        assert!(matches!(err, Error::MissingHeaderOutput { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = err.to_string().replace("exit status: ", "exit code: ");
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py with exit code: 0
        --- stdout:

        --- stderr:
        1099 |     n = strlen(p);
             |         ^~~~~~~~~
        /usr/bin/ld: cannot find -lncurses: No such file or directory
        collect2: error: ld returned 1 exit status
        error: command '/usr/bin/x86_64-linux-gnu-gcc' failed with exit code 1
        ---
        "###);
        insta::assert_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @"This error likely indicates that you need to install the library that provides a shared library for ncurses for pygraphviz-1.11 (e.g., libncurses-dev)"
        );
    }

    #[test]
    fn missing_wheel_package() {
        let output = PythonRunnerOutput {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: Vec::new(),
            stderr: indoc!(
                r"
            usage: setup.py [global_opts] cmd1 [cmd1_opts] [cmd2 [cmd2_opts] ...]
               or: setup.py --help [cmd1 cmd2 ...]
               or: setup.py --help-commands
               or: setup.py cmd --help

            error: invalid command 'bdist_wheel'"
            )
            .lines()
            .map(ToString::to_string)
            .collect(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            BuildOutput::Debug,
            None,
            None,
            Some("pygraphviz-1.11"),
        );
        assert!(matches!(err, Error::MissingHeaderOutput { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = err.to_string().replace("exit status: ", "exit code: ");
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py with exit code: 0
        --- stdout:

        --- stderr:
        usage: setup.py [global_opts] cmd1 [cmd1_opts] [cmd2 [cmd2_opts] ...]
           or: setup.py --help [cmd1 cmd2 ...]
           or: setup.py --help-commands
           or: setup.py cmd --help

        error: invalid command 'bdist_wheel'
        ---
        "###);
        insta::assert_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @"This error likely indicates that pygraphviz-1.11 depends on wheel, but doesn't declare it as a build dependency. If pygraphviz-1.11 is a first-party package, consider adding wheel to its `build-system.requires`. Otherwise, `uv pip install wheel` into the environment and re-run with `--no-build-isolation`."
        );
    }

    #[test]
    fn missing_distutils() {
        let output = PythonRunnerOutput {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: Vec::new(),
            stderr: indoc!(
                r"
                import distutils.core
                ModuleNotFoundError: No module named 'distutils'
                "
            )
            .lines()
            .map(ToString::to_string)
            .collect(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            BuildOutput::Debug,
            Some(&PackageName::from_str("pygraphviz").unwrap()),
            Some(&Version::new([1, 11])),
            Some("pygraphviz-1.11"),
        );
        assert!(matches!(err, Error::MissingHeaderOutput { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = err.to_string().replace("exit status: ", "exit code: ");
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py with exit code: 0
        --- stdout:

        --- stderr:
        import distutils.core
        ModuleNotFoundError: No module named 'distutils'
        ---
        "###);
        insta::assert_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @"distutils was removed from the standard library in Python 3.12. Consider adding a constraint (like `pygraphviz >1.11`) to avoid building a version of pygraphviz that depends on distutils."
        );
    }
}
