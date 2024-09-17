//! Build wheels from source distributions.
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

mod error;

use fs_err as fs;
use indoc::formatdoc;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::de::{value, IntoDeserializer, SeqAccess, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::ffi::OsString;
use std::fmt::Formatter;
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::LazyLock;
use std::{env, iter};
use tempfile::{tempdir_in, TempDir};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, info_span, instrument, Instrument};

pub use crate::error::{Error, MissingHeaderCause};
use distribution_types::Resolution;
use pep440_rs::Version;
use pep508_rs::PackageName;
use pypi_types::{Requirement, VerbatimParsedUrl};
use uv_configuration::{BuildKind, BuildOutput, ConfigSettings};
use uv_fs::{rename_with_retry, PythonExt, Simplified};
use uv_python::{Interpreter, PythonEnvironment};
use uv_types::{BuildContext, BuildIsolation, SourceBuildTrait};

/// The default backend to use when PEP 517 is used without a `build-system` section.
static DEFAULT_BACKEND: LazyLock<Pep517Backend> = LazyLock::new(|| Pep517Backend {
    backend: "setuptools.build_meta:__legacy__".to_string(),
    backend_path: None,
    requirements: vec![Requirement::from(
        pep508_rs::Requirement::from_str("setuptools >= 40.8.0").unwrap(),
    )],
});

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
}

/// Holds the state through a series of PEP 517 frontend to backend calls or a single `setup.py`
/// invocation.
///
/// This keeps both the temp dir and the result of a potential `prepare_metadata_for_build_wheel`
/// call which changes how we call `build_wheel`.
pub struct SourceBuild {
    temp_dir: TempDir,
    source_tree: PathBuf,
    config_settings: ConfigSettings,
    /// If performing a PEP 517 build, the backend to use.
    pep517_backend: Pep517Backend,
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
    /// The name of the package, if known.
    package_name: Option<PackageName>,
    /// The version of the package, if known.
    package_version: Option<Version>,
    /// Distribution identifier, e.g., `foo-1.2.3`. Used for error reporting if the name and
    /// version are unknown.
    version_id: Option<String>,
    /// Whether we do a regular PEP 517 build or an PEP 660 editable build
    build_kind: BuildKind,
    /// Whether to send build output to `stderr` or `tracing`, etc.
    level: BuildOutput,
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
        fallback_package_name: Option<&PackageName>,
        fallback_package_version: Option<&Version>,
        interpreter: &Interpreter,
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        version_id: Option<String>,
        config_settings: ConfigSettings,
        build_isolation: BuildIsolation<'_>,
        build_kind: BuildKind,
        mut environment_variables: FxHashMap<OsString, OsString>,
        level: BuildOutput,
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
            Self::extract_pep517_backend(&source_tree, &default_backend).map_err(|err| *err)?;

        let package_name = project
            .as_ref()
            .map(|project| &project.name)
            .or(fallback_package_name)
            .cloned();
        let package_version = project
            .as_ref()
            .and_then(|project| project.version.as_ref())
            .or(fallback_package_version)
            .cloned();

        // Create a virtual environment, or install into the shared environment if requested.
        let venv = if let Some(venv) = build_isolation.shared_environment(package_name.as_ref()) {
            venv.clone()
        } else {
            uv_virtualenv::create_venv(
                temp_dir.path(),
                interpreter.clone(),
                uv_virtualenv::Prompt::None,
                false,
                false,
                false,
                false,
            )?
        };

        // Set up the build environment. If build isolation is disabled, we assume the build
        // environment is already setup.
        if build_isolation.is_isolated(package_name.as_ref()) {
            debug!("Resolving build requirements");

            let resolved_requirements = Self::get_resolved_requirements(
                build_context,
                source_build_context,
                &default_backend,
                &pep517_backend,
            )
            .await?;

            build_context
                .install(&resolved_requirements, &venv)
                .await
                .map_err(|err| {
                    Error::RequirementsInstall("`build-system.requires` (install)", err)
                })?;
        } else {
            debug!("Proceeding without build isolation");
        }

