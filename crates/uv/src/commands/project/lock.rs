#![allow(clippy::single_match_else)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use uv_auth::UrlAuthPolicies;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DryRun, ExtrasSpecification, PreviewMode, Reinstall, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    DependencyMetadata, HashGeneration, Index, IndexLocations, NameRequirementSpecification,
    UnresolvedRequirementSpecification,
};
use uv_git::ResolvedRepositoryReference;
use uv_normalize::{GroupName, PackageName};
use uv_pep440::Version;
use uv_pypi_types::{Conflicts, Requirement, SupportedEnvironments};
use uv_python::{Interpreter, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_requirements::ExtrasResolver;
use uv_resolver::{
    FlatIndex, InMemoryIndex, Lock, Options, OptionsBuilder, PythonRequirement, RequiresPython,
    ResolverEnvironment, ResolverManifest, SatisfiesResult, UniversalMarker,
};
use uv_scripts::{Pep723ItemRef, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache, WorkspaceMember};

use crate::commands::pip::loggers::{DefaultResolveLogger, ResolveLogger, SummaryResolveLogger};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    init_script_python_requirement, ProjectError, ProjectInterpreter, ScriptInterpreter,
    UniversalState,
};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{diagnostics, pip, ExitStatus, ScriptPath};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverSettings};

/// The result of running a lock operation.
#[derive(Debug, Clone)]
pub(crate) enum LockResult {
    /// The lock was unchanged.
    Unchanged(Lock),
    /// The lock was changed.
    Changed(Option<Lock>, Lock),
}

impl LockResult {
    pub(crate) fn lock(&self) -> &Lock {
        match self {
            LockResult::Unchanged(lock) => lock,
            LockResult::Changed(_, lock) => lock,
        }
    }

    pub(crate) fn into_lock(self) -> Lock {
        match self {
            LockResult::Unchanged(lock) => lock,
            LockResult::Changed(_, lock) => lock,
        }
    }
}

