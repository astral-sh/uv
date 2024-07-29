//! Build wheels from source distributions
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

use fs_err as fs;
use indoc::formatdoc;
use itertools::Itertools;
use regex::Regex;
use rustc_hash::FxHashMap;
use serde::de::{value, SeqAccess, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::LazyLock;
use std::{env, iter};
use tempfile::{tempdir_in, TempDir};
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, info_span, instrument, Instrument};

use distribution_types::Resolution;
use pep440_rs::Version;
use pep508_rs::PackageName;
use pypi_types::{Requirement, VerbatimParsedUrl};
use uv_configuration::{BuildKind, ConfigSettings, SetupPyStrategy};
use uv_fs::{rename_with_retry, PythonExt, Simplified};
use uv_python::{Interpreter, PythonEnvironment};
use uv_types::{BuildContext, BuildIsolation, SourceBuildTrait};

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

/// The default backend to use when PEP 517 is used without a `build-system` section.
static DEFAULT_BACKEND: LazyLock<Pep517Backend> = LazyLock::new(|| Pep517Backend {
    backend: "setuptools.build_meta:__legacy__".to_string(),
    backend_path: None,
    requirements: vec![Requirement::from(
        pep508_rs::Requirement::from_str("setuptools >= 40.8.0").unwrap(),
    )],
});

/// The requirements for `--legacy-setup-py` builds.
static SETUP_PY_REQUIREMENTS: LazyLock<[Requirement; 2]> = LazyLock::new(|| {
    [
        Requirement::from(pep508_rs::Requirement::from_str("setuptools >= 40.8.0").unwrap()),
        Requirement::from(pep508_rs::Requirement::from_str("wheel").unwrap()),
    ]
});

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid source distribution: {0}")]
    InvalidSourceDist(String),
    #[error("Invalid `pyproject.toml`")]
    InvalidPyprojectToml(#[from] toml::de::Error),
    #[error("Editable installs with setup.py legacy builds are unsupported, please specify a build backend in pyproject.toml")]
    EditableSetupPy,
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] anyhow::Error),
    #[error("Failed to create temporary virtualenv")]
    Virtualenv(#[from] uv_virtualenv::Error),
    #[error("Failed to run {0}")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    BuildBackend {
        message: String,
        exit_code: ExitStatus,
        stdout: String,
        stderr: String,
    },
    /// Nudge the user towards installing the missing dev library
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    MissingHeader {
        message: String,
        exit_code: ExitStatus,
        stdout: String,
        stderr: String,
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
    PythonPackage(String),
}

#[derive(Debug, Error)]
pub struct MissingHeaderCause {
    missing_library: MissingLibrary,
    version_id: String,
}

impl Display for MissingHeaderCause {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.missing_library {
            MissingLibrary::Header(header) => {
                write!(
                    f,
                    "This error likely indicates that you need to install a library that provides \"{}\" for {}",
                    header, self.version_id
                )
            }
            MissingLibrary::Linker(library) => {
                write!(
                    f,
                    "This error likely indicates that you need to install the library that provides a shared library \
                    for {library} for {version_id} (e.g. lib{library}-dev)",
                    library = library, version_id = self.version_id
                )
            }
            MissingLibrary::PythonPackage(package) => {
                write!(
                    f,
                    "This error likely indicates that {version_id} depends on {package}, but doesn't declare it as a build dependency. \
                        If {version_id} is a first-party package, consider adding {package} to its `build-system.requires`. \
                        Otherwise, `uv pip install {package}` into the environment and re-run with `--no-build-isolation`.",
                    package = package, version_id = self.version_id
                )
            }
        }
    }
}

impl Error {
    fn from_command_output(
        message: String,
        output: &Output,
        version_id: impl Into<String>,
    ) -> Self {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        // In the cases i've seen it was the 5th and 3rd last line (see test case), 10 seems like a reasonable cutoff
        let missing_library = stderr.lines().rev().take(10).find_map(|line| {
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
                Some(MissingLibrary::PythonPackage("wheel".to_string()))
            } else if TORCH_NOT_FOUND_RE.is_match(line.trim()) {
                Some(MissingLibrary::PythonPackage("torch".to_string()))
            } else {
                None
            }
        });

