//! Build wheels from source distributions.
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

mod error;
mod pipreqs;

use std::borrow::Cow;
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

use fs_err as fs;
use indoc::formatdoc;
use itertools::Itertools;
use rustc_hash::FxHashMap;
use serde::de::{self, IntoDeserializer, SeqAccess, Visitor, value};
use serde::{Deserialize, Deserializer};
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tracing::{Instrument, debug, info_span, instrument, warn};

use uv_cache_key::cache_digest;
use uv_configuration::{BuildKind, BuildOutput, SourceStrategy};
use uv_distribution::BuildRequires;
use uv_distribution_types::{
    ConfigSettings, ExtraBuildRequirement, ExtraBuildRequires, IndexLocations, Requirement,
    Resolution,
};
use uv_fs::LockedFile;
use uv_fs::{PythonExt, Simplified};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_preview::Preview;
use uv_pypi_types::VerbatimParsedUrl;
use uv_python::{Interpreter, PythonEnvironment};
use uv_static::EnvVars;
use uv_types::{AnyErrorBuild, BuildContext, BuildIsolation, BuildStack, SourceBuildTrait};
use uv_warnings::warn_user_once;
use uv_workspace::WorkspaceCache;

pub use crate::error::{Error, MissingHeaderCause};

/// The default backend to use when PEP 517 is used without a `build-system` section.
static DEFAULT_BACKEND: LazyLock<Pep517Backend> = LazyLock::new(|| Pep517Backend {
    backend: "setuptools.build_meta:__legacy__".to_string(),
    backend_path: None,
    requirements: vec![Requirement::from(
        uv_pep508::Requirement::from_str("setuptools >= 40.8.0").unwrap(),
    )],
});

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    /// Build-related data
    build_system: Option<BuildSystem>,
    /// Project metadata
    project: Option<Project>,
    /// Tool configuration
    tool: Option<Tool>,
}