/// Resolve the project requirements into a lockfile.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn lock(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    dry_run: DryRun,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    network_settings: NetworkSettings,
    script: Option<ScriptPath>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> anyhow::Result<ExitStatus> {
    // If necessary, initialize the PEP 723 script.
    let script = match script {
        Some(ScriptPath::Path(path)) => {
            let client_builder = BaseClientBuilder::new()
                .connectivity(network_settings.connectivity)
                .native_tls(network_settings.native_tls)
                .allow_insecure_host(network_settings.allow_insecure_host.clone());
            let reporter = PythonDownloadReporter::single(printer);
            let requires_python = init_script_python_requirement(
                python.as_deref(),
                &install_mirrors,
                project_dir,
                false,
                python_preference,
                python_downloads,
                no_config,
                &client_builder,
                cache,
                &reporter,
            )
            .await?;
            Some(Pep723Script::init(&path, requires_python.specifiers()).await?)
        }
        Some(ScriptPath::Script(script)) => Some(script),
        None => None,
    };

    // Find the project requirements.
    let workspace_cache = WorkspaceCache::default();
    let workspace;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        workspace =
            Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
                .await?;
        LockTarget::Workspace(&workspace)
    };

    // Determine the lock mode.
    let interpreter;
    let mode = if frozen {
        LockMode::Frozen
    } else {
        interpreter = match target {
            LockTarget::Workspace(workspace) => ProjectInterpreter::discover(
                workspace,
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                &network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            LockTarget::Script(script) => ScriptInterpreter::discover(
                Pep723ItemRef::Script(script),
                python.as_deref().map(PythonRequest::parse),
                &network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        };

        if locked {
            LockMode::Locked(&interpreter)
        } else if dry_run.enabled() {
            LockMode::DryRun(&interpreter)
        } else {
            LockMode::Write(&interpreter)
        }
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Perform the lock operation.
    match LockOperation::new(
        mode,
        &settings,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute(target)
    .await
    {
        Ok(lock) => {
            if dry_run.enabled() {
                // In `--dry-run` mode, show all changes.
                let mut changed = false;
                if let LockResult::Changed(previous, lock) = &lock {
                    for event in LockEvent::detect_changes(previous.as_ref(), lock, dry_run) {
                        changed = true;
                        writeln!(printer.stderr(), "{event}")?;
                    }
                }
                if !changed {
                    writeln!(
                        printer.stderr(),
                        "{}",
                        "No lockfile changes detected".bold()
                    )?;
                }
            } else {
                if let LockResult::Changed(Some(previous), lock) = &lock {
                    for event in LockEvent::detect_changes(Some(previous), lock, dry_run) {
                        writeln!(printer.stderr(), "{event}")?;
                    }
                }
            }

            Ok(ExitStatus::Success)
        }
        Err(ProjectError::Operation(err)) => {
            diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => Err(err.into()),
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum LockMode<'env> {
    /// Write the lockfile to disk.
    Write(&'env Interpreter),
    /// Perform a resolution, but don't write the lockfile to disk.
    DryRun(&'env Interpreter),
    /// Error if the lockfile is not up-to-date with the project requirements.
    Locked(&'env Interpreter),
    /// Use the existing lockfile without performing a resolution.
    Frozen,
}

/// A lock operation.
pub(super) struct LockOperation<'env> {
    mode: LockMode<'env>,
    constraints: Vec<NameRequirementSpecification>,
    settings: &'env ResolverSettings,
    network_settings: &'env NetworkSettings,
    state: &'env UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: Concurrency,
    cache: &'env Cache,
    printer: Printer,
    preview: PreviewMode,
}

impl<'env> LockOperation<'env> {
    /// Initialize a [`LockOperation`].
    pub(super) fn new(
        mode: LockMode<'env>,
        settings: &'env ResolverSettings,
        network_settings: &'env NetworkSettings,
        state: &'env UniversalState,
        logger: Box<dyn ResolveLogger>,
        concurrency: Concurrency,
        cache: &'env Cache,
        printer: Printer,
        preview: PreviewMode,
    ) -> Self {
        Self {
            mode,
            constraints: vec![],
            settings,
            network_settings,
            state,
            logger,
            concurrency,
            cache,
            printer,
            preview,
        }
    }

    /// Set the external constraints for the [`LockOperation`].
    #[must_use]
    pub(super) fn with_constraints(
        mut self,
        constraints: Vec<NameRequirementSpecification>,
    ) -> Self {
        self.constraints = constraints;
        self
    }

    /// Perform a [`LockOperation`].
    pub(super) async fn execute(self, target: LockTarget<'_>) -> Result<LockResult, ProjectError> {
        match self.mode {
            LockMode::Frozen => {
                // Read the existing lockfile, but don't attempt to lock the project.
                let existing = target
                    .read()
                    .await?
                    .ok_or_else(|| ProjectError::MissingLockfile)?;
                Ok(LockResult::Unchanged(existing))
            }
            LockMode::Locked(interpreter) => {
                // Read the existing lockfile.
                let existing = target
                    .read()
                    .await?
                    .ok_or_else(|| ProjectError::MissingLockfile)?;

                // Perform the lock operation, but don't write the lockfile to disk.
                let result = do_lock(
                    target,
                    interpreter,
                    Some(existing),
                    self.constraints,
                    self.settings,
                    self.network_settings,
                    self.state,
                    self.logger,
                    self.concurrency,
                    self.cache,
                    self.printer,
                    self.preview,
                )
                .await?;

                // If the lockfile changed, return an error.
                if matches!(result, LockResult::Changed(_, _)) {
                    return Err(ProjectError::LockMismatch);
                }

                Ok(result)
            }
            LockMode::Write(interpreter) | LockMode::DryRun(interpreter) => {
                // Read the existing lockfile.
                let existing = match target.read().await {
                    Ok(Some(existing)) => Some(existing),
                    Ok(None) => None,
                    Err(ProjectError::Lock(err)) => {
                        warn_user!(
                            "Failed to read existing lockfile; ignoring locked requirements: {err}"
                        );
                        None
                    }
                    Err(err) => return Err(err),
                };

                // Perform the lock operation.
                let result = do_lock(
                    target,
                    interpreter,
                    existing,
                    self.constraints,
                    self.settings,
                    self.network_settings,
                    self.state,
                    self.logger,
                    self.concurrency,
                    self.cache,
                    self.printer,
                    self.preview,
                )
                .await?;

                // If the lockfile changed, write it to disk.
                if !matches!(self.mode, LockMode::DryRun(_)) {
                    if let LockResult::Changed(_, lock) = &result {
                        target.commit(lock).await?;
                    }
                }

                Ok(result)
            }
        }
    }
}

/// Lock the project requirements into a lockfile.
async fn do_lock(
    target: LockTarget<'_>,
    interpreter: &Interpreter,
    existing_lock: Option<Lock>,
    external: Vec<NameRequirementSpecification>,
    settings: &ResolverSettings,
    network_settings: &NetworkSettings,
    state: &UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<LockResult, ProjectError> {
    let start = std::time::Instant::now();

    // Extract the project settings.
    let ResolverSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        fork_strategy,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
        sources,
    } = settings;

    // Collect the requirements, etc.
    let members = target.members();
    let packages = target.packages();
    let requirements = target.requirements();
    let overrides = target.overrides();
    let constraints = target.constraints();
    let build_constraints = target.build_constraints();
    let dependency_groups = target.dependency_groups()?;
    let source_trees = vec![];

    // If necessary, lower the overrides and constraints.
    let requirements = target.lower(requirements, index_locations, *sources)?;
    let overrides = target.lower(overrides, index_locations, *sources)?;
    let constraints = target.lower(constraints, index_locations, *sources)?;
    let build_constraints = target.lower(build_constraints, index_locations, *sources)?;
    let dependency_groups = dependency_groups
        .into_iter()
        .map(|(name, requirements)| {
            let requirements = target.lower(requirements, index_locations, *sources)?;
            Ok((name, requirements))
        })
        .collect::<Result<BTreeMap<_, _>, ProjectError>>()?;

    // Collect the conflicts.
    let mut conflicts = target.conflicts();
    if let LockTarget::Workspace(workspace) = target {
        if let Some(groups) = &workspace.pyproject_toml().dependency_groups {
            if let Some(project) = &workspace.pyproject_toml().project {
                conflicts.expand_transitive_group_includes(&project.name, groups);
            }
        }
    }

    // Collect the list of supported environments.
    let environments = {
        let environments = target.environments();

        // Ensure that the environments are disjoint.
        if let Some(environments) = &environments {
            for (lhs, rhs) in environments
                .as_markers()
                .iter()
                .zip(environments.as_markers().iter().skip(1))
            {
                if !lhs.is_disjoint(*rhs) {
                    let mut hint = lhs.negate();
                    hint.and(*rhs);

                    let lhs = lhs
                        .contents()
                        .map(|contents| contents.to_string())
                        .unwrap_or_else(|| "true".to_string());
                    let rhs = rhs
                        .contents()
                        .map(|contents| contents.to_string())
                        .unwrap_or_else(|| "true".to_string());
                    let hint = hint
                        .contents()
                        .map(|contents| contents.to_string())
                        .unwrap_or_else(|| "true".to_string());

                    return Err(ProjectError::OverlappingMarkers(lhs, rhs, hint));
                }
            }
        }

        environments
    };

    // Collect the list of required platforms.
    let required_environments = if let Some(required_environments) = target.required_environments()
    {
        // Ensure that the environments are disjoint.
        for (lhs, rhs) in required_environments
            .as_markers()
            .iter()
            .zip(required_environments.as_markers().iter().skip(1))
        {
            if !lhs.is_disjoint(*rhs) {
                let mut hint = lhs.negate();
                hint.and(*rhs);

                let lhs = lhs
                    .contents()
                    .map(|contents| contents.to_string())
                    .unwrap_or_else(|| "true".to_string());
                let rhs = rhs
                    .contents()
                    .map(|contents| contents.to_string())
                    .unwrap_or_else(|| "true".to_string());
                let hint = hint
                    .contents()
                    .map(|contents| contents.to_string())
                    .unwrap_or_else(|| "true".to_string());

                return Err(ProjectError::OverlappingMarkers(lhs, rhs, hint));
            }
        }

        Some(required_environments)
    } else {
        None
    };

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    let requires_python = target.requires_python()?;

    let requires_python = if let Some(requires_python) = requires_python {
        if requires_python.is_unbounded() {
            let default =
                RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
            warn_user_once!("The workspace `requires-python` value (`{requires_python}`) does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `{default}`).");
        } else if requires_python.is_exact_without_patch() {
            warn_user_once!("The workspace `requires-python` value (`{requires_python}`) contains an exact match without a patch version. When omitted, the patch version is implicitly `0` (e.g., `{requires_python}.0`). Did you mean `{requires_python}.*`?");
        }
        requires_python
    } else {
        let default =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
        warn_user_once!(
            "No `requires-python` value found in the workspace. Defaulting to `{default}`."
        );
        default
    };

    // If any of the forks are incompatible with the Python requirement, error.
    for environment in environments
        .map(SupportedEnvironments::as_markers)
        .into_iter()
        .flatten()
        .copied()
    {
        if requires_python.to_marker_tree().is_disjoint(environment) {
            return if let Some(contents) = environment.contents() {
                Err(ProjectError::DisjointEnvironment(
                    contents,
                    requires_python.specifiers().clone(),
                ))
            } else {
                Err(ProjectError::EmptyEnvironment)
            };
        }
    }

    // Determine the Python requirement.
    let python_requirement =
        PythonRequirement::from_requires_python(interpreter, requires_python.clone());

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    for index in target.indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(network_settings.native_tls)
        .connectivity(network_settings.connectivity)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .url_auth_policies(UrlAuthPolicies::from(index_locations))
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
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

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease_mode(*prerelease)
        .fork_strategy(*fork_strategy)
        .exclude_newer(*exclude_newer)
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .required_environments(required_environments.cloned().unwrap_or_default())
        .build();
    let hasher = HashStrategy::Generate(HashGeneration::Url);

    let build_constraints = Constraints::from_requirements(build_constraints.iter().cloned());

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_hasher = HashStrategy::default();
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::new();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    let workspace_cache = WorkspaceCache::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.fork().into_inner(),
        *index_strategy,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        &build_hasher,
        *exclude_newer,
        *sources,
        workspace_cache,
        concurrency,
        preview,
    );

    let database = DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads);

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = if let Some(existing_lock) = existing_lock {
        match ValidatedLock::validate(
            existing_lock,
            target.install_path(),
            packages,
            &members,
            &requirements,
            &dependency_groups,
            &constraints,
            &overrides,
            &conflicts,
            environments,
            required_environments,
            dependency_metadata,
            interpreter,
            &requires_python,
            index_locations,
            upgrade,
            &options,
            &hasher,
            state.index(),
            &database,
            printer,
        )
        .await
        {
            Ok(result) => Some(result),
            Err(ProjectError::Lock(err)) if err.is_resolution() => {
                // Resolver errors are not recoverable, as such errors can leave the resolver in a
                // broken state. Specifically, tasks that fail with an error can be left as pending.
                return Err(ProjectError::Lock(err));
            }
            Err(err) => {
                warn_user!("Failed to validate existing lockfile: {err}");
                None
            }
        }
    } else {
        None
    };

    match existing_lock {
        // Resolution from the lockfile succeeded.
        Some(ValidatedLock::Satisfies(lock)) => {
            // Print the success message after completing resolution.
            logger.on_complete(lock.len(), start, printer)?;

            Ok(LockResult::Unchanged(lock))
        }

        // The lockfile did not contain enough information to obtain a resolution, fallback
        // to a fresh resolve.
        _ => {
            // Determine whether we can reuse the existing package versions.
            let versions_lock = existing_lock.as_ref().and_then(|lock| match &lock {
                ValidatedLock::Satisfies(lock) => Some(lock),
                ValidatedLock::Preferable(lock) => Some(lock),
                ValidatedLock::Versions(lock) => Some(lock),
                ValidatedLock::Unusable(_) => None,
            });

            // If an existing lockfile exists, build up a set of preferences.
            let LockedRequirements { preferences, git } = versions_lock
                .map(|lock| read_lock_requirements(lock, target.install_path(), upgrade))
                .transpose()?
                .unwrap_or_default();

            // Populate the Git resolver.
            for ResolvedRepositoryReference { reference, sha } in git {
                debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
                state.git().insert(reference, sha);
            }

            // Determine whether we can reuse the existing package forks.
            let forks_lock = existing_lock.as_ref().and_then(|lock| match &lock {
                ValidatedLock::Satisfies(lock) => Some(lock),
                ValidatedLock::Preferable(lock) => Some(lock),
                ValidatedLock::Versions(_) => None,
                ValidatedLock::Unusable(_) => None,
            });

            // When we run the same resolution from the lockfile again, we could get a different result the
            // second time due to the preferences causing us to skip a fork point (see the
            // `preferences-dependent-forking` packse scenario). To avoid this, we store the forks in the
            // lockfile. We read those after all the lockfile filters, to allow the forks to change when
            // the environment changed, e.g. the python bound check above can lead to different forking.
            let resolver_env = ResolverEnvironment::universal(
                forks_lock
                    .map(|lock| {
                        lock.fork_markers()
                            .iter()
                            .copied()
                            .map(UniversalMarker::combined)
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        environments
                            .cloned()
                            .map(SupportedEnvironments::into_markers)
                            .unwrap_or_default()
                    }),
            );

            // Resolve the requirements.
            let resolution = pip::operations::resolve(
                ExtrasResolver::new(&hasher, state.index(), database)
                    .with_reporter(Arc::new(ResolverReporter::from(printer)))
                    .resolve(target.members_requirements())
                    .await
                    .map_err(|err| ProjectError::Operation(err.into()))?
                    .into_iter()
                    .chain(target.group_requirements())
                    .chain(requirements.iter().cloned())
                    .chain(
                        dependency_groups
                            .values()
                            .flat_map(|requirements| requirements.iter().cloned()),
                    )
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                constraints
                    .iter()
                    .cloned()
                    .map(NameRequirementSpecification::from)
                    .chain(external)
                    .collect(),
                overrides
                    .iter()
                    .cloned()
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                source_trees,
                // The root is always null in workspaces, it "depends on" the projects
                None,
                packages.keys().cloned().collect(),
                &extras,
                &groups,
                preferences,
                EmptyInstalledPackages,
                &hasher,
                &Reinstall::default(),
                upgrade,
                None,
                resolver_env,
                python_requirement,
                conflicts.clone(),
                &client,
                &flat_index,
                state.index(),
                &build_dispatch,
                concurrency,
                options,
                Box::new(SummaryResolveLogger),
                printer,
            )
            .await?;

            // Print the success message after completing resolution.
            logger.on_complete(resolution.len(), start, printer)?;

            // Notify the user of any resolution diagnostics.
            pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

            let manifest = ResolverManifest::new(
                members,
                requirements,
                constraints,
                overrides,
                dependency_groups,
                dependency_metadata.values().cloned(),
            )
            .relative_to(target.install_path())?;

            let previous = existing_lock.map(ValidatedLock::into_lock);
            let lock = Lock::from_resolution(&resolution, target.install_path())?
                .with_manifest(manifest)
                .with_conflicts(conflicts)
                .with_supported_environments(
                    environments
                        .cloned()
                        .map(SupportedEnvironments::into_markers)
                        .unwrap_or_default(),
                )
                .with_required_environments(
                    required_environments
                        .cloned()
                        .map(SupportedEnvironments::into_markers)
                        .unwrap_or_default(),
                );

            Ok(LockResult::Changed(previous, lock))
        }
    }
}

#[derive(Debug)]
enum ValidatedLock {
    /// An existing lockfile was provided, and it satisfies the workspace requirements.
    Satisfies(Lock),
    /// An existing lockfile was provided, but its contents should be ignored.
    Unusable(Lock),
    /// An existing lockfile was provided, and the locked versions and forks should be preferred if
    /// possible, even though the lockfile does not satisfy the workspace requirements.
    Preferable(Lock),
    /// An existing lockfile was provided, and the locked versions should be preferred if possible,
    /// though the forks should be ignored.
    Versions(Lock),
}

impl ValidatedLock {
    /// Validate a [`Lock`] against the workspace requirements.
    async fn validate<Context: BuildContext>(
        lock: Lock,
        install_path: &Path,
        packages: &BTreeMap<PackageName, WorkspaceMember>,
        members: &[PackageName],
        requirements: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Vec<Requirement>>,
        constraints: &[Requirement],
        overrides: &[Requirement],
        conflicts: &Conflicts,
        environments: Option<&SupportedEnvironments>,
        required_environments: Option<&SupportedEnvironments>,
        dependency_metadata: &DependencyMetadata,
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
        index_locations: &IndexLocations,
        upgrade: &Upgrade,
        options: &Options,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, Context>,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        // Start with the most severe condition: a fundamental option changed between resolutions.
        if lock.resolution_mode() != options.resolution_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in resolution mode: `{}` vs. `{}`",
                lock.resolution_mode().cyan(),
                options.resolution_mode.cyan()
            );
            return Ok(Self::Unusable(lock));
        }
        if lock.prerelease_mode() != options.prerelease_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in pre-release mode: `{}` vs. `{}`",
                lock.prerelease_mode().cyan(),
                options.prerelease_mode.cyan()
            );
            return Ok(Self::Unusable(lock));
        }
        if lock.fork_strategy() != options.fork_strategy {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in fork strategy: `{}` vs. `{}`",
                lock.fork_strategy().cyan(),
                options.fork_strategy.cyan()
            );
            return Ok(Self::Unusable(lock));
        }
        match (lock.exclude_newer(), options.exclude_newer) {
            (None, None) => (),
            (Some(existing), Some(provided)) if existing == provided => (),
            (Some(existing), Some(provided)) => {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to change in timestamp cutoff: `{}` vs. `{}`",
                    existing.cyan(),
                    provided.cyan()
                );
                return Ok(Self::Unusable(lock));
            }
            (Some(existing), None) => {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to removal of timestamp cutoff: `{}`",
                    existing.cyan(),
                );
                return Ok(Self::Unusable(lock));
            }
            (None, Some(provided)) => {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to addition of timestamp cutoff: `{}`",
                    provided.cyan()
                );
                return Ok(Self::Unusable(lock));
            }
        }

        match upgrade {
            Upgrade::None => {}
            Upgrade::All => {
                // If the user specified `--upgrade`, then we can't use the existing lockfile.
                debug!("Ignoring existing lockfile due to `--upgrade`");
                return Ok(Self::Unusable(lock));
            }
            Upgrade::Packages(_) => {
                // If the user specified `--upgrade-package`, then at best we can prefer some of
                // the existing versions.
                debug!("Ignoring existing lockfile due to `--upgrade-package`");
                return Ok(Self::Preferable(lock));
            }
        }

        // If the Requires-Python bound has changed, we have to perform a clean resolution, since
        // the set of `resolution-markers` may no longer cover the entire supported Python range.
        if lock.requires_python().range() != requires_python.range() {
            debug!(
                "Ignoring existing lockfile due to change in Python requirement: `{}` vs. `{}`",
                lock.requires_python(),
                requires_python,
            );
            return if lock.fork_markers().is_empty() {
                Ok(Self::Preferable(lock))
            } else {
                Ok(Self::Versions(lock))
            };
        }

        // If the set of supported environments has changed, we have to perform a clean resolution.
        let expected = lock.simplified_supported_environments();
        let actual = environments
            .map(SupportedEnvironments::as_markers)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(|marker| lock.simplify_environment(marker))
            .collect::<Vec<_>>();
        if expected != actual {
            debug!(
                "Ignoring existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
            );
            return Ok(Self::Versions(lock));
        }

        if let Err((fork_markers_union, environments_union)) = lock.check_marker_coverage() {
            warn_user!(
                "Ignoring existing lockfile due to fork markers not covering the supported environments: `{}` vs `{}`",
                fork_markers_union.try_to_string().unwrap_or("true".to_string()),
                environments_union.try_to_string().unwrap_or("true".to_string()),
            );
            return Ok(Self::Versions(lock));
        }

        // If the set of required platforms has changed, we have to perform a clean resolution.
        let expected = lock.simplified_required_environments();
        let actual = required_environments
            .map(SupportedEnvironments::as_markers)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(|marker| lock.simplify_environment(marker))
            .collect::<Vec<_>>();
        if expected != actual {
            debug!(
                "Ignoring existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
            );
            return Ok(Self::Versions(lock));
        }

        // If the conflicting group config has changed, we have to perform a clean resolution.
        if conflicts != lock.conflicts() {
            debug!(
                "Ignoring existing lockfile due to change in conflicting groups: `{:?}` vs. `{:?}`",
                conflicts,
                lock.conflicts(),
            );
            return Ok(Self::Versions(lock));
        }

        // If the user provided at least one index URL (from the command line, or from a configuration
        // file), don't use the existing lockfile if it references any registries that are no longer
        // included in the current configuration.
        //
        // However, if _no_ indexes were provided, we assume that the user wants to reuse the existing
        // distributions, even though a failure to reuse the lockfile will result in re-resolving
        // against PyPI by default.
        let indexes = if index_locations.is_none() {
            None
        } else {
            Some(index_locations)
        };

        // Determine whether the lockfile satisfies the workspace requirements.
        match lock
            .satisfies(
                install_path,
                packages,
                members,
                requirements,
                constraints,
                overrides,
                dependency_groups,
                dependency_metadata,
                indexes,
                interpreter.tags()?,
                hasher,
                index,
                database,
            )
            .await?
        {
            SatisfiesResult::Satisfied => {
                debug!("Existing `uv.lock` satisfies workspace requirements");
                Ok(Self::Satisfies(lock))
            }
            SatisfiesResult::MismatchedMembers(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched members:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVirtual(name, expected) => {
                if expected {
                    debug!(
                        "Ignoring existing lockfile due to mismatched source: `{name}` (expected: `virtual`)"
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to mismatched source: `{name}` (expected: `editable`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDynamic(name, expected) => {
                if expected {
                    debug!(
                        "Ignoring existing lockfile due to static version: `{name}` (expected a dynamic version)"
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to dynamic version: `{name}` (expected a static version)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVersion(name, expected, actual) => {
                if let Some(actual) = actual {
                    debug!(
                        "Ignoring existing lockfile due to mismatched version: `{name}` (expected: `{expected}`, found: `{actual}`)"
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to mismatched version: `{name}` (expected: `{expected}`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedRequirements(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched requirements:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedConstraints(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched constraints:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedOverrides(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched overrides:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDependencyGroups(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched dependency groups:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedStaticMetadata(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched static metadata:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRoot(name) => {
                debug!("Ignoring existing lockfile due to missing root package: `{name}`");
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRemoteIndex(name, version, index) => {
                debug!(
                    "Ignoring existing lockfile due to missing remote index: `{name}` `{version}` from `{index}`"
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingLocalIndex(name, version, index) => {
                debug!(
                    "Ignoring existing lockfile due to missing local index: `{name}` `{version}` from `{}`", index.display()
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageRequirements(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Ignoring existing lockfile due to mismatched requirements for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to mismatched requirements for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageDependencyGroups(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Ignoring existing lockfile due to mismatched dependency groups for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to mismatched dependency groups for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageProvidesExtra(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Ignoring existing lockfile due to mismatched extras for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Ignoring existing lockfile due to mismatched extras for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingVersion(name) => {
                debug!("Ignoring existing lockfile due to missing version: `{name}`");
                Ok(Self::Preferable(lock))
            }
        }
    }

    /// Convert the [`ValidatedLock`] into a [`Lock`].
    #[must_use]
    fn into_lock(self) -> Lock {
        match self {
            Self::Unusable(lock) => lock,
            Self::Satisfies(lock) => lock,
            Self::Preferable(lock) => lock,
            Self::Versions(lock) => lock,
        }
    }
}

/// A modification to a lockfile.
#[derive(Debug, Clone)]
pub(crate) enum LockEvent<'lock> {
    Update(
        DryRun,
        PackageName,
        BTreeSet<Option<&'lock Version>>,
        BTreeSet<Option<&'lock Version>>,
    ),
    Add(DryRun, PackageName, BTreeSet<Option<&'lock Version>>),
    Remove(DryRun, PackageName, BTreeSet<Option<&'lock Version>>),
}

impl<'lock> LockEvent<'lock> {
    /// Detect the change events between an (optional) existing and updated lockfile.
    pub(crate) fn detect_changes(
        existing_lock: Option<&'lock Lock>,
        new_lock: &'lock Lock,
        dry_run: DryRun,
    ) -> impl Iterator<Item = Self> {
        // Identify the package-versions in the existing lockfile.
        let mut existing_packages: FxHashMap<&PackageName, BTreeSet<Option<&Version>>> =
            if let Some(existing_lock) = existing_lock {
                existing_lock.packages().iter().fold(
                    FxHashMap::with_capacity_and_hasher(
                        existing_lock.packages().len(),
                        FxBuildHasher,
                    ),
                    |mut acc, package| {
                        acc.entry(package.name())
                            .or_default()
                            .insert(package.version());
                        acc
                    },
                )
            } else {
                FxHashMap::default()
            };

        // Identify the package-versions in the updated lockfile.
        let mut new_packages: FxHashMap<&PackageName, BTreeSet<Option<&Version>>> =
            new_lock.packages().iter().fold(
                FxHashMap::with_capacity_and_hasher(new_lock.packages().len(), FxBuildHasher),
                |mut acc, package| {
                    acc.entry(package.name())
                        .or_default()
                        .insert(package.version());
                    acc
                },
            );

        let names = existing_packages
            .keys()
            .chain(new_packages.keys())
            .map(|name| (*name).clone())
            .collect::<BTreeSet<_>>();

        names.into_iter().filter_map(move |name| {
            match (existing_packages.remove(&name), new_packages.remove(&name)) {
                (Some(existing_versions), Some(new_versions)) => {
                    if existing_versions != new_versions {
                        Some(Self::Update(dry_run, name, existing_versions, new_versions))
                    } else {
                        None
                    }
                }
                (Some(existing_versions), None) => {
                    Some(Self::Remove(dry_run, name, existing_versions))
                }
                (None, Some(new_versions)) => Some(Self::Add(dry_run, name, new_versions)),
                (None, None) => {
                    unreachable!("The key `{name}` should exist in at least one of the maps");
                }
            }
        })
    }
}

impl std::fmt::Display for LockEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// Format a version for inclusion in the upgrade report.
        fn format_version(version: Option<&Version>) -> String {
            version
                .map(|version| format!("v{version}"))
                .unwrap_or_else(|| "(dynamic)".to_string())
        }

        match self {
            Self::Update(dry_run, name, existing_versions, new_versions) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(|version| format_version(*version))
                    .collect::<Vec<_>>()
                    .join(", ");
                let new_versions = new_versions
                    .iter()
                    .map(|version| format_version(*version))
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {existing_versions} -> {new_versions}",
                    if dry_run.enabled() {
                        "Update"
                    } else {
                        "Updated"
                    }
                    .green()
                    .bold()
                )
            }
            Self::Add(dry_run, name, new_versions) => {
                let new_versions = new_versions
                    .iter()
                    .map(|version| format_version(*version))
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {new_versions}",
                    if dry_run.enabled() { "Add" } else { "Added" }
                        .green()
                        .bold()
                )
            }
            Self::Remove(dry_run, name, existing_versions) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(|version| format_version(*version))
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {existing_versions}",
                    if dry_run.enabled() {
                        "Remove"
                    } else {
                        "Removed"
                    }
                    .red()
                    .bold()
                )
            }
        }
    }
}