        if let Some(missing_library) = missing_library {
            return Self::MissingHeader {
                message,
                exit_code: output.status,
                stdout,
                stderr,
                missing_header_cause: MissingHeaderCause {
                    missing_library,
                    version_id: version_id.into(),
                },
            };
        }

        Self::BuildBackend {
            message,
            exit_code: output.status,
            stdout,
            stderr,
        }
    }
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    /// Build-related data
    build_system: Option<BuildSystem>,
    /// Project metadata
    project: Option<Project>,
}

/// The `[project]` section of a pyproject.toml as specified in PEP 621.
///
/// This representation only includes a subset of the fields defined in PEP 621 necessary for
/// informing wheel builds.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct Project {
    /// The name of the project
    name: PackageName,
    /// The version of the project as supported by PEP 440
    version: Option<Version>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified so another tool
    /// can/will provide such metadata dynamically.
    dynamic: Option<Vec<String>>,
}

/// The `[build-system]` section of a pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct BuildSystem {
    /// PEP 508 dependencies required to execute the build system.
    requires: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    /// A string naming a Python object that will be used to perform the build.
    build_backend: Option<String>,
    /// Specify that their backend code is hosted in-tree, this key contains a list of directories.
    backend_path: Option<BackendPath>,
}

impl BackendPath {
    /// Return an iterator over the paths in the backend path.
    fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendPath(Vec<String>);

impl<'de> Deserialize<'de> for BackendPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrVec;

        impl<'de> Visitor<'de> for StringOrVec {
            type Value = Vec<String>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("list of strings")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Allow exactly `backend-path = "."`, as used in `flit_core==2.3.0`.
                if s == "." {
                    Ok(vec![".".to_string()])
                } else {
                    Err(de::Error::invalid_value(de::Unexpected::Str(s), &self))
                }
            }

            fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
            where
                S: SeqAccess<'de>,
            {
                Deserialize::deserialize(value::SeqAccessDeserializer::new(seq))
            }
        }

        deserializer.deserialize_any(StringOrVec).map(BackendPath)
    }
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
    backend_path: Option<BackendPath>,
}

