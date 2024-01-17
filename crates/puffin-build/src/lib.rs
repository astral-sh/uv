//! Build wheels from source distributions
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::str::FromStr;
use std::sync::Arc;

use fs_err as fs;
use once_cell::sync::Lazy;
use pyproject_toml::{BuildSystem, Project};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tempfile::{tempdir, tempdir_in, TempDir};
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info_span, instrument, Instrument};

use crate::daemon::Pep517Daemon;
use distribution_types::Resolution;
use pep508_rs::Requirement;
use puffin_extract::extract_source;
use puffin_interpreter::{Interpreter, Virtualenv};
use puffin_traits::{BuildContext, BuildKind, SetupPyStrategy, SourceBuildTrait};

mod daemon;

/// e.g. `pygraphviz/graphviz_wrap.c:3020:10: fatal error: graphviz/cgraph.h: No such file or directory`
static MISSING_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r".*\.(?:c|c..|h|h..):\d+:\d+: fatal error: (.*\.(?:h|h..)): No such file or directory",
    )
    .unwrap()
});

/// e.g. `/usr/bin/ld: cannot find -lncurses: No such file or directory`
static LD_NOT_FOUND_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"/usr/bin/ld: cannot find -l([a-zA-Z10-9]+): No such file or directory").unwrap()
});

/// The default backend to use when PEP 517 is used without a `build-system` section.
static DEFAULT_BACKEND: Lazy<Pep517Backend> = Lazy::new(|| Pep517Backend {
    backend: "setuptools.build_meta:__legacy__".to_string(),
    backend_path: None,
    requirements: vec![
        Requirement::from_str("wheel").unwrap(),
        Requirement::from_str("setuptools >= 40.8.0").unwrap(),
    ],
});

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to extract archive: {0}")]
    Extraction(PathBuf, #[source] puffin_extract::Error),
    #[error("Unsupported archive format (extension not recognized): {0}")]
    UnsupportedArchiveType(String),
    #[error("Invalid source distribution: {0}")]
    InvalidSourceDist(String),
    #[error("Invalid pyproject.toml")]
    InvalidPyprojectToml(#[from] toml::de::Error),
    #[error("Editable installs with setup.py legacy builds are unsupported, please specify a build backend in pyproject.toml")]
    EditableSetupPy,
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] anyhow::Error),
    #[error("Failed to create temporary virtual environment")]
    Gourgeist(#[from] gourgeist::Error),
    #[error("Failed to run {0}")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("{message}")]
    DaemonError {
        // TODO: Do not expose this
        message: String,
    },
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    BuildBackend {
        message: String,
        stdout: String,
        stderr: String,
    },
    /// Nudge the user towards installing the missing dev library
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    MissingHeader {
        message: String,
        stdout: String,
        stderr: String,
        #[source]
        missing_header_cause: MissingHeaderCause,
    },
}

#[derive(Debug)]
pub enum MissingLibrary {
    Header(String),
    Linker(String),
}

#[derive(Debug, Error)]
pub struct MissingHeaderCause {
    missing_library: MissingLibrary,
    package_id: String,
}

impl Display for MissingHeaderCause {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.missing_library {
            MissingLibrary::Header(header) => {
                write!(
                    f,
                    "This error likely indicates that you need to install a library that provides \"{}\" for {}",
                    header, self.package_id
                )
            }
            MissingLibrary::Linker(library) => {
                write!(
                    f,
                    "This error likely indicates that you need to install the library that provides a shared library \
                    for {library} for {package_id} (e.g. lib{library}-dev)",
                    library = library, package_id = self.package_id
                )
            }
        }
    }
}

impl Error {
    fn from_command_output(
        message: String,
        output: &Output,
        package_id: impl Into<String>,
    ) -> Self {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        // In the cases i've seen it was the 5th and 3rd last line (see test case), 10 seems like a reasonable cutoff
        let missing_library = stderr.lines().rev().take(10).find_map(|line| {
            if let Some((_, [header])) =
                MISSING_HEADER_RE.captures(line.trim()).map(|c| c.extract())
            {
                Some(MissingLibrary::Header(header.to_string()))
            } else if let Some((_, [library])) =
                LD_NOT_FOUND_RE.captures(line.trim()).map(|c| c.extract())
            {
                Some(MissingLibrary::Linker(library.to_string()))
            } else {
                None
            }
        });

        if let Some(missing_library) = missing_library {
            return Self::MissingHeader {
                message,
                stdout,
                stderr,
                missing_header_cause: MissingHeaderCause {
                    missing_library,
                    package_id: package_id.into(),
                },
            };
        }

        Self::BuildBackend {
            message,
            stdout,
            stderr,
        }
    }
}