        // Figure out what the modified path should be, and remove the PATH variable from the
        // environment variables if it's there.
        let user_path = environment_variables.remove(&OsString::from("PATH"));

        // See if there is an OS PATH variable.
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
        let runner = PythonRunner::new(concurrent_builds, level);
        if build_isolation.is_isolated(package_name.as_ref()) {
            debug!("Creating PEP 517 build environment");

            create_pep517_build_environment(
                &runner,
                &source_tree,
                &venv,
                &pep517_backend,
                build_context,
                package_name.as_ref(),
                package_version.as_ref(),
                version_id.as_deref(),
                build_kind,
                level,
                &config_settings,
                &environment_variables,
                &modified_path,
                &temp_dir,
            )
            .await?;
        }

        Ok(Self {
            temp_dir,
            source_tree,
            pep517_backend,
            project,
            venv,
            build_kind,
            level,
            config_settings,
            metadata_directory: None,
            package_name,
            package_version,
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
        pep517_backend: &Pep517Backend,
    ) -> Result<Resolution, Error> {
        Ok(
            if pep517_backend.requirements == default_backend.requirements {
                let mut resolution = source_build_context.default_resolution.lock().await;
                if let Some(resolved_requirements) = &*resolution {
                    resolved_requirements.clone()
                } else {
                    let resolved_requirements = build_context
                        .resolve(&default_backend.requirements)
                        .await
                        .map_err(|err| {
                            Error::RequirementsInstall("`setup.py` build (resolve)", err)
                        })?;
                    *resolution = Some(resolved_requirements.clone());
                    resolved_requirements
                }
            } else {
                build_context
                    .resolve(&pep517_backend.requirements)
                    .await
                    .map_err(|err| {
                        Error::RequirementsInstall("`build-system.requires` (resolve)", err)
                    })?
            },
        )
    }

    /// Extract the PEP 517 backend from the `pyproject.toml` or `setup.py` file.
    fn extract_pep517_backend(
        source_tree: &Path,
        default_backend: &Pep517Backend,
    ) -> Result<(Pep517Backend, Option<Project>), Box<Error>> {
        match fs::read_to_string(source_tree.join("pyproject.toml")) {
            Ok(toml) => {
                let pyproject_toml: toml_edit::ImDocument<_> =
                    toml_edit::ImDocument::from_str(&toml)
                        .map_err(Error::InvalidPyprojectTomlSyntax)?;
                let pyproject_toml: PyProjectToml =
                    PyProjectToml::deserialize(pyproject_toml.into_deserializer())
                        .map_err(Error::InvalidPyprojectTomlSchema)?;
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
                Ok((backend, pyproject_toml.project))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // We require either a `pyproject.toml` or a `setup.py` file at the top level.
                if !source_tree.join("setup.py").is_file() {
                    return Err(Box::new(Error::InvalidSourceDist(
                        source_tree.to_path_buf(),
                    )));
                }

                // If no `pyproject.toml` is present, by default, proceed with a PEP 517 build using
                // the default backend, to match `build`. `pip` uses `setup.py` directly in this
                // case,  but plans to make PEP 517 builds the default in the future.
                // See: https://github.com/pypa/pip/issues/9175.
                Ok((default_backend.clone(), None))
            }
            Err(err) => Err(Box::new(err.into())),
        }
    }

    /// Try calling `prepare_metadata_for_build_wheel` to get the metadata without executing the
    /// actual build.
    pub async fn get_metadata_without_build(&mut self) -> Result<Option<PathBuf>, Error> {
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
        if self.pep517_backend.backend == "hatchling.build" {
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
            self.pep517_backend.backend, self.build_kind,
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
            self.pep517_backend.backend_import(),
            self.build_kind,
            escape_path_for_python(&metadata_directory),
            self.config_settings.escape_for_python(),
            outfile.escape_for_python(),
        };
        let span = info_span!(
            "run_python_script",
            script = format!("prepare_metadata_for_build_{}", self.build_kind),
            version_id = self.version_id,
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
                self.level,
                self.package_name.as_ref(),
                self.package_version.as_ref(),
                self.version_id.as_deref(),
            ));
        }