impl Pep517Backend {
    fn backend_import(&self) -> String {
        let import = if let Some((path, object)) = self.backend.split_once(':') {
            format!("from {path} import {object} as backend")
        } else {
            format!("import {} as backend", self.backend)
        };

        let backend_path_encoded = self
            .backend_path
            .iter()
            .flat_map(BackendPath::iter)
            .map(|path| {
                // Turn into properly escaped python string
                '"'.to_string()
                    + &path.replace('\\', "\\\\").replace('"', "\\\"")
                    + &'"'.to_string()
            })
            .join(", ");

        // > Projects can specify that their backend code is hosted in-tree by including the
        // > backend-path key in pyproject.toml. This key contains a list of directories, which the
        // > frontend will add to the start of sys.path when loading the backend, and running the
        // > backend hooks.
        formatdoc! {r#"
            import sys

            if sys.path[0] == "":
                sys.path.pop(0)

            sys.path = [{backend_path}] + sys.path

            {import}
        "#, backend_path = backend_path_encoded}
    }
}

/// Uses an [`Rc`] internally, clone freely.
#[derive(Debug, Default, Clone)]
pub struct SourceBuildContext {
    /// An in-memory resolution of the default backend's requirements for PEP 517 builds.
    default_resolution: Rc<Mutex<Option<Resolution>>>,
    /// An in-memory resolution of the build requirements for `--legacy-setup-py` builds.
    setup_py_resolution: Rc<Mutex<Option<Resolution>>>,
}

/// Holds the state through a series of PEP 517 frontend to backend calls or a single setup.py
/// invocation.
///
/// This keeps both the temp dir and the result of a potential `prepare_metadata_for_build_wheel`
/// call which changes how we call `build_wheel`.
pub struct SourceBuild {
    temp_dir: TempDir,
    source_tree: PathBuf,
    config_settings: ConfigSettings,
    /// If performing a PEP 517 build, the backend to use.
    pep517_backend: Option<Pep517Backend>,
    /// The PEP 621 project metadata, if any.
    project: Option<Project>,
    /// The virtual environment in which to build the source distribution.
    venv: PythonEnvironment,
    /// Populated if `prepare_metadata_for_build_wheel` was called.
    ///
    /// > If the build frontend has previously called `prepare_metadata_for_build_wheel` and depends
    /// > on the wheel resulting from this call to have metadata matching this earlier call, then
    /// > it should provide the path to the created .dist-info directory as the `metadata_directory`
    /// > argument. If this argument is provided, then `build_wheel` MUST produce a wheel with
    /// > identical metadata. The directory passed in by the build frontend MUST be identical to the
    /// > directory created by `prepare_metadata_for_build_wheel`, including any unrecognized files
    /// > it created.
    metadata_directory: Option<PathBuf>,
    /// Package id such as `foo-1.2.3`, for error reporting
    version_id: String,
    /// Whether we do a regular PEP 517 build or an PEP 660 editable build
    build_kind: BuildKind,
    /// Modified PATH that contains the `venv_bin`, `user_path` and `system_path` variables in that order
    modified_path: OsString,
    /// Environment variables to be passed in during metadata or wheel building
    environment_variables: FxHashMap<OsString, OsString>,
    /// Runner for Python scripts.
    runner: PythonRunner,
}

impl SourceBuild {
    /// Create a virtual environment in which to build a source distribution, extracting the
    /// contents from an archive if necessary.
    ///
    /// `source_dist` is for error reporting only.
    pub async fn setup(
        source: &Path,
        subdirectory: Option<&Path>,
        interpreter: &Interpreter,
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        version_id: String,
        setup_py: SetupPyStrategy,
        config_settings: ConfigSettings,
        build_isolation: BuildIsolation<'_>,
        build_kind: BuildKind,
        mut environment_variables: FxHashMap<OsString, OsString>,
        concurrent_builds: usize,
    ) -> Result<Self, Error> {
        let temp_dir = build_context.cache().environment()?;

        let source_tree = if let Some(subdir) = subdirectory {
            source.join(subdir)
        } else {
            source.to_path_buf()
        };

        let default_backend: Pep517Backend = DEFAULT_BACKEND.clone();

        // Check if we have a PEP 517 build backend.
        let (pep517_backend, project) =
            Self::extract_pep517_backend(&source_tree, setup_py, &default_backend)
                .map_err(|err| *err)?;

        // Create a virtual environment, or install into the shared environment if requested.
        let venv = match build_isolation {
            BuildIsolation::Isolated => uv_virtualenv::create_venv(
                temp_dir.path(),
                interpreter.clone(),
                uv_virtualenv::Prompt::None,
                false,
                false,
                false,
            )?,
            BuildIsolation::Shared(venv) => venv.clone(),
        };

        // Setup the build environment. If build isolation is disabled, we assume the build
        // environment is already setup.
        if build_isolation.is_isolated() {
            let resolved_requirements = Self::get_resolved_requirements(
                build_context,
                source_build_context,
                &default_backend,
                pep517_backend.as_ref(),
            )
            .await?;

            build_context
                .install(&resolved_requirements, &venv)
                .await
                .map_err(|err| {
                    Error::RequirementsInstall("build-system.requires (install)", err)
                })?;
        }

        // Figure out what the modified path should be
        // Remove the PATH variable from the environment variables if it's there
        let user_path = environment_variables.remove(&OsString::from("PATH"));
        // See if there is an OS PATH variable
        let os_path = env::var_os("PATH");

        // Prepend the user supplied PATH to the existing OS PATH
        let modified_path = if let Some(user_path) = user_path {
            match os_path {
                // Prepend the user supplied PATH to the existing PATH
                Some(env_path) => {
                    let user_path = PathBuf::from(user_path);
                    let new_path = env::split_paths(&user_path).chain(env::split_paths(&env_path));
                    Some(env::join_paths(new_path).map_err(Error::BuildScriptPath)?)
                }
                // Use the user supplied PATH
                None => Some(user_path),
            }
        } else {
            os_path
        };

        // Prepend the venv bin directory to the modified path
        let modified_path = if let Some(path) = modified_path {
            let venv_path = iter::once(venv.scripts().to_path_buf()).chain(env::split_paths(&path));
            env::join_paths(venv_path).map_err(Error::BuildScriptPath)?
        } else {
            OsString::from(venv.scripts())
        };

        // Create the PEP 517 build environment. If build isolation is disabled, we assume the build
        // environment is already setup.
        let runner = PythonRunner::new(concurrent_builds);
        if build_isolation.is_isolated() {
            if let Some(pep517_backend) = &pep517_backend {
                create_pep517_build_environment(
                    &runner,
                    &source_tree,
                    &venv,
                    pep517_backend,
                    build_context,
                    &version_id,
                    build_kind,
                    &config_settings,
                    &environment_variables,
                    &modified_path,
                    &temp_dir,
                )
                .await?;
            }
        }

        Ok(Self {
            temp_dir,
            source_tree,
            pep517_backend,
            project,
            venv,
            build_kind,
            config_settings,
            metadata_directory: None,
            version_id,
            environment_variables,
            modified_path,
            runner,
        })
    }