/// A pyproject.toml as specified in PEP 517
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    /// Build-related data
    pub build_system: Option<BuildSystem>,
    /// Project metadata
    pub project: Option<Project>,
}

/// `[build-backend]` from pyproject.toml
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pep517Backend {
    /// The build backend string such as `setuptools.build_meta:__legacy__` or `maturin` from
    /// `build-backend.backend` in pyproject.toml
    ///
    /// <https://peps.python.org/pep-0517/#build-wheel>
    backend: String,
    /// `build-backend.requirements` in pyproject.toml
    requirements: Vec<Requirement>,
    /// <https://peps.python.org/pep-0517/#in-tree-build-backends>
    backend_path: Option<Vec<String>>,
}

/// Uses an [`Arc`] internally, clone freely
#[derive(Debug, Default, Clone)]
pub struct SourceBuildContext {
    /// Cache the first resolution of `pip`, `setuptools` and `wheel` we made for setup.py (and
    /// some PEP 517) builds so we can reuse it.
    setup_py_resolution: Arc<Mutex<Option<Resolution>>>,
}

/// Holds the state through a series of PEP 517 frontend to backend calls or a single setup.py
/// invocation.
///
/// This keeps both the temp dir and the result of a potential `prepare_metadata_for_build_wheel`
/// call which changes how we call `build_wheel`.
pub struct SourceBuild {
    temp_dir: TempDir,
    source_tree: PathBuf,
    /// If performing a PEP 517 build, the backend to use.
    pep517_backend: Option<Pep517Backend>,
    /// If performing a PEP 517 build, a stateful daemon to use for PEP 517 calls.
    pep517_daemon: Pep517Daemon,
    /// The virtual environment in which to build the source distribution.
    venv: Virtualenv,
    /// Populated if `prepare_metadata_for_build_wheel` was called.
    ///
    /// > If the build frontend has previously called prepare_metadata_for_build_wheel and depends
    /// > on the wheel resulting from this call to have metadata matching this earlier call, then
    /// > it should provide the path to the created .dist-info directory as the metadata_directory
    /// > argument. If this argument is provided, then build_wheel MUST produce a wheel with
    /// > identical metadata. The directory passed in by the build frontend MUST be identical to the
    /// > directory created by prepare_metadata_for_build_wheel, including any unrecognized files
    /// > it created.
    metadata_directory: Option<PathBuf>,
    /// Package id such as `foo-1.2.3`, for error reporting
    package_id: String,
    /// Whether we do a regular PEP 517 build or an PEP 660 editable build
    build_kind: BuildKind,
}