        let dirname = fs::read_to_string(&outfile)?;
        if dirname.is_empty() {
            return Ok(None);
        }
        self.metadata_directory = Some(metadata_directory.join(dirname));
        Ok(self.metadata_directory.clone())
    }

    /// Build a distribution from an archive (`.zip` or `.tar.gz`) or source tree, and return the
    /// location of the built distribution.
    ///
    /// The location will be inside `temp_dir`, i.e., you must use the distribution before dropping
    /// the temporary directory.
    ///
    /// <https://packaging.python.org/en/latest/specifications/source-distribution-format/>
    #[instrument(skip_all, fields(version_id = self.version_id))]
    pub async fn build(&self, wheel_dir: &Path) -> Result<String, Error> {
        // The build scripts run with the extracted root as cwd, so they need the absolute path.
        let wheel_dir = std::path::absolute(wheel_dir)?;

        // Prevent clashes from two uv processes building distributions in parallel.
        let tmp_dir = tempdir_in(&wheel_dir)?;
        let filename = self
            .pep517_build(tmp_dir.path(), &self.pep517_backend)
            .await?;

        let from = tmp_dir.path().join(&filename);
        let to = wheel_dir.join(&filename);
        rename_with_retry(from, to).await?;
        Ok(filename)
    }

    /// Perform a PEP 517 build for a wheel or source distribution (sdist).
    async fn pep517_build(
        &self,
        output_dir: &Path,
        pep517_backend: &Pep517Backend,
    ) -> Result<String, Error> {
        // Write the hook output to a file so that we can read it back reliably.
        let outfile = self
            .temp_dir
            .path()
            .join(format!("build_{}.txt", self.build_kind));

        // Construct the appropriate build script based on the build kind.
        let script = match self.build_kind {
            BuildKind::Sdist => {
                debug!(
                    r#"Calling `{}.build_{}("{}", {})`"#,
                    pep517_backend.backend,
                    self.build_kind,
                    output_dir.escape_for_python(),
                    self.config_settings.escape_for_python(),
                );
                formatdoc! {
                    r#"
                    {}

                    sdist_filename = backend.build_{}("{}", {})
                    with open("{}", "w") as fp:
                        fp.write(sdist_filename)
                    "#,
                    pep517_backend.backend_import(),
                    self.build_kind,
                    output_dir.escape_for_python(),
                    self.config_settings.escape_for_python(),
                    outfile.escape_for_python()
                }
            }
            BuildKind::Wheel | BuildKind::Editable => {
                let metadata_directory = self
                    .metadata_directory
                    .as_deref()
                    .map_or("None".to_string(), |path| {
                        format!(r#""{}""#, path.escape_for_python())
                    });
                debug!(
                    r#"Calling `{}.build_{}("{}", {}, {})`"#,
                    pep517_backend.backend,
                    self.build_kind,
                    output_dir.escape_for_python(),
                    self.config_settings.escape_for_python(),
                    metadata_directory,
                );
                formatdoc! {
                    r#"
                    {}

                    wheel_filename = backend.build_{}("{}", {}, {})
                    with open("{}", "w") as fp:
                        fp.write(wheel_filename)
                    "#,
                    pep517_backend.backend_import(),
                    self.build_kind,
                    output_dir.escape_for_python(),
                    self.config_settings.escape_for_python(),
                    metadata_directory,
                    outfile.escape_for_python()
                }
            }
        };

        let span = info_span!(
            "run_python_script",
            script = format!("build_{}", self.build_kind),
            version_id = self.version_id,
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
                    "Build backend failed to build {} through `build_{}()`",
                    self.build_kind, self.build_kind,
                ),
                &output,
                self.level,
                self.package_name.as_ref(),
                self.package_version.as_ref(),
                self.version_id.as_deref(),
            ));
        }

        let distribution_filename = fs::read_to_string(&outfile)?;
        if !output_dir.join(&distribution_filename).is_file() {
            return Err(Error::from_command_output(
                format!(
                    "Build backend failed to produce {} through `build_{}()`: `{distribution_filename}` not found",
                    self.build_kind, self.build_kind,
                ),
                &output,
                self.level,
                self.package_name.as_ref(),
                self.package_version.as_ref(),
                self.version_id.as_deref(),
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
        Ok(self.build(wheel_dir).await?)
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
    package_name: Option<&PackageName>,
    package_version: Option<&Version>,
    version_id: Option<&str>,
    build_kind: BuildKind,
    level: BuildOutput,
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
        script = format!("get_requires_for_build_{}", build_kind),
        version_id = version_id,
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
            level,
            package_name,
            package_version,
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
            level,
            package_name,
            package_version,
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
            level,
            package_name,
            package_version,
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
            .map_err(|err| Error::RequirementsInstall("`build-system.requires` (resolve)", err))?;

        build_context
            .install(&resolution, venv)
            .await
            .map_err(|err| Error::RequirementsInstall("`build-system.requires` (install)", err))?;
    }

    Ok(())
}

/// A runner that manages the execution of external python processes with a
/// concurrency limit.
#[derive(Debug)]
struct PythonRunner {
    control: Semaphore,
    level: BuildOutput,
}

#[derive(Debug)]
struct PythonRunnerOutput {
    stdout: Vec<String>,
    stderr: Vec<String>,
    status: ExitStatus,
}

impl PythonRunner {
    /// Create a `PythonRunner` with the provided concurrency limit and output level.
    fn new(concurrency: usize, level: BuildOutput) -> Self {
        Self {
            control: Semaphore::new(concurrency),
            level,
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
    ) -> Result<PythonRunnerOutput, Error> {
        /// Read lines from a reader and store them in a buffer.
        async fn read_from(
            mut reader: tokio::io::Lines<tokio::io::BufReader<impl tokio::io::AsyncRead + Unpin>>,
            mut printer: Printer,
            buffer: &mut Vec<String>,
        ) -> io::Result<()> {
            loop {
                match reader.next_line().await? {
                    Some(line) => {
                        let _ = write!(printer, "{line}");
                        buffer.push(line);
                    }
                    None => return Ok(()),
                }
            }
        }

        let _permit = self.control.acquire().await.unwrap();

        let mut child = Command::new(venv.python_executable())
            .args(["-c", script])
            .current_dir(source_tree.simplified())
            .envs(environment_variables)
            .env("PATH", modified_path)
            .env("VIRTUAL_ENV", venv.root())
            .env("CLICOLOR_FORCE", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))?;

        // Create buffers to capture `stdout` and `stderr`.
        let mut stdout_buf = Vec::with_capacity(1024);
        let mut stderr_buf = Vec::with_capacity(1024);

        // Create separate readers for `stdout` and `stderr`.
        let stdout_reader = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
        let stderr_reader = tokio::io::BufReader::new(child.stderr.take().unwrap()).lines();

        // Asynchronously read from the in-memory pipes.
        let printer = Printer::from(self.level);
        let result = tokio::join!(
            read_from(stdout_reader, printer, &mut stdout_buf),
            read_from(stderr_reader, printer, &mut stderr_buf),
        );
        match result {
            (Ok(()), Ok(())) => {}
            (Err(err), _) | (_, Err(err)) => {
                return Err(Error::CommandFailed(
                    venv.python_executable().to_path_buf(),
                    err,
                ))
            }
        }

        // Wait for the child process to finish.
        let status = child
            .wait()
            .await
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))?;

        Ok(PythonRunnerOutput {
            stdout: stdout_buf,
            stderr: stderr_buf,
            status,
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Printer {
    /// Send the build backend output to `stderr`.
    Stderr,
    /// Send the build backend output to `tracing`.
    Debug,
}

impl From<BuildOutput> for Printer {
    fn from(output: BuildOutput) -> Self {
        match output {
            BuildOutput::Stderr => Self::Stderr,
            BuildOutput::Debug => Self::Debug,
        }
    }
}

impl Write for Printer {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self {
            Self::Stderr => {
                anstream::eprintln!("{s}");
            }
            Self::Debug => {
                debug!("{s}");
            }
        }
        Ok(())
    }
}