    async fn get_resolved_requirements(
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        default_backend: &Pep517Backend,
        pep517_backend: Option<&Pep517Backend>,
    ) -> Result<Resolution, Error> {
        Ok(if let Some(pep517_backend) = pep517_backend {
            if pep517_backend.requirements == default_backend.requirements {
                let mut resolution = source_build_context.default_resolution.lock().await;
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
                    .resolve(&*SETUP_PY_REQUIREMENTS)
                    .await
                    .map_err(|err| Error::RequirementsInstall("setup.py build (resolve)", err))?;
                *resolution = Some(resolved_requirements.clone());
                resolved_requirements
            }
        })
    }

    /// Extract the PEP 517 backend from the `pyproject.toml` or `setup.py` file.
    fn extract_pep517_backend(
        source_tree: &Path,
        setup_py: SetupPyStrategy,
        default_backend: &Pep517Backend,
    ) -> Result<(Option<Pep517Backend>, Option<Project>), Box<Error>> {
        match fs::read_to_string(source_tree.join("pyproject.toml")) {
            Ok(toml) => {
                let pyproject_toml: PyProjectToml =
                    toml::from_str(&toml).map_err(Error::InvalidPyprojectToml)?;
                let backend = if let Some(build_system) = pyproject_toml.build_system {
                    Pep517Backend {
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
                        requirements: build_system
                            .requires
                            .into_iter()
                            .map(Requirement::from)
                            .collect(),
                    }
                } else {
                    // If a `pyproject.toml` is present, but `[build-system]` is missing, proceed with
                    // a PEP 517 build using the default backend, to match `pip` and `build`.
                    default_backend.clone()
                };
                Ok((Some(backend), pyproject_toml.project))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // We require either a `pyproject.toml` or a `setup.py` file at the top level.
                if !source_tree.join("setup.py").is_file() {
                    return Err(Box::new(Error::InvalidSourceDist(
                        "The archive contains neither a `pyproject.toml` nor a `setup.py` file at the top level"
                            .to_string(),
                    )));
                }

                // If no `pyproject.toml` is present, by default, proceed with a PEP 517 build using
                // the default backend, to match `build`. `pip` uses `setup.py` directly in this
                // case (which we allow via `SetupPyStrategy::Setuptools`), but plans to make PEP
                // 517 builds the default in the future.
                // See: https://github.com/pypa/pip/issues/9175.
                match setup_py {
                    SetupPyStrategy::Pep517 => Ok((Some(default_backend.clone()), None)),
                    SetupPyStrategy::Setuptools => Ok((None, None)),
                }
            }
            Err(err) => Err(Box::new(err.into())),
        }
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

        // Hatch allows for highly dynamic customization of metadata via hooks. In such cases, Hatch
        // can't uphold the PEP 517 contract, in that the metadata Hatch would return by
        // `prepare_metadata_for_build_wheel` isn't guaranteed to match that of the built wheel.
        //
        // Hatch disables `prepare_metadata_for_build_wheel` entirely for pip. We'll instead disable
        // it on our end when metadata is defined as "dynamic" in the pyproject.toml, which should
        // allow us to leverage the hook in _most_ cases while still avoiding incorrect metadata for
        // the remaining cases.
        //
        // This heuristic will have false positives (i.e., there will be some Hatch projects for
        // which we could have safely called `prepare_metadata_for_build_wheel`, despite having
        // dynamic metadata). However, false positives are preferable to false negatives, since
        // this is just an optimization.
        //
        // See: https://github.com/astral-sh/uv/issues/2130
        if pep517_backend.backend == "hatchling.build" {
            if self
                .project
                .as_ref()
                .and_then(|project| project.dynamic.as_ref())
                .is_some_and(|dynamic| {
                    dynamic
                        .iter()
                        .any(|field| field == "dependencies" || field == "optional-dependencies")
                })
            {
                return Ok(None);
            }
        }

        let metadata_directory = self.temp_dir.path().join("metadata_directory");
        fs::create_dir(&metadata_directory)?;

        // Write the hook output to a file so that we can read it back reliably.
        let outfile = self.temp_dir.path().join(format!(
            "prepare_metadata_for_build_{}.txt",
            self.build_kind
        ));

        debug!(
            "Calling `{}.prepare_metadata_for_build_{}()`",
            pep517_backend.backend, self.build_kind,
        );
        let script = formatdoc! {
            r#"
            {}
            import json

            prepare_metadata_for_build = getattr(backend, "prepare_metadata_for_build_{}", None)
            if prepare_metadata_for_build:
                dirname = prepare_metadata_for_build("{}", {})
            else:
                dirname = None

            with open("{}", "w") as fp:
                fp.write(dirname or "")
            "#,
            pep517_backend.backend_import(),
            self.build_kind,
            escape_path_for_python(&metadata_directory),
            self.config_settings.escape_for_python(),
            outfile.escape_for_python(),
        };
        let span = info_span!(
            "run_python_script",
            script=format!("prepare_metadata_for_build_{}", self.build_kind),
            python_version = %self.venv.interpreter().python_version()
        );
        let output = self
            .runner
            .run_script(
                &self.venv,
                &script,
                &self.source_tree,
                &self.environment_variables,
                &self.modified_path,
            )
            .instrument(span)
            .await?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                format!("Build backend failed to determine metadata through `prepare_metadata_for_build_{}`", self.build_kind),
                &output,
                &self.version_id,
            ));
        }

        let dirname = fs::read_to_string(&outfile)?;
        if dirname.is_empty() {
            return Ok(None);
        }
        self.metadata_directory = Some(metadata_directory.join(dirname));
        Ok(self.metadata_directory.clone())
    }

    /// Build a source distribution from an archive (`.zip` or `.tar.gz`), return the location of the
    /// built wheel.
    ///
    /// The location will be inside `temp_dir`, i.e. you must use the wheel before dropping the temp
    /// dir.
    ///
    /// <https://packaging.python.org/en/latest/specifications/source-distribution-format/>
    #[instrument(skip_all, fields(version_id = self.version_id))]
    pub async fn build_wheel(&self, wheel_dir: &Path) -> Result<String, Error> {
        // The build scripts run with the extracted root as cwd, so they need the absolute path.
        let wheel_dir = fs::canonicalize(wheel_dir)?;

        if let Some(pep517_backend) = &self.pep517_backend {
            // Prevent clashes from two uv processes building wheels in parallel.
            let tmp_dir = tempdir_in(&wheel_dir)?;
            let filename = self.pep517_build(tmp_dir.path(), pep517_backend).await?;

            let from = tmp_dir.path().join(&filename);
            let to = wheel_dir.join(&filename);
            rename_with_retry(from, to).await?;
            Ok(filename)
        } else {
            if self.build_kind != BuildKind::Wheel {
                return Err(Error::EditableSetupPy);
            }
            // We checked earlier that setup.py exists.
            let span = info_span!(
                "run_python_script",
                script="setup.py bdist_wheel",
                python_version = %self.venv.interpreter().python_version()
            );
            let output = self
                .runner
                .run_setup_py(&self.venv, "bdist_wheel", &self.source_tree)
                .instrument(span)
                .await?;
            if !output.status.success() {
                return Err(Error::from_command_output(
                    "Failed building wheel through setup.py".to_string(),
                    &output,
                    &self.version_id,
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
                    &self.version_id)
                );
            };

            let from = dist_wheel.path();
            let to = wheel_dir.join(dist_wheel.file_name());
            fs_err::copy(from, to)?;

            Ok(dist_wheel.file_name().to_string_lossy().to_string())
        }
    }

    async fn pep517_build(
        &self,
        wheel_dir: &Path,
        pep517_backend: &Pep517Backend,
    ) -> Result<String, Error> {
        let metadata_directory = self
            .metadata_directory
            .as_deref()
            .map_or("None".to_string(), |path| {
                format!(r#""{}""#, path.escape_for_python())
            });

        // Write the hook output to a file so that we can read it back reliably.
        let outfile = self
            .temp_dir
            .path()
            .join(format!("build_{}.txt", self.build_kind));

        debug!(
            r#"Calling `{}.build_{}("{}", {}, {})`"#,
            pep517_backend.backend,
            self.build_kind,
            wheel_dir.escape_for_python(),
            self.config_settings.escape_for_python(),
            metadata_directory,
        );
        let script = formatdoc! {
            r#"
            {}

            wheel_filename = backend.build_{}("{}", {}, {})
            with open("{}", "w") as fp:
                fp.write(wheel_filename)
            "#,
            pep517_backend.backend_import(),
            self.build_kind,
            wheel_dir.escape_for_python(),
            self.config_settings.escape_for_python(),
            metadata_directory,
            outfile.escape_for_python()
        };
        let span = info_span!(
            "run_python_script",
            script=format!("build_{}", self.build_kind),
            python_version = %self.venv.interpreter().python_version()
        );
        let output = self
            .runner
            .run_script(
                &self.venv,
                &script,
                &self.source_tree,
                &self.environment_variables,
                &self.modified_path,
            )
            .instrument(span)
            .await?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                format!(
                    "Build backend failed to build wheel through `build_{}()`",
                    self.build_kind
                ),
                &output,
                &self.version_id,
            ));
        }

        let distribution_filename = fs::read_to_string(&outfile)?;
        if !wheel_dir.join(&distribution_filename).is_file() {
            return Err(Error::from_command_output(
                format!(
                    "Build backend failed to produce wheel through `build_{}()`: `{distribution_filename}` not found",
                    self.build_kind
                ),
                &output,
                &self.version_id,
            ));
        }
        Ok(distribution_filename)
    }
}