/// The `[project]` section of a pyproject.toml as specified in PEP 621.
///
/// This representation only includes a subset of the fields defined in PEP 621 necessary for
/// informing wheel builds.
#[derive(Deserialize, Debug)]
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
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct BuildSystem {
    /// PEP 508 dependencies required to execute the build system.
    requires: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
    /// A string naming a Python object that will be used to perform the build.
    build_backend: Option<String>,
    /// Specify that their backend code is hosted in-tree, this key contains a list of directories.
    backend_path: Option<BackendPath>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Tool {
    uv: Option<ToolUv>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct ToolUv {
    workspace: Option<de::IgnoredAny>,
    /// To warn users about ignored build backend settings.
    build_backend: Option<de::IgnoredAny>,
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

    fn is_setuptools(&self) -> bool {
        // either `setuptools.build_meta` or `setuptools.build_meta:__legacy__`
        self.backend.split(':').next() == Some("setuptools.build_meta")
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
        install_path: &Path,
        fallback_package_name: Option<&PackageName>,
        fallback_package_version: Option<&Version>,
        interpreter: &Interpreter,
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        version_id: Option<&str>,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
        workspace_cache: &WorkspaceCache,
        config_settings: ConfigSettings,
        build_isolation: BuildIsolation<'_>,
        extra_build_requires: &ExtraBuildRequires,
        build_stack: &BuildStack,
        build_kind: BuildKind,
        mut environment_variables: FxHashMap<OsString, OsString>,
        level: BuildOutput,
        concurrent_builds: usize,
        preview: Preview,
    ) -> Result<Self, Error> {
        let temp_dir = build_context.cache().venv_dir()?;

        let source_tree = if let Some(subdir) = subdirectory {
            source.join(subdir)
        } else {
            source.to_path_buf()
        };

        let default_backend: Pep517Backend = DEFAULT_BACKEND.clone();
        // Check if we have a PEP 517 build backend.
        let (pep517_backend, project) = Self::extract_pep517_backend(
            &source_tree,
            install_path,
            fallback_package_name,
            locations,
            source_strategy,
            workspace_cache,
            &default_backend,
        )
        .await
        .map_err(|err| *err)?;

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

        let extra_build_dependencies = package_name
            .as_ref()
            .and_then(|name| extra_build_requires.get(name).cloned())
            .unwrap_or_default()
            .into_iter()
            .map(|requirement| {
                match requirement {
                    ExtraBuildRequirement {
                        requirement,
                        match_runtime: true,
                    } if requirement.source.is_empty() => {
                        Err(Error::UnmatchedRuntime(
                            requirement.name.clone(),
                            // SAFETY: if `package_name` is `None`, the iterator is empty.
                            package_name.clone().unwrap(),
                        ))
                    }
                    requirement => Ok(requirement),
                }
            })
            .map_ok(Requirement::from)
            .collect::<Result<Vec<_>, _>>()?;

        // Create a virtual environment, or install into the shared environment if requested.
        let venv = if let Some(venv) = build_isolation.shared_environment(package_name.as_ref()) {
            venv.clone()
        } else {
            uv_virtualenv::create_venv(
                temp_dir.path(),
                interpreter.clone(),
                uv_virtualenv::Prompt::None,
                false,
                uv_virtualenv::OnExisting::Remove,
                false,
                false,
                false,
                preview,
            )?
        };

        // Set up the build environment. If build isolation is disabled, we assume the build
        // environment is already setup.
        if build_isolation.is_isolated(package_name.as_ref()) {
            debug!("Resolving build requirements");

            let dependency_sources = if extra_build_dependencies.is_empty() {
                "`build-system.requires`"
            } else {
                "`build-system.requires` and `extra-build-dependencies`"
            };

            let resolved_requirements = Self::get_resolved_requirements(
                build_context,
                source_build_context,
                &default_backend,
                &pep517_backend,
                extra_build_dependencies,
                build_stack,
            )
            .await?;

            build_context
                .install(&resolved_requirements, &venv, build_stack)
                .await
                .map_err(|err| Error::RequirementsInstall(dependency_sources, err.into()))?;
        } else {
            debug!("Proceeding without build isolation");
        }

        // Figure out what the modified path should be, and remove the PATH variable from the
        // environment variables if it's there.
        let user_path = environment_variables.remove(&OsString::from(EnvVars::PATH));

        // See if there is an OS PATH variable.
        let os_path = env::var_os(EnvVars::PATH);

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
                install_path,
                &venv,
                &pep517_backend,
                build_context,
                package_name.as_ref(),
                package_version.as_ref(),
                version_id,
                locations,
                source_strategy,
                workspace_cache,
                build_stack,
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
            version_id: version_id.map(ToString::to_string),
            environment_variables,
            modified_path,
            runner,
        })
    }

    /// Acquire a lock on the source tree, if necessary.
    async fn acquire_lock(&self) -> Result<Option<LockedFile>, Error> {
        // Depending on the command, setuptools puts `*.egg-info`, `build/`, and `dist/` in the
        // source tree, and concurrent invocations of setuptools using the same source dir can
        // stomp on each other. We need to lock something to fix that, but we don't want to dump a
        // `.lock` file into the source tree that the user will need to .gitignore. Take a global
        // proxy lock instead.
        let mut source_tree_lock = None;
        if self.pep517_backend.is_setuptools() {
            debug!("Locking the source tree for setuptools");
            let canonical_source_path = self.source_tree.canonicalize()?;
            let lock_path = env::temp_dir().join(format!(
                "uv-setuptools-{}.lock",
                cache_digest(&canonical_source_path)
            ));
            source_tree_lock = LockedFile::acquire(lock_path, self.source_tree.to_string_lossy())
                .await
                .inspect_err(|err| {
                    warn!("Failed to acquire build lock: {err}");
                })
                .ok();
        }
        Ok(source_tree_lock)
    }

    async fn get_resolved_requirements(
        build_context: &impl BuildContext,
        source_build_context: SourceBuildContext,
        default_backend: &Pep517Backend,
        pep517_backend: &Pep517Backend,
        extra_build_dependencies: Vec<Requirement>,
        build_stack: &BuildStack,
    ) -> Result<Resolution, Error> {
        Ok(
            if pep517_backend.requirements == default_backend.requirements
                && extra_build_dependencies.is_empty()
            {
                let mut resolution = source_build_context.default_resolution.lock().await;
                if let Some(resolved_requirements) = &*resolution {
                    resolved_requirements.clone()
                } else {
                    let resolved_requirements = build_context
                        .resolve(&default_backend.requirements, build_stack)
                        .await
                        .map_err(|err| {
                            Error::RequirementsResolve("`setup.py` build", err.into())
                        })?;
                    *resolution = Some(resolved_requirements.clone());
                    resolved_requirements
                }
            } else {
                let (requirements, dependency_sources) = if extra_build_dependencies.is_empty() {
                    (
                        Cow::Borrowed(&pep517_backend.requirements),
                        "`build-system.requires`",
                    )
                } else {
                    // If there are extra build dependencies, we need to resolve them together with
                    // the backend requirements.
                    let mut requirements = pep517_backend.requirements.clone();
                    requirements.extend(extra_build_dependencies);
                    (
                        Cow::Owned(requirements),
                        "`build-system.requires` and `extra-build-dependencies`",
                    )
                };
                build_context
                    .resolve(&requirements, build_stack)
                    .await
                    .map_err(|err| Error::RequirementsResolve(dependency_sources, err.into()))?
            },
        )
    }

    /// Extract the PEP 517 backend from the `pyproject.toml` or `setup.py` file.
    async fn extract_pep517_backend(
        source_tree: &Path,
        install_path: &Path,
        package_name: Option<&PackageName>,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
        workspace_cache: &WorkspaceCache,
        default_backend: &Pep517Backend,
    ) -> Result<(Pep517Backend, Option<Project>), Box<Error>> {
        let pyproject_toml = match fs::read_to_string(source_tree.join("pyproject.toml")) {
            Ok(toml) => {
                let pyproject_toml = toml_edit::Document::from_str(&toml)
                    .map_err(Error::InvalidPyprojectTomlSyntax)?;
                let pyproject_toml = PyProjectToml::deserialize(pyproject_toml.into_deserializer())
                    .map_err(Error::InvalidPyprojectTomlSchema)?;
                pyproject_toml
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
                return Ok((default_backend.clone(), None));
            }
            Err(err) => return Err(Box::new(err.into())),
        };

        if source_strategy == SourceStrategy::Enabled
            && pyproject_toml
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .map(|uv| uv.build_backend.is_some())
                .unwrap_or(false)
            && pyproject_toml
                .build_system
                .as_ref()
                .and_then(|build_backend| build_backend.build_backend.as_deref())
                != Some("uv_build")
        {
            warn_user_once!(
                "There are settings for the `uv_build` build backend defined in \
                `tool.uv.build-backend`, but the project does not use the `uv_build` backend: {}",
                source_tree.join("pyproject.toml").simplified_display()
            );
        }

        let backend = if let Some(build_system) = pyproject_toml.build_system {
            // If necessary, lower the requirements.
            let requirements = match source_strategy {
                SourceStrategy::Enabled => {
                    if let Some(name) = pyproject_toml
                        .project
                        .as_ref()
                        .map(|project| &project.name)
                        .or(package_name)
                    {
                        let build_requires = uv_pypi_types::BuildRequires {
                            name: Some(name.clone()),
                            requires_dist: build_system.requires,
                        };
                        let build_requires = BuildRequires::from_project_maybe_workspace(
                            build_requires,
                            install_path,
                            locations,
                            source_strategy,
                            workspace_cache,
                        )
                        .await
                        .map_err(Error::Lowering)?;
                        build_requires.requires_dist
                    } else {
                        build_system
                            .requires
                            .into_iter()
                            .map(Requirement::from)
                            .collect()
                    }
                }
                SourceStrategy::Disabled => build_system
                    .requires
                    .into_iter()
                    .map(Requirement::from)
                    .collect(),
            };

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
                requirements,
            }
        } else {
            // If a `pyproject.toml` is present, but `[build-system]` is missing, proceed
            // with a PEP 517 build using the default backend (`setuptools`), to match `pip`
            // and `build`.
            //
            // If there is no build system defined and there is no metadata source for
            // `setuptools`, warn. The build will succeed, but the metadata will be
            // incomplete (for example, the package name will be `UNKNOWN`).
            if pyproject_toml.project.is_none()
                && !source_tree.join("setup.py").is_file()
                && !source_tree.join("setup.cfg").is_file()
            {
                // Give a specific hint for `uv pip install .` in a workspace root.
                let looks_like_workspace_root = pyproject_toml
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|tool| tool.workspace.as_ref())
                    .is_some();
                if looks_like_workspace_root {
                    warn_user_once!(
                        "`{}` appears to be a workspace root without a Python project; \
                        consider using `uv sync` to install the workspace, or add a \
                        `[build-system]` table to `pyproject.toml`",
                        source_tree.simplified_display().cyan(),
                    );
                } else {
                    warn_user_once!(
                        "`{}` does not appear to be a Python project, as the `pyproject.toml` \
                        does not include a `[build-system]` table, and neither `setup.py` \
                        nor `setup.cfg` are present in the directory",
                        source_tree.simplified_display().cyan(),
                    );
                }
            }

            default_backend.clone()
        };
        Ok((backend, pyproject_toml.project))
    }

    /// Try calling `prepare_metadata_for_build_wheel` to get the metadata without executing the
    /// actual build.
    pub async fn get_metadata_without_build(&mut self) -> Result<Option<PathBuf>, Error> {
        // We've already called this method; return the existing result.
        if let Some(metadata_dir) = &self.metadata_directory {
            return Ok(Some(metadata_dir.clone()));
        }

        // Lock the source tree, if necessary.
        let _lock = self.acquire_lock().await?;

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
                format!(
                    "Call to `{}.prepare_metadata_for_build_{}` failed",
                    self.pep517_backend.backend, self.build_kind
                ),
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
        let filename = self.pep517_build(&wheel_dir).await?;
        Ok(filename)
    }

    /// Perform a PEP 517 build for a wheel or source distribution (sdist).
    async fn pep517_build(&self, output_dir: &Path) -> Result<String, Error> {
        // Lock the source tree, if necessary.
        let _lock = self.acquire_lock().await?;

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
                    self.pep517_backend.backend,
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
                    self.pep517_backend.backend_import(),
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
                    self.pep517_backend.backend,
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
                    self.pep517_backend.backend_import(),
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
                    "Call to `{}.build_{}` failed",
                    self.pep517_backend.backend, self.build_kind
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
                    "Call to `{}.build_{}` failed",
                    self.pep517_backend.backend, self.build_kind
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
    async fn metadata(&mut self) -> Result<Option<PathBuf>, AnyErrorBuild> {
        Ok(self.get_metadata_without_build().await?)
    }

    async fn wheel<'a>(&'a self, wheel_dir: &'a Path) -> Result<String, AnyErrorBuild> {
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
    install_path: &Path,
    venv: &PythonEnvironment,
    pep517_backend: &Pep517Backend,
    build_context: &impl BuildContext,
    package_name: Option<&PackageName>,
    package_version: Option<&Version>,
    version_id: Option<&str>,
    locations: &IndexLocations,
    source_strategy: SourceStrategy,
    workspace_cache: &WorkspaceCache,
    build_stack: &BuildStack,
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
            format!(
                "Call to `{}.build_{}` failed",
                pep517_backend.backend, build_kind
            ),
            &output,
            level,
            package_name,
            package_version,
            version_id,
        ));
    }

    // Read and deserialize the requirements from the output file.
    let read_requires_result = fs_err::read(&outfile)
        .map_err(|err| err.to_string())
        .and_then(|contents| serde_json::from_slice(&contents).map_err(|err| err.to_string()));
    let extra_requires: Vec<uv_pep508::Requirement<VerbatimParsedUrl>> = match read_requires_result
    {
        Ok(extra_requires) => extra_requires,
        Err(err) => {
            return Err(Error::from_command_output(
                format!(
                    "Call to `{}.get_requires_for_build_{}` failed: {}",
                    pep517_backend.backend, build_kind, err
                ),
                &output,
                level,
                package_name,
                package_version,
                version_id,
            ));
        }
    };

    // If necessary, lower the requirements.
    let extra_requires = match source_strategy {
        SourceStrategy::Enabled => {
            let build_requires = uv_pypi_types::BuildRequires {
                name: package_name.cloned(),
                requires_dist: extra_requires,
            };
            let build_requires = BuildRequires::from_project_maybe_workspace(
                build_requires,
                install_path,
                locations,
                source_strategy,
                workspace_cache,
            )
            .await
            .map_err(Error::Lowering)?;
            build_requires.requires_dist
        }
        SourceStrategy::Disabled => extra_requires.into_iter().map(Requirement::from).collect(),
    };

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
            .resolve(&requirements, build_stack)
            .await
            .map_err(|err| {
                Error::RequirementsResolve("`build-system.requires`", AnyErrorBuild::from(err))
            })?;

        build_context
            .install(&resolution, venv, build_stack)
            .await
            .map_err(|err| {
                Error::RequirementsInstall("`build-system.requires`", AnyErrorBuild::from(err))
            })?;
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
            mut reader: tokio::io::Split<tokio::io::BufReader<impl tokio::io::AsyncRead + Unpin>>,
            mut printer: Printer,
            buffer: &mut Vec<String>,
        ) -> io::Result<()> {
            loop {
                match reader.next_segment().await? {
                    Some(line_buf) => {
                        let line_buf = line_buf.strip_suffix(b"\r").unwrap_or(&line_buf);
                        let line = String::from_utf8_lossy(line_buf).into();
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
            .env(EnvVars::PATH, modified_path)
            .env(EnvVars::VIRTUAL_ENV, venv.root())
            // NOTE: it would be nice to get colored output from build backends,
            // but setting CLICOLOR_FORCE=1 changes the output of underlying
            // tools, which might mess with wrappers trying to parse their
            // output.
            .env(EnvVars::PYTHONIOENCODING, "utf-8:backslashreplace")
            // Remove potentially-sensitive environment variables.
            .env_remove(EnvVars::PYX_API_KEY)
            .env_remove(EnvVars::UV_API_KEY)
            .env_remove(EnvVars::PYX_AUTH_TOKEN)
            .env_remove(EnvVars::UV_AUTH_TOKEN)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))?;

        // Create buffers to capture `stdout` and `stderr`.
        let mut stdout_buf = Vec::with_capacity(1024);
        let mut stderr_buf = Vec::with_capacity(1024);

        // Create separate readers for `stdout` and `stderr`.
        let stdout_reader = tokio::io::BufReader::new(child.stdout.take().unwrap()).split(b'\n');
        let stderr_reader = tokio::io::BufReader::new(child.stderr.take().unwrap()).split(b'\n');

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
                ));
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
    /// Hide the build backend output.
    Quiet,
}

impl From<BuildOutput> for Printer {
    fn from(output: BuildOutput) -> Self {
        match output {
            BuildOutput::Stderr => Self::Stderr,
            BuildOutput::Debug => Self::Debug,
            BuildOutput::Quiet => Self::Quiet,
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
            Self::Quiet => {}
        }
        Ok(())
    }
}
