use std::env;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::LazyLock;

use crate::PythonRunnerOutput;
use owo_colors::OwoColorize;
use regex::Regex;
use thiserror::Error;
use uv_configuration::BuildOutput;
use uv_distribution_types::IsBuildBackendError;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_types::AnyErrorBuild;

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

/// e.g. `ModuleNotFoundError`
static MODULE_NOT_FOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("ModuleNotFoundError: No module named ['\"]([^'\"]+)['\"]").unwrap()
});

/// e.g. `ModuleNotFoundError: No module named 'distutils'`
static DISTUTILS_NOT_FOUND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ModuleNotFoundError: No module named 'distutils'").unwrap());

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Lowering(#[from] uv_distribution::MetadataError),
    #[error("{} does not appear to be a Python project, as neither `pyproject.toml` nor `setup.py` are present in the directory", _0.simplified_display())]
    InvalidSourceDist(PathBuf),
    #[error("Invalid `pyproject.toml`")]
    InvalidPyprojectTomlSyntax(#[from] toml_edit::TomlError),
    #[error(
        "`pyproject.toml` does not match the required schema. When the `[project]` table is present, `project.name` must be present and non-empty."
    )]
    InvalidPyprojectTomlSchema(#[from] toml_edit::de::Error),
    #[error("Failed to resolve requirements from {0}")]
    RequirementsResolve(&'static str, #[source] AnyErrorBuild),
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] AnyErrorBuild),
    #[error("Failed to create temporary virtualenv")]
    Virtualenv(#[from] uv_virtualenv::Error),
    // Build backend errors
    #[error("Failed to run `{0}`")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("The build backend returned an error")]
    BuildBackend(#[from] BuildBackendError),
    #[error("The build backend returned an error")]
    MissingHeader(#[from] MissingHeaderError),
    #[error("Failed to build PATH for build script")]
    BuildScriptPath(#[source] env::JoinPathsError),
    // For the convenience of typing `setup_build` properly.
    #[error("Building source distributions for `{0}` is disabled")]
    NoSourceDistBuild(PackageName),
    #[error("Building source distributions is disabled")]
    NoSourceDistBuilds,
    #[error("Cyclic build dependency detected for `{0}`")]
    CyclicBuildDependency(PackageName),
    #[error(
        "Extra build requirement `{0}` was declared with `match-runtime = true`, but `{1}` does not declare static metadata, making runtime-matching impossible"
    )]
    UnmatchedRuntime(PackageName, PackageName),
}

impl IsBuildBackendError for Error {
    fn is_build_backend_error(&self) -> bool {
        match self {
            Self::Io(_)
            | Self::Lowering(_)
            | Self::InvalidSourceDist(_)
            | Self::InvalidPyprojectTomlSyntax(_)
            | Self::InvalidPyprojectTomlSchema(_)
            | Self::RequirementsResolve(_, _)
            | Self::RequirementsInstall(_, _)
            | Self::Virtualenv(_)
            | Self::NoSourceDistBuild(_)
            | Self::NoSourceDistBuilds
            | Self::CyclicBuildDependency(_)
            | Self::UnmatchedRuntime(_, _) => false,
            Self::CommandFailed(_, _)
            | Self::BuildBackend(_)
            | Self::MissingHeader(_)
            | Self::BuildScriptPath(_) => true,
        }
    }
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

/// Extract the package name from a version specifier string.
/// Uses PEP 508 naming rules but more lenient for hinting purposes.
fn extract_package_name(version_id: &str) -> &str {
    // https://peps.python.org/pep-0508/#names
    // ^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$ with re.IGNORECASE
    // Since we're only using this for a hint, we're more lenient than what we would be doing if this was used for parsing
    let end = version_id
        .char_indices()
        .take_while(|(_, char)| matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_'))
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());

    if end == 0 {
        version_id
    } else {
        &version_id[..end]
    }
}

/// Write a hint about missing build dependencies.
fn hint_build_dependency(
    f: &mut std::fmt::Formatter<'_>,
    display_name: &str,
    package_name: &str,
    package: &str,
) -> std::fmt::Result {
    let table_key = if package_name.contains('.') {
        format!("\"{package_name}\"")
    } else {
        package_name.to_string()
    };
    write!(
        f,
        "This error likely indicates that `{}` depends on `{}`, but doesn't declare it as a build dependency. \
        If `{}` is a first-party package, consider adding `{}` to its `{}`. \
        Otherwise, either add it to your `pyproject.toml` under:\n\
        \n\
            [tool.uv.extra-build-dependencies]\n\
            {} = [\"{}\"]\n\
        \n\
        or `{}` into the environment and re-run with `{}`.",
        display_name.cyan(),
        package.cyan(),
        package_name.cyan(),
        package.cyan(),
        "build-system.requires".green(),
        table_key.cyan(),
        package.cyan(),
        format!("uv pip install {package}").green(),
        "--no-build-isolation".green(),
    )
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
                        "This error likely indicates that you need to install a library that provides \"{}\" for `{}`",
                        header.cyan(),
                        format!("{package_name}@{package_version}").cyan(),
                    )
                } else if let Some(version_id) = &self.version_id {
                    write!(
                        f,
                        "This error likely indicates that you need to install a library that provides \"{}\" for `{}`",
                        header.cyan(),
                        version_id.cyan(),
                    )
                } else {
                    write!(
                        f,
                        "This error likely indicates that you need to install a library that provides \"{}\"",
                        header.cyan(),
                    )
                }
            }
            MissingLibrary::Linker(library) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for `{}` for `{}` (e.g., `{}`)",
                        library.cyan(),
                        format!("{package_name}@{package_version}").cyan(),
                        format!("lib{library}-dev").cyan(),
                    )
                } else if let Some(version_id) = &self.version_id {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for `{}` for `{}` (e.g., `{}`)",
                        library.cyan(),
                        version_id.cyan(),
                        format!("lib{library}-dev").cyan(),
                    )
                } else {
                    write!(
                        f,
                        "This error likely indicates that you need to install the library that provides a shared library for `{}` (e.g., `{}`)",
                        library.cyan(),
                        format!("lib{library}-dev").cyan(),
                    )
                }
            }
            MissingLibrary::BuildDependency(package) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    hint_build_dependency(
                        f,
                        &format!("{package_name}@{package_version}"),
                        package_name.as_str(),
                        package,
                    )
                } else if let Some(version_id) = &self.version_id {
                    let package_name = extract_package_name(version_id);
                    hint_build_dependency(f, package_name, package_name, package)
                } else {
                    write!(
                        f,
                        "This error likely indicates that a package depends on `{}`, but doesn't declare it as a build dependency. If the package is a first-party package, consider adding `{}` to its `{}`. Otherwise, `{}` into the environment and re-run with `{}`.",
                        package.cyan(),
                        package.cyan(),
                        "build-system.requires".green(),
                        format!("uv pip install {package}").green(),
                        "--no-build-isolation".green(),
                    )
                }
            }
            MissingLibrary::DeprecatedModule(package, version) => {
                if let (Some(package_name), Some(package_version)) =
                    (&self.package_name, &self.package_version)
                {
                    write!(
                        f,
                        "`{}` was removed from the standard library in Python {version}. Consider adding a constraint (like `{}`) to avoid building a version of `{}` that depends on `{}`.",
                        package.cyan(),
                        format!("{package_name} >{package_version}").green(),
                        package_name.cyan(),
                        package.cyan(),
                    )
                } else {
                    write!(
                        f,
                        "`{}` was removed from the standard library in Python {version}. Consider adding a constraint to avoid building a package that depends on `{}`.",
                        package.cyan(),
                        package.cyan(),
                    )
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub struct BuildBackendError {
    message: String,
    exit_code: ExitStatus,
    stdout: Vec<String>,
    stderr: Vec<String>,
}

impl Display for BuildBackendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.exit_code)?;

        let mut non_empty = false;

        if self.stdout.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout.join("\n"))?;
            non_empty = true;
        }

        if self.stderr.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr.join("\n"))?;
            non_empty = true;
        }

        if non_empty {
            writeln!(f)?;
        }

        write!(
            f,
            "\n{}{} This usually indicates a problem with the package or the build environment.",
            "hint".bold().cyan(),
            ":".bold()
        )?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub struct MissingHeaderError {
    message: String,
    exit_code: ExitStatus,
    stdout: Vec<String>,
    stderr: Vec<String>,
    cause: MissingHeaderCause,
}

