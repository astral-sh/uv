use std::fmt::Write;
use std::path::PathBuf;

use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{Resolution, UnresolvedRequirement, UnresolvedRequirementSpecification};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::MarkerTreeContents;
use pypi_types::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, Constraints, ExtrasSpecification, Reinstall, Upgrade};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVersionFile, VersionRequest,
};
use uv_requirements::{
    NamedRequirementsError, NamedRequirementsResolver, RequirementsSpecification,
};
use uv_resolver::{
    FlatIndex, OptionsBuilder, PythonRequirement, RequiresPython, ResolutionGraph, ResolverMarkers,
};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::Workspace;

use crate::commands::pip::loggers::{InstallLogger, ResolveLogger};
use crate::commands::pip::operations::{Changelog, Modifications};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{pip, SharedState};
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverInstallerSettings, ResolverSettingsRef};

pub(crate) mod add;
pub(crate) mod environment;
pub(crate) mod export;
pub(crate) mod init;
pub(crate) mod lock;
pub(crate) mod remove;
pub(crate) mod run;
pub(crate) mod sync;
pub(crate) mod tree;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ProjectError {
    #[error("The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.")]
    LockMismatch,

    #[error(
        "Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`."
    )]
    MissingLockfile,

    #[error("The current Python version ({0}) is not compatible with the locked Python requirement: `{1}`")]
    LockedPythonIncompatibility(Version, RequiresPython),

    #[error("The current Python platform is not compatible with the lockfile's supported environments: {0}")]
    LockedPlatformIncompatibility(String),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the project's Python requirement: `{1}`")]
    RequestedPythonProjectIncompatibility(Version, RequiresPython),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the project's Python requirement: `{2}`")]
    DotPythonVersionProjectIncompatibility(String, Version, RequiresPython),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the project's Python requirement: `{1}`")]
    RequiresPythonProjectIncompatibility(Version, RequiresPython),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the script's Python requirement: `{1}`")]
    RequestedPythonScriptIncompatibility(Version, VersionSpecifiers),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the script's Python requirement: `{2}`")]
    DotPythonVersionScriptIncompatibility(String, Version, VersionSpecifiers),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the script's Python requirement: `{1}`")]
    RequiresPythonScriptIncompatibility(Version, VersionSpecifiers),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the project's Python requirement: `{1}`. However, a workspace member (`{member}`) supports Python {3}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _2.cyan(), venv = format!("uv venv --python {_0}").green(), install = "uv pip install -e .".green(), path = _4.user_display().cyan() )]
    RequestedMemberIncompatibility(
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the project's Python requirement: `{2}`. However, a workspace member (`{member}`) supports Python {4}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _3.cyan(), venv = format!("uv venv --python {_1}").green(), install = "uv pip install -e .".green(), path = _5.user_display().cyan() )]
    DotPythonVersionMemberIncompatibility(
        String,
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the project's Python requirement: `{1}`. However, a workspace member (`{member}`) supports Python {3}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _2.cyan(), venv = format!("uv venv --python {_0}").green(), install = "uv pip install -e .".green(), path = _4.user_display().cyan() )]
    RequiresPythonMemberIncompatibility(
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("Supported environments must be disjoint, but the following markers overlap: `{0}` and `{1}`.\n\n{hint}{colon} replace `{1}` with `{2}`.", hint = "hint".bold().cyan(), colon = ":".bold())]
    OverlappingMarkers(String, String, String),

    #[error("Environment markers `{0}` don't overlap with Python requirement `{1}`")]
    DisjointEnvironment(MarkerTreeContents, VersionSpecifiers),

    #[error("Environment marker is empty")]
    EmptyEnvironment,

    #[error("Failed to parse `pyproject.toml`")]
    TomlParse(#[source] toml::de::Error),

    #[error("Failed to update `pyproject.toml`")]
    TomlUpdate,

    #[error(transparent)]
    Python(#[from] uv_python::Error),

    #[error(transparent)]
    Virtualenv(#[from] uv_virtualenv::Error),

    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Tags(#[from] platform_tags::TagsError),

    #[error(transparent)]
    FlatIndex(#[from] uv_client::FlatIndexError),

    #[error(transparent)]
    Lock(#[from] uv_resolver::LockError),

    #[error(transparent)]
    Operation(#[from] pip::operations::Error),

    #[error(transparent)]
    RequiresPython(#[from] uv_resolver::RequiresPythonError),

    #[error(transparent)]
    Interpreter(#[from] uv_python::InterpreterError),

    #[error(transparent)]
    Tool(#[from] uv_tool::Error),

    #[error(transparent)]
    Name(#[from] uv_normalize::InvalidNameError),

    #[error(transparent)]
    NamedRequirements(#[from] uv_requirements::NamedRequirementsError),

    #[error(transparent)]
    PyprojectMut(#[from] uv_workspace::pyproject_mut::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

/// Compute the `Requires-Python` bound for the [`Workspace`].
///
/// For a [`Workspace`] with multiple packages, the `Requires-Python` bound is the union of the
/// `Requires-Python` bounds of all the packages.
pub(crate) fn find_requires_python(
    workspace: &Workspace,
) -> Result<Option<RequiresPython>, uv_resolver::RequiresPythonError> {
    RequiresPython::intersection(workspace.packages().values().filter_map(|member| {
        member
            .pyproject_toml()
            .project
            .as_ref()
            .and_then(|project| project.requires_python.as_ref())
    }))
}

/// Returns an error if the [`Interpreter`] does not satisfy the [`Workspace`] `requires-python`.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_requires_python(
    interpreter: &Interpreter,
    workspace: &Workspace,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), ProjectError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }

    // If the Python version is compatible with one of the workspace _members_, raise
    // a dedicated error. For example, if the workspace root requires Python >=3.12, but
    // a library in the workspace is compatible with Python >=3.8, the user may attempt
    // to sync on Python 3.8. This will fail, but we should provide a more helpful error
    // message.
    for (name, member) in workspace.packages() {
        let Some(project) = member.pyproject_toml().project.as_ref() else {
            continue;
        };
        let Some(specifiers) = project.requires_python.as_ref() else {
            continue;
        };
        if specifiers.contains(interpreter.python_version()) {
            return match source {
                PythonRequestSource::UserRequest => {
                    Err(ProjectError::RequestedMemberIncompatibility(
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
                PythonRequestSource::DotPythonVersion(file) => {
                    Err(ProjectError::DotPythonVersionMemberIncompatibility(
                        file.to_string(),
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
                PythonRequestSource::RequiresPython => {
                    Err(ProjectError::RequiresPythonMemberIncompatibility(
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
            };
        }
    }

    match source {
        PythonRequestSource::UserRequest => {
            Err(ProjectError::RequestedPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(ProjectError::DotPythonVersionProjectIncompatibility(
                file.to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(ProjectError::RequiresPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
    }
}

/// Find the virtual environment for the current project.
fn find_environment(
    workspace: &Workspace,
    cache: &Cache,
) -> Result<PythonEnvironment, uv_python::Error> {
    PythonEnvironment::from_root(workspace.venv(), cache)
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum FoundInterpreter {
    Interpreter(Interpreter),
    Environment(PythonEnvironment),
}

#[derive(Debug, Clone)]
pub(crate) enum PythonRequestSource {
    /// The request was provided by the user.
    UserRequest,
    /// The request was inferred from a `.python-version` or `.python-versions` file.
    DotPythonVersion(String),
    /// The request was inferred from a `pyproject.toml` file.
    RequiresPython,
}

/// The resolved Python request and requirement for a [`Workspace`].
#[derive(Debug, Clone)]
pub(crate) struct WorkspacePython {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, and (3) any
    /// `Requires-Python` specifier in the `pyproject.toml`.
    python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the project, computed by taking the intersection of all
    /// `Requires-Python` specifiers in the workspace.
    requires_python: Option<RequiresPython>,
}

impl WorkspacePython {
    /// Determine the [`WorkspacePython`] for the current [`Workspace`].
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: &Workspace,
    ) -> Result<Self, ProjectError> {
        let requires_python = find_requires_python(workspace)?;

        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            let source = PythonRequestSource::UserRequest;
            let request = Some(request);
            (source, request)
        } else if let Some(file) =
            PythonVersionFile::discover(workspace.install_path(), false, false).await?
        {
            // (2) Request from `.python-version`
            let source = PythonRequestSource::DotPythonVersion(file.file_name().to_string());
            let request = file.into_version();
            (source, request)
        } else {
            // (3) `Requires-Python` in `pyproject.toml`
            let request = requires_python
                .as_ref()
                .map(RequiresPython::specifiers)
                .map(|specifiers| {
                    PythonRequest::Version(VersionRequest::Range(specifiers.clone()))
                });
            let source = PythonRequestSource::RequiresPython;
            (source, request)
        };

        Ok(Self {
            source,
            python_request,
            requires_python,
        })
    }
}

impl FoundInterpreter {
    /// Discover the interpreter to use in the current [`Workspace`].
    pub(crate) async fn discover(
        workspace: &Workspace,
        python_request: Option<PythonRequest>,
        python_preference: PythonPreference,
        python_downloads: PythonDownloads,
        connectivity: Connectivity,
        native_tls: bool,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        // Resolve the Python request and requirement for the workspace.
        let WorkspacePython {
            source,
            python_request,
            requires_python,
        } = WorkspacePython::from_request(python_request, workspace).await?;

        // Read from the virtual environment first.
        match find_environment(workspace, cache) {
            Ok(venv) => {
                if python_request.as_ref().map_or(true, |request| {
                    if request.satisfied(venv.interpreter(), cache) {
                        debug!("The virtual environment's Python version satisfies `{request}`");
                        true
                    } else {
                        debug!(
                            "The virtual environment's Python version does not satisfy `{request}`"
                        );
                        false
                    }
                }) {
                    if let Some(requires_python) = requires_python.as_ref() {
                        if requires_python.contains(venv.interpreter().python_version()) {
                            return Ok(Self::Environment(venv));
                        }
                        debug!(
                            "The virtual environment's Python version does not meet the project's Python requirement: `{requires_python}`"
                        );
                    } else {
                        return Ok(Self::Environment(venv));
                    }
                }
            }
            Err(uv_python::Error::MissingEnvironment(_)) => {}
            Err(uv_python::Error::Query(uv_python::InterpreterError::NotFound(path))) => {
                warn_user!(
                    "Ignoring existing virtual environment linked to non-existent Python interpreter: {}",
                    path.user_display().cyan()
                );
            }
            Err(err) => return Err(err.into()),
        };

        let client_builder = BaseClientBuilder::default()
            .connectivity(connectivity)
            .native_tls(native_tls);

        let reporter = PythonDownloadReporter::single(printer);

        // Locate the Python interpreter to use in the environment
        let python = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
        )
        .await?;

        let managed = python.source().is_managed();
        let interpreter = python.into_interpreter();

        if managed {
            writeln!(
                printer.stderr(),
                "Using Python {}",
                interpreter.python_version().cyan()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "Using Python {} interpreter at: {}",
                interpreter.python_version(),
                interpreter.sys_executable().user_display().cyan()
            )?;
        }

        if let Some(requires_python) = requires_python.as_ref() {
            validate_requires_python(&interpreter, workspace, requires_python, &source)?;
        }

        Ok(Self::Interpreter(interpreter))
    }

    /// Convert the [`FoundInterpreter`] into an [`Interpreter`].
    pub(crate) fn into_interpreter(self) -> Interpreter {
        match self {
            FoundInterpreter::Interpreter(interpreter) => interpreter,
            FoundInterpreter::Environment(venv) => venv.into_interpreter(),
        }
    }
}

/// Initialize a virtual environment for the current project.
pub(crate) async fn get_or_init_environment(
    workspace: &Workspace,
    python: Option<PythonRequest>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment, ProjectError> {
    match FoundInterpreter::discover(
        workspace,
        python,
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?
    {
        // If we found an existing, compatible environment, use it.
        FoundInterpreter::Environment(environment) => Ok(environment),

        // Otherwise, create a virtual environment with the discovered interpreter.
        FoundInterpreter::Interpreter(interpreter) => {
            let venv = workspace.venv();

            // Remove the existing virtual environment if it doesn't meet the requirements.
            match fs_err::remove_dir_all(&venv) {
                Ok(()) => {
                    writeln!(
                        printer.stderr(),
                        "Removed virtual environment at: {}",
                        venv.user_display().cyan()
                    )?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }

            writeln!(
                printer.stderr(),
                "Creating virtualenv at: {}",
                venv.user_display().cyan()
            )?;

            // Determine a prompt for the environment, in order of preference:
            //
            // 1) The name of the project
            // 2) The name of the directory at the root of the workspace
            // 3) No prompt
            let prompt = workspace
                .pyproject_toml()
                .project
                .as_ref()
                .map(|p| p.name.to_string())
                .or_else(|| {
                    workspace
                        .install_path()
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                })
                .map(uv_virtualenv::Prompt::Static)
                .unwrap_or(uv_virtualenv::Prompt::None);

            Ok(uv_virtualenv::create_venv(
                &venv,
                interpreter,
                prompt,
                false,
                false,
                false,
                false,
            )?)
        }
    }
}

/// Resolve any [`UnresolvedRequirementSpecification`] into a fully-qualified [`Requirement`].
pub(crate) async fn resolve_names(
    requirements: Vec<UnresolvedRequirementSpecification>,
    interpreter: &Interpreter,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<Vec<Requirement>, NamedRequirementsError> {
    // Partition the requirements into named and unnamed requirements.
    let (mut requirements, unnamed): (Vec<_>, Vec<_>) =
        requirements
            .into_iter()
            .partition_map(|spec| match spec.requirement {
                UnresolvedRequirement::Named(requirement) => itertools::Either::Left(requirement),
                UnresolvedRequirement::Unnamed(requirement) => {
                    itertools::Either::Right(requirement)
                }
            });

    // Short-circuit if there are no unnamed requirements.
    if unnamed.is_empty() {
        return Ok(requirements);
    }

    // Extract the project settings.
    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        resolution: _,
        prerelease: _,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode: _,
        sources,
        upgrade: _,
        reinstall: _,
        build_options,
    } = settings;

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
        .allow_insecure_host(allow_insecure_host.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if *no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package)
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let hasher = HashStrategy::default();
    let flat_index = FlatIndex::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        *index_strategy,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        &build_hasher,
        *exclude_newer,
        *sources,
        concurrency,
    );

    // Resolve the unnamed requirements.
    requirements.extend(
        NamedRequirementsResolver::new(
            unnamed,
            &hasher,
            &state.index,
            DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?,
    );

    Ok(requirements)
}

/// Run dependency resolution for an interpreter, returning the [`ResolutionGraph`].
pub(crate) async fn resolve_environment<'a>(
    interpreter: &Interpreter,
    spec: RequirementsSpecification,
    settings: ResolverSettingsRef<'_>,
    state: &SharedState,
    logger: Box<dyn ResolveLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ResolutionGraph, ProjectError> {
    warn_on_requirements_txt_setting(&spec, settings);

    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        resolution,
        prerelease,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        upgrade: _,
        build_options,
        sources,
    } = settings;

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        source_trees,
        ..
    } = spec;

    // Determine the tags, markers, and interpreter to use for resolution.
    let tags = interpreter.tags()?;
    let markers = interpreter.resolver_markers();
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package)
    };

    let options = OptionsBuilder::new()
        .resolution_mode(resolution)
        .prerelease_mode(prerelease)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let dev = Vec::default();
    let extras = ExtrasSpecification::default();
    let hasher = HashStrategy::default();
    let preferences = Vec::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();

    // When resolving from an interpreter, we assume an empty environment, so reinstalls and
    // upgrades aren't relevant.
    let reinstall = Reinstall::default();
    let upgrade = Upgrade::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let resolve_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer,
        sources,
        concurrency,
    );

    // Resolve the requirements.
    Ok(pip::operations::resolve(
        requirements,
        constraints,
        overrides,
        dev,
        source_trees,
        project,
        None,
        &extras,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &reinstall,
        &upgrade,
        Some(tags),
        ResolverMarkers::specific_environment(markers),
        python_requirement,
        &client,
        &flat_index,
        &state.index,
        &resolve_dispatch,
        concurrency,
        options,
        logger,
        printer,
    )
    .await?)
}

/// Sync a [`PythonEnvironment`] with a set of resolved requirements.
pub(crate) async fn sync_environment(
    venv: PythonEnvironment,
    resolution: &Resolution,
    settings: InstallerSettingsRef<'_>,
    state: &SharedState,
    logger: Box<dyn InstallLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<PythonEnvironment> {
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
        sources,
    } = settings;

    let site_packages = SitePackages::from_environment(&venv)?;

    // Determine the markers tags to use for resolution.
    let interpreter = venv.interpreter();
    let tags = venv.interpreter().tags()?;
    let markers = interpreter.resolver_markers();

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(&venv, no_build_isolation_package)
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let dry_run = false;
    let hasher = HashStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer,
        sources,
        concurrency,
    );

    // Sync the environment.
    pip::operations::install(
        resolution,
        site_packages,
        Modifications::Exact,
        reinstall,
        build_options,
        link_mode,
        compile_bytecode,
        index_locations,
        config_setting,
        &hasher,
        &markers,
        tags,
        &client,
        &state.in_flight,
        concurrency,
        &build_dispatch,
        cache,
        &venv,
        logger,
        dry_run,
        printer,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(venv)
}

/// The result of updating a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
#[derive(Debug)]
pub(crate) struct EnvironmentUpdate {
    /// The updated [`PythonEnvironment`].
    pub(crate) environment: PythonEnvironment,
    /// The [`Changelog`] of changes made to the environment.
    pub(crate) changelog: Changelog,
}

impl EnvironmentUpdate {
    /// Convert the [`EnvironmentUpdate`] into a [`PythonEnvironment`].
    pub(crate) fn into_environment(self) -> PythonEnvironment {
        self.environment
    }
}

/// Update a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
pub(crate) async fn update_environment(
    venv: PythonEnvironment,
    spec: RequirementsSpecification,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    resolve: Box<dyn ResolveLogger>,
    install: Box<dyn InstallLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<EnvironmentUpdate> {
    warn_on_requirements_txt_setting(&spec, settings.as_ref().into());

    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        allow_insecure_host,
        resolution,
        prerelease,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode,
        sources,
        upgrade,
        reinstall,
        build_options,
    } = settings;

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        source_trees,
        ..
    } = spec;

    // Determine markers to use for resolution.
    let interpreter = venv.interpreter();
    let markers = venv.interpreter().resolver_markers();

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_environment(&venv)?;
    if source_trees.is_empty() && reinstall.is_none() && upgrade.is_none() && overrides.is_empty() {
        match site_packages.satisfies(&requirements, &constraints, &markers)? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                debug!(
                    "All requirements satisfied: {}",
                    recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                        .join(" | ")
                );
                return Ok(EnvironmentUpdate {
                    environment: venv,
                    changelog: Changelog::default(),
                });
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
        .allow_insecure_host(allow_insecure_host.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let build_isolation = if *no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(&venv, no_build_isolation_package)
    };

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease_mode(*prerelease)
        .exclude_newer(*exclude_newer)
        .index_strategy(*index_strategy)
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let dev = Vec::default();
    let dry_run = false;
    let extras = ExtrasSpecification::default();
    let hasher = HashStrategy::default();
    let preferences = Vec::default();

    // Determine the tags to use for resolution.
    let tags = venv.interpreter().tags()?;
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        &state.index,
        &state.git,
        &state.capabilities,
        &state.in_flight,
        *index_strategy,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        &build_hasher,
        *exclude_newer,
        *sources,
        concurrency,
    );

    // Resolve the requirements.
    let resolution = match pip::operations::resolve(
        requirements,
        constraints,
        overrides,
        dev,
        source_trees,
        project,
        None,
        &extras,
        preferences,
        site_packages.clone(),
        &hasher,
        reinstall,
        upgrade,
        Some(tags),
        ResolverMarkers::specific_environment(markers.clone()),
        python_requirement,
        &client,
        &flat_index,
        &state.index,
        &build_dispatch,
        concurrency,
        options,
        resolve,
        printer,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(err) => return Err(err.into()),
    };

    // Sync the environment.
    let changelog = pip::operations::install(
        &resolution,
        site_packages,
        Modifications::Exact,
        reinstall,
        build_options,
        *link_mode,
        *compile_bytecode,
        index_locations,
        config_setting,
        &hasher,
        &markers,
        tags,
        &client,
        &state.in_flight,
        concurrency,
        &build_dispatch,
        cache,
        &venv,
        install,
        dry_run,
        printer,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(EnvironmentUpdate {
        environment: venv,
        changelog,
    })
}

/// Warn if the user provides (e.g.) an `--index-url` in a requirements file.
fn warn_on_requirements_txt_setting(
    spec: &RequirementsSpecification,
    settings: ResolverSettingsRef<'_>,
) {
    let RequirementsSpecification {
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        no_binary,
        no_build,
        ..
    } = spec;

    if settings.index_locations.no_index() {
        // Nothing to do, we're ignoring the URLs anyway.
    } else if *no_index {
        warn_user_once!("Ignoring `--no-index` from requirements file. Instead, use the `--no-index` command-line argument, or set `no-index` in a `uv.toml` or `pyproject.toml` file.");
    } else {
        if let Some(index_url) = index_url {
            if settings.index_locations.index() != Some(index_url) {
                warn_user_once!(
                    "Ignoring `--index-url` from requirements file: `{index_url}`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file."
                );
            }
        }
        for extra_index_url in extra_index_urls {
            if !settings
                .index_locations
                .extra_index()
                .contains(extra_index_url)
            {
                warn_user_once!(
                    "Ignoring `--extra-index-url` from requirements file: `{extra_index_url}`. Instead, use the `--extra-index-url` command-line argument, or set `extra-index-url` in a `uv.toml` or `pyproject.toml` file.`"

                );
            }
        }
        for find_link in find_links {
            if !settings.index_locations.flat_index().contains(find_link) {
                warn_user_once!(
                    "Ignoring `--find-links` from requirements file: `{find_link}`. Instead, use the `--find-links` command-line argument, or set `find-links` in a `uv.toml` or `pyproject.toml` file.`"
                );
            }
        }
    }

    if !no_binary.is_none() && settings.build_options.no_binary() != no_binary {
        warn_user_once!("Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-binary` command-line argument, or set `no-binary` in a `uv.toml` or `pyproject.toml` file.");
    }

    if !no_build.is_none() && settings.build_options.no_build() != no_build {
        warn_user_once!("Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-build` command-line argument, or set `no-build` in a `uv.toml` or `pyproject.toml` file.");
    }
}