impl SourceBuild {
    /// Create a virtual environment in which to build a source distribution, extracting the
    /// contents from an archive if necessary.
    ///
    /// `source_dist` is for error reporting only.
    #[allow(clippy::too_many_arguments)]
    pub async fn setup(
        source: &Path,
        subdirectory: Option<&Path>,
        interpreter: &Interpreter,
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        package_id: String,
        setup_py: SetupPyStrategy,
        build_kind: BuildKind,
    ) -> Result<SourceBuild, Error> {
        let temp_dir = tempdir()?;

        let source_root = if fs::metadata(source)?.is_dir() {
            source.to_path_buf()
        } else {
            debug!("Unpacking for build: {}", source.display());
            let extracted = temp_dir.path().join("extracted");
            extract_source(source, &extracted)
                .map_err(|err| Error::Extraction(extracted.clone(), err))?
        };
        let source_tree = if let Some(subdir) = subdirectory {
            source_root.join(subdir)
        } else {
            source_root
        };

        let default_backend: Pep517Backend = DEFAULT_BACKEND.clone();

        // Check if we have a PEP 517 build backend.
        let pep517_backend = match fs::read_to_string(source_tree.join("pyproject.toml")) {
            Ok(toml) => {
                let pyproject_toml: PyProjectToml =
                    toml::from_str(&toml).map_err(Error::InvalidPyprojectToml)?;
                if let Some(build_system) = pyproject_toml.build_system {
                    Some(Pep517Backend {
                        // If `build-backend` is missing, inject the legacy setuptools backend, but
                        // retain the `requires`, to match `pip` and `build`. Note that while PEP 517
                        // says that in this case we "should revert to the legacy behaviour of running
                        // `setup.py` (either directly, or by implicitly invoking the
                        // `setuptools.build_meta:__legacy__` backend)", we found that in practice, only
                        // the legacy setuptools backend is allowed. See also:
                        // https://github.com/pypa/build/blob/de5b44b0c28c598524832dff685a98d5a5148c44/src/build/__init__.py#L114-L118
                        backend: build_system
                            .build_backend
                            .unwrap_or_else(|| "setuptools.build_meta:__legacy__".to_string()),
                        backend_path: build_system.backend_path,
                        requirements: build_system.requires,
                    })
                } else {
                    // If a `pyproject.toml` is present, but `[build-system]` is missing, proceed with
                    // a PEP 517 build using the default backend, to match `pip` and `build`.
                    Some(default_backend.clone())
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // We require either a `pyproject.toml` or a `setup.py` file at the top level.
                if !source_tree.join("setup.py").is_file() {
                    return Err(Error::InvalidSourceDist(
                        "The archive contains neither a `pyproject.toml` nor a `setup.py` file at the top level"
                            .to_string(),
                    ));
                }

                // If no `pyproject.toml` is present, by default, proceed with a PEP 517 build using
                // the default backend, to match `build`. `pip` uses `setup.py` directly in this
                // case (which we allow via `SetupPyStrategy::Setuptools`), but plans to make PEP
                // 517 builds the default in the future.
                // See: https://github.com/pypa/pip/issues/9175.
                match setup_py {
                    SetupPyStrategy::Pep517 => Some(default_backend.clone()),
                    SetupPyStrategy::Setuptools => None,
                }
            }
            Err(err) => return Err(err.into()),
        };

        let venv = gourgeist::create_venv(&temp_dir.path().join(".venv"), interpreter.clone())?;

        // Setup the build environment.
        let resolved_requirements = if let Some(pep517_backend) = pep517_backend.as_ref() {
            if pep517_backend.requirements == default_backend.requirements {
                let mut resolution = source_build_context.setup_py_resolution.lock().await;
                if let Some(resolved_requirements) = &*resolution {
                    resolved_requirements.clone()
                } else {
                    let resolved_requirements = build_context
                        .resolve(&default_backend.requirements)
                        .await
                        .map_err(|err| {
                            Error::RequirementsInstall("setup.py build (resolve)", err)
                        })?;
                    *resolution = Some(resolved_requirements.clone());
                    resolved_requirements
                }
            } else {
                build_context
                    .resolve(&pep517_backend.requirements)
                    .await
                    .map_err(|err| {
                        Error::RequirementsInstall("build-system.requires (resolve)", err)
                    })?
            }
        } else {
            // Install default requirements for `setup.py`-based builds.
            let mut resolution = source_build_context.setup_py_resolution.lock().await;
            if let Some(resolved_requirements) = &*resolution {
                resolved_requirements.clone()
            } else {
                let resolved_requirements = build_context
                    .resolve(&default_backend.requirements)
                    .await
                    .map_err(|err| Error::RequirementsInstall("setup.py build (resolve)", err))?;
                *resolution = Some(resolved_requirements.clone());
                resolved_requirements
            }
        };

        build_context
            .install(&resolved_requirements, &venv)
            .await
            .map_err(|err| Error::RequirementsInstall("build-system.requires (install)", err))?;

        let mut pep517_daemon = Pep517Daemon::new(&venv, &source_tree).await?;

        // If we're using the default backend configuration, skip `get_requires_for_build_*`, since
        // we already installed the requirements above.
        if let Some(pep517_backend) = &pep517_backend {
            if pep517_backend != &default_backend {
                let environment = create_pep517_build_environment(
                    &source_tree,
                    &venv,
                    pep517_backend,
                    &mut pep517_daemon,
                    build_context,
                    &package_id,
                    build_kind,
                )
                .await;

                if environment.is_err() {
                    pep517_daemon.close().await?;
                }

                environment?
            }
        }

        Ok(Self {
            temp_dir,
            source_tree,
            pep517_backend,
            pep517_daemon,
            venv,
            build_kind,
            metadata_directory: None,
            package_id,
        })
    }

    /// Try calling `prepare_metadata_for_build_wheel` to get the metadata without executing the
    /// actual build.
    pub async fn get_metadata_without_build(&mut self) -> Result<Option<PathBuf>, Error> {
        let Some(pep517_backend) = &self.pep517_backend else {
            return Ok(None);
        };

        // We've already called this method; return the existing result.
        if let Some(metadata_dir) = &self.metadata_directory {
            return Ok(Some(metadata_dir.clone()));
        }

        let metadata_directory = self.temp_dir.path().join("metadata_directory");
        fs::create_dir(&metadata_directory)?;

        let result = self
            .pep517_daemon
            .prepare_metadata_for_build_wheel(&pep517_backend, metadata_directory.clone())
            .await?;

        if let Some(path) = result {
            self.metadata_directory = Some(metadata_directory.join(path));
            Ok(self.metadata_directory.clone())
        } else {
            Ok(None)
        }
    }

    /// Build a source distribution from an archive (`.zip` or `.tar.gz`), return the location of the
    /// built wheel.
    ///
    /// The location will be inside `temp_dir`, i.e. you must use the wheel before dropping the temp
    /// dir.
    ///
    /// <https://packaging.python.org/en/latest/specifications/source-distribution-format/>
    #[instrument(skip_all, fields(package_id = self.package_id))]
    pub async fn build(&mut self, wheel_dir: &Path) -> Result<String, Error> {
        // The build scripts run with the extracted root as cwd, so they need the absolute path.
        let wheel_dir = fs::canonicalize(wheel_dir)?;

        if let Some(pep517_backend) = self.pep517_backend.clone() {
            // Prevent clashes from two puffin processes building wheels in parallel.
            let tmp_dir = tempdir_in(&wheel_dir)?;
            let filename = self.pep517_build(tmp_dir.path(), &pep517_backend).await?;

            let from = tmp_dir.path().join(&filename);
            let to = wheel_dir.join(&filename);
            fs_err::rename(from, to)?;
            Ok(filename)
        } else {
            if self.build_kind != BuildKind::Wheel {
                return Err(Error::EditableSetupPy);
            }
            // We checked earlier that setup.py exists.
            let python_interpreter = self.venv.python_executable();
            let span = info_span!(
                "run_python_script",
                script="setup.py bdist_wheel",
                python_version = %self.venv.interpreter().version()
            );
            let output = Command::new(&python_interpreter)
                .args(["setup.py", "bdist_wheel"])
                .current_dir(&self.source_tree)
                .output()
                .instrument(span)
                .await
                .map_err(|err| Error::CommandFailed(python_interpreter, err))?;
            if !output.status.success() {
                return Err(Error::from_command_output(
                    "Failed building wheel through setup.py".to_string(),
                    &output,
                    &self.package_id,
                ));
            }
            let dist = fs::read_dir(self.source_tree.join("dist"))?;
            let dist_dir = dist.collect::<io::Result<Vec<fs_err::DirEntry>>>()?;
            let [dist_wheel] = dist_dir.as_slice() else {
                return Err(Error::from_command_output(
                    format!(
                        "Expected exactly wheel in `dist/` after invoking setup.py, found {dist_dir:?}"
                    ),
                    &output,
                    &self.package_id)
                );
            };

            let from = dist_wheel.path();
            let to = wheel_dir.join(dist_wheel.file_name());
            fs_err::copy(from, to)?;

            Ok(dist_wheel.file_name().to_string_lossy().to_string())
        }
    }

    async fn pep517_build(
        &mut self,
        wheel_dir: &Path,
        pep517_backend: &Pep517Backend,
    ) -> Result<String, Error> {
        let distribution_filename = self
            .pep517_daemon
            .build(
                &pep517_backend,
                self.build_kind,
                wheel_dir,
                self.metadata_directory.clone().take().as_deref(),
            )
            .await?;

        if wheel_dir.join(distribution_filename.clone()).is_file() {
            Ok(distribution_filename)
        } else {
            Err(Error::DaemonError {
                message: "failed to build wheel".to_string(),
            })
        }
    }
}

impl SourceBuildTrait for SourceBuild {
    async fn metadata(&mut self) -> anyhow::Result<Option<PathBuf>> {
        Ok(self.get_metadata_without_build().await?)
    }