impl Display for MissingHeaderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.exit_code)?;

        if self.stdout.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout.join("\n"))?;
        }

        if self.stderr.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr.join("\n"))?;
        }

        write!(
            f,
            "\n\n{}{} {}",
            "hint".bold().cyan(),
            ":".bold(),
            self.cause
        )?;

        Ok(())
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
            } else if DISTUTILS_NOT_FOUND_RE.is_match(line.trim()) {
                Some(MissingLibrary::DeprecatedModule(
                    "distutils".to_string(),
                    Version::new([3, 12]),
                ))
            } else if let Some(caps) = MODULE_NOT_FOUND.captures(line.trim()) {
                if let Some(module_match) = caps.get(1) {
                    let module_name = module_match.as_str();
                    let package_name = match crate::pipreqs::MODULE_MAPPING.lookup(module_name) {
                        Some(package) => package.to_string(),
                        None => module_name.to_string(),
                    };
                    Some(MissingLibrary::BuildDependency(package_name))
                } else {
                    None
                }
            } else {
                None
            }
        });

        if let Some(missing_library) = missing_library {
            return match level {
                BuildOutput::Stderr | BuildOutput::Quiet => {
                    Self::MissingHeader(MissingHeaderError {
                        message,
                        exit_code: output.status,
                        stdout: vec![],
                        stderr: vec![],
                        cause: MissingHeaderCause {
                            missing_library,
                            package_name: name.cloned(),
                            package_version: version.cloned(),
                            version_id: version_id.map(ToString::to_string),
                        },
                    })
                }
                BuildOutput::Debug => Self::MissingHeader(MissingHeaderError {
                    message,
                    exit_code: output.status,
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    cause: MissingHeaderCause {
                        missing_library,
                        package_name: name.cloned(),
                        package_version: version.cloned(),
                        version_id: version_id.map(ToString::to_string),
                    },
                }),
            };
        }

        match level {
            BuildOutput::Stderr | BuildOutput::Quiet => Self::BuildBackend(BuildBackendError {
                message,
                exit_code: output.status,
                stdout: vec![],
                stderr: vec![],
            }),
            BuildOutput::Debug => Self::BuildBackend(BuildBackendError {
                message,
                exit_code: output.status,
                stdout: output.stdout.clone(),
                stderr: output.stderr.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{Error, PythonRunnerOutput};
    use indoc::indoc;
    use std::process::ExitStatus;
    use std::str::FromStr;
    use uv_configuration::BuildOutput;
    use uv_normalize::PackageName;
    use uv_pep440::Version;

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

        assert!(matches!(err, Error::MissingHeader { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = std::error::Error::source(&err)
            .unwrap()
            .to_string()
            .replace("exit status: ", "exit code: ");
        let formatted = anstream::adapter::strip_str(&formatted);
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py (exit code: 0)

        [stdout]
        running bdist_wheel
        running build
        [...]
        creating build/temp.linux-x86_64-cpython-39/pygraphviz
        gcc -Wno-unused-result -Wsign-compare -DNDEBUG -g -fwrapv -O3 -Wall -DOPENSSL_NO_SSL3 -fPIC -DSWIG_PYTHON_STRICT_BYTE_CHAR -I/tmp/.tmpy6vVes/.venv/include -I/home/konsti/.pyenv/versions/3.9.18/include/python3.9 -c pygraphviz/graphviz_wrap.c -o build/temp.linux-x86_64-cpython-39/pygraphviz/graphviz_wrap.o

        [stderr]
        warning: no files found matching '*.png' under directory 'doc'
        warning: no files found matching '*.txt' under directory 'doc'
        [...]
        no previously-included directories found matching 'doc/build'
        pygraphviz/graphviz_wrap.c:3020:10: fatal error: graphviz/cgraph.h: No such file or directory
         3020 | #include "graphviz/cgraph.h"
              |          ^~~~~~~~~~~~~~~~~~~
        compilation terminated.
        error: command '/usr/bin/gcc' failed with exit code 1

        hint: This error likely indicates that you need to install a library that provides "graphviz/cgraph.h" for `pygraphviz-1.11`
        "###);
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
        assert!(matches!(err, Error::MissingHeader { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = std::error::Error::source(&err)
            .unwrap()
            .to_string()
            .replace("exit status: ", "exit code: ");
        let formatted = anstream::adapter::strip_str(&formatted);
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py (exit code: 0)

        [stderr]
        1099 |     n = strlen(p);
             |         ^~~~~~~~~
        /usr/bin/ld: cannot find -lncurses: No such file or directory
        collect2: error: ld returned 1 exit status
        error: command '/usr/bin/x86_64-linux-gnu-gcc' failed with exit code 1

        hint: This error likely indicates that you need to install the library that provides a shared library for `ncurses` for `pygraphviz-1.11` (e.g., `libncurses-dev`)
        "###);
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
        assert!(matches!(err, Error::MissingHeader { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = std::error::Error::source(&err)
            .unwrap()
            .to_string()
            .replace("exit status: ", "exit code: ");
        let formatted = anstream::adapter::strip_str(&formatted);
        insta::assert_snapshot!(formatted, @r#"
        Failed building wheel through setup.py (exit code: 0)

        [stderr]
        usage: setup.py [global_opts] cmd1 [cmd1_opts] [cmd2 [cmd2_opts] ...]
           or: setup.py --help [cmd1 cmd2 ...]
           or: setup.py --help-commands
           or: setup.py cmd --help

        error: invalid command 'bdist_wheel'

        hint: This error likely indicates that `pygraphviz-1.11` depends on `wheel`, but doesn't declare it as a build dependency. If `pygraphviz-1.11` is a first-party package, consider adding `wheel` to its `build-system.requires`. Otherwise, either add it to your `pyproject.toml` under:

        [tool.uv.extra-build-dependencies]
        "pygraphviz-1.11" = ["wheel"]

        or `uv pip install wheel` into the environment and re-run with `--no-build-isolation`.
        "#);
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
        assert!(matches!(err, Error::MissingHeader { .. }));
        // Unix uses exit status, Windows uses exit code.
        let formatted = std::error::Error::source(&err)
            .unwrap()
            .to_string()
            .replace("exit status: ", "exit code: ");
        let formatted = anstream::adapter::strip_str(&formatted);
        insta::assert_snapshot!(formatted, @r###"
        Failed building wheel through setup.py (exit code: 0)

        [stderr]
        import distutils.core
        ModuleNotFoundError: No module named 'distutils'

        hint: `distutils` was removed from the standard library in Python 3.12. Consider adding a constraint (like `pygraphviz >1.11`) to avoid building a version of `pygraphviz` that depends on `distutils`.
        "###);
    }
}