impl SourceBuildTrait for SourceBuild {
    async fn metadata(&mut self) -> anyhow::Result<Option<PathBuf>> {
        Ok(self.get_metadata_without_build().await?)
    }

    async fn wheel<'a>(&'a self, wheel_dir: &'a Path) -> anyhow::Result<String> {
        Ok(self.build_wheel(wheel_dir).await?)
    }
}

fn escape_path_for_python(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Not a method because we call it before the builder is completely initialized
async fn create_pep517_build_environment(
    runner: &PythonRunner,
    source_tree: &Path,
    venv: &PythonEnvironment,
    pep517_backend: &Pep517Backend,
    build_context: &impl BuildContext,
    version_id: &str,
    build_kind: BuildKind,
    config_settings: &ConfigSettings,
    environment_variables: &FxHashMap<OsString, OsString>,
    modified_path: &OsString,
    temp_dir: &TempDir,
) -> Result<(), Error> {
    // Write the hook output to a file so that we can read it back reliably.
    let outfile = temp_dir
        .path()
        .join(format!("get_requires_for_build_{build_kind}.txt"));

    debug!(
        "Calling `{}.get_requires_for_build_{}()`",
        pep517_backend.backend, build_kind
    );

    let script = formatdoc! {
        r#"
            {}
            import json

            get_requires_for_build = getattr(backend, "get_requires_for_build_{}", None)
            if get_requires_for_build:
                requires = get_requires_for_build({})
            else:
                requires = []

            with open("{}", "w") as fp:
                json.dump(requires, fp)
        "#,
        pep517_backend.backend_import(),
        build_kind,
        config_settings.escape_for_python(),
        outfile.escape_for_python()
    };
    let span = info_span!(
        "run_python_script",
        script=format!("get_requires_for_build_{}", build_kind),
        python_version = %venv.interpreter().python_version()
    );
    let output = runner
        .run_script(
            venv,
            &script,
            source_tree,
            environment_variables,
            modified_path,
        )
        .instrument(span)
        .await?;
    if !output.status.success() {
        return Err(Error::from_command_output(
            format!("Build backend failed to determine extra requires with `build_{build_kind}()`"),
            &output,
            version_id,
        ));
    }

    // Read the requirements from the output file.
    let contents = fs_err::read(&outfile).map_err(|err| {
        Error::from_command_output(
            format!(
                "Build backend failed to read extra requires from `get_requires_for_build_{build_kind}`: {err}"
            ),
            &output,
            version_id,
        )
    })?;

    // Deserialize the requirements from the output file.
    let extra_requires: Vec<pep508_rs::Requirement<VerbatimParsedUrl>> = serde_json::from_slice::<Vec<pep508_rs::Requirement<VerbatimParsedUrl>>>(&contents).map_err(|err| {
        Error::from_command_output(
            format!(
                "Build backend failed to return extra requires with `get_requires_for_build_{build_kind}`: {err}"
            ),
            &output,
            version_id,
        )
    })?;
    let extra_requires: Vec<_> = extra_requires.into_iter().map(Requirement::from).collect();

    // Some packages (such as tqdm 4.66.1) list only extra requires that have already been part of
    // the pyproject.toml requires (in this case, `wheel`). We can skip doing the whole resolution
    // and installation again.
    // TODO(konstin): Do we still need this when we have a fast resolver?
    if extra_requires
        .iter()
        .any(|req| !pep517_backend.requirements.contains(req))
    {
        debug!("Installing extra requirements for build backend");
        let requirements: Vec<_> = pep517_backend
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

/// A runner that manages the execution of external python processes with a
/// concurrency limit.
struct PythonRunner {
    control: Semaphore,
}

impl PythonRunner {
    /// Create a `PythonRunner` with the provided concurrency limit.
    fn new(concurrency: usize) -> PythonRunner {
        PythonRunner {
            control: Semaphore::new(concurrency),
        }
    }

    /// Spawn a process that runs a python script in the provided environment.
    ///
    /// If the concurrency limit has been reached this method will wait until a pending
    /// script completes before spawning this one.
    ///
    /// Note: It is the caller's responsibility to create an informative span.
    async fn run_script(
        &self,
        venv: &PythonEnvironment,
        script: &str,
        source_tree: &Path,
        environment_variables: &FxHashMap<OsString, OsString>,
        modified_path: &OsString,
    ) -> Result<Output, Error> {
        let _permit = self.control.acquire().await.unwrap();

        Command::new(venv.python_executable())
            .args(["-c", script])
            .current_dir(source_tree.simplified())
            // Pass in remaining environment variables
            .envs(environment_variables)
            // Set the modified PATH
            .env("PATH", modified_path)
            // Activate the venv
            .env("VIRTUAL_ENV", venv.root())
            .env("CLICOLOR_FORCE", "1")
            .output()
            .await
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))
    }

    /// Spawn a process that runs a `setup.py` script.
    ///
    /// If the concurrency limit has been reached this method will wait until a pending
    /// script completes before spawning this one.
    ///
    /// Note: It is the caller's responsibility to create an informative span.
    async fn run_setup_py(
        &self,
        venv: &PythonEnvironment,
        script: &str,
        source_tree: &Path,
    ) -> Result<Output, Error> {
        let _permit = self.control.acquire().await.unwrap();

        Command::new(venv.python_executable())
            .args(["setup.py", script])
            .current_dir(source_tree.simplified())
            .output()
            .await
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))
    }
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
            @"This error likely indicates that you need to install the library that provides a shared library for ncurses for pygraphviz-1.11 (e.g. libncurses-dev)"
        );
    }

    #[test]
    fn missing_wheel_package() {
        let output = Output {
            status: ExitStatus::default(), // This is wrong but `from_raw` is platform-gated.
            stdout: Vec::new(),
            stderr: indoc!(
                r"
            usage: setup.py [global_opts] cmd1 [cmd1_opts] [cmd2 [cmd2_opts] ...]
               or: setup.py --help [cmd1 cmd2 ...]
               or: setup.py --help-commands
               or: setup.py cmd --help

            error: invalid command 'bdist_wheel'
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
}