    async fn wheel<'a>(&'a mut self, wheel_dir: &'a Path) -> anyhow::Result<String> {
        Ok(self.build(wheel_dir).await?)
    }

    async fn finish(&mut self) -> anyhow::Result<()> {
        self.pep517_daemon.close().await?;
        Ok(())
    }
}
/// Not a method because we call it before the builder is completely initialized
async fn create_pep517_build_environment(
    source_tree: &Path,
    venv: &Virtualenv,
    pep517_backend: &Pep517Backend,
    pep517_daemon: &mut Pep517Daemon,
    build_context: &impl BuildContext,
    package_id: &str,
    build_kind: BuildKind,
) -> Result<(), Error> {
    let extra_requires = pep517_daemon
        .get_requires_for_build(pep517_backend, build_kind)
        .await?;

    // Some packages (such as tqdm 4.66.1) list only extra requires that have already been part of
    // the pyproject.toml requires (in this case, `wheel`). We can skip doing the whole resolution
    // and installation again.
    // TODO(konstin): Do we still need this when we have a fast resolver?
    if extra_requires
        .iter()
        .any(|req| !pep517_backend.requirements.contains(req))
    {
        debug!("Installing extra requirements for build backend");
        let requirements: Vec<Requirement> = pep517_backend
            .requirements
            .iter()
            .cloned()
            .chain(extra_requires)
            .collect();
        let resolution = build_context
            .resolve(&requirements)
            .await
            .map_err(|err| Error::RequirementsInstall("build-system.requires (resolve)", err))?;

        build_context
            .install(&resolution, venv)
            .await
            .map_err(|err| Error::RequirementsInstall("build-system.requires (install)", err))?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::process::{ExitStatus, Output};

    use indoc::indoc;

    use crate::Error;

    #[test]
    fn missing_header() {
        let output = Output {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: indoc!(r"
                running bdist_wheel
                running build
                [...]
                creating build/temp.linux-x86_64-cpython-39/pygraphviz
                gcc -Wno-unused-result -Wsign-compare -DNDEBUG -g -fwrapv -O3 -Wall -DOPENSSL_NO_SSL3 -fPIC -DSWIG_PYTHON_STRICT_BYTE_CHAR -I/tmp/.tmpy6vVes/.venv/include -I/home/konsti/.pyenv/versions/3.9.18/include/python3.9 -c pygraphviz/graphviz_wrap.c -o build/temp.linux-x86_64-cpython-39/pygraphviz/graphviz_wrap.o
                "
            ).as_bytes().to_vec(),
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
            ).as_bytes().to_vec(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            "pygraphviz-1.11",
        );
        assert!(matches!(err, Error::MissingHeader { .. }));
        insta::assert_display_snapshot!(err, @r###"
        Failed building wheel through setup.py:
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
        insta::assert_display_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @r###"This error likely indicates that you need to install a library that provides "graphviz/cgraph.h" for pygraphviz-1.11"###
        );
    }

    #[test]
    fn missing_linker_library() {
        let output = Output {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: Vec::new(),
            stderr: indoc!(
                r"
                1099 |     n = strlen(p);
                     |         ^~~~~~~~~
               /usr/bin/ld: cannot find -lncurses: No such file or directory
               collect2: error: ld returned 1 exit status
               error: command '/usr/bin/x86_64-linux-gnu-gcc' failed with exit code 1
                "
            )
            .as_bytes()
            .to_vec(),
        };

        let err = Error::from_command_output(
            "Failed building wheel through setup.py".to_string(),
            &output,
            "pygraphviz-1.11",
        );
        assert!(matches!(err, Error::MissingHeader { .. }));
        insta::assert_display_snapshot!(err, @r###"
        Failed building wheel through setup.py:
        --- stdout:

        --- stderr:
        1099 |     n = strlen(p);
              |         ^~~~~~~~~
        /usr/bin/ld: cannot find -lncurses: No such file or directory
        collect2: error: ld returned 1 exit status
        error: command '/usr/bin/x86_64-linux-gnu-gcc' failed with exit code 1
        ---
        "###);
        insta::assert_display_snapshot!(
            std::error::Error::source(&err).unwrap(),
            @"This error likely indicates that you need to install the library that provides a shared library for ncurses for pygraphviz-1.11 (e.g. libncurses-dev)"
        );
    }
}
