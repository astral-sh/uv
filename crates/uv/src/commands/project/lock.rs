#![allow(clippy::single_match_else)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use uv_cache::{Cache, Refresh};
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DependencyGroupsWithDefaults, DryRun, ExtrasSpecification, Reinstall,
    Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::{DistributionDatabase, LoweredExtraBuildDependencies};
use uv_distribution_types::{
    DependencyMetadata, HashGeneration, Index, IndexLocations, NameRequirementSpecification,
    Requirement, RequiresPython, UnresolvedRequirementSpecification,
};
use uv_git::ResolvedRepositoryReference;
use uv_git_types::GitOid;
use uv_normalize::{GroupName, PackageName};
use uv_pep440::Version;
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{ConflictKind, Conflicts, SupportedEnvironments};
use uv_python::{Interpreter, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_requirements::ExtrasResolver;
use uv_requirements::upgrade::{LockedRequirements, read_lock_requirements};
use uv_resolver::{
    FlatIndex, InMemoryIndex, Lock, Options, OptionsBuilder, Package, PythonRequirement,
    ResolverEnvironment, ResolverManifest, SatisfiesResult, UniversalMarker,
};
use uv_scripts::Pep723Script;
use uv_settings::PythonInstallMirrors;
use uv_types::{
    BuildContext, BuildIsolation, BuildPreferences, EmptyInstalledPackages, HashStrategy,
};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, Editability, Workspace, WorkspaceCache, WorkspaceMember};

use crate::commands::pip::loggers::{DefaultResolveLogger, ResolveLogger, SummaryResolveLogger};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    MissingLockfileSource, ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState,
    init_script_python_requirement, script_extra_build_requires,
};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{ExitStatus, ScriptPath, diagnostics, pip};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, LockCheckSource, ResolverSettings};

/// The result of running a lock operation.
#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub(crate) enum LockResult {
    /// The lock was unchanged.
    Unchanged(Lock),
    /// The lock was changed.
    Changed(Option<Lock>, Lock),
}

impl LockResult {
    pub(crate) fn lock(&self) -> &Lock {
        match self {
            Self::Unchanged(lock) => lock,
            Self::Changed(_, lock) => lock,
        }
    }

    pub(crate) fn into_lock(self) -> Lock {
        match self {
            Self::Unchanged(lock) => lock,
            Self::Changed(_, lock) => lock,
        }
    }
}

/// Resolve the project requirements into a lockfile.
pub(crate) async fn lock(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    dry_run: DryRun,
    refresh: Refresh,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    script: Option<ScriptPath>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> anyhow::Result<ExitStatus> {
    // If necessary, initialize the PEP 723 script.
    let script = match script {
        Some(ScriptPath::Path(path)) => {
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
                preview,
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
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else {
        interpreter = match target {
            LockTarget::Workspace(workspace) => ProjectInterpreter::discover(
                workspace,
                project_dir,
                // Don't enable any groups' requires-python for interpreter discovery
                &DependencyGroupsWithDefaults::none(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                no_config,
                Some(false),
                cache,
                printer,
                preview,
            )
            .await?
            .into_interpreter(),
            LockTarget::Script(script) => ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                no_config,
                Some(false),
                cache,
                printer,
                preview,
            )
            .await?
            .into_interpreter(),
        };

        if let LockCheck::Enabled(lock_check) = lock_check {
            LockMode::Locked(&interpreter, lock_check)
        } else if dry_run.enabled() {
            LockMode::DryRun(&interpreter)
        } else {
            LockMode::Write(&interpreter)
        }
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Perform the lock operation.
    match Box::pin(
        LockOperation::new(
            mode,
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            concurrency,
            cache,
            &workspace_cache,
            printer,
            preview,
        )
        .with_refresh(&refresh)
        .execute(target),
    )
    .await
    {
        Ok(lock) => {
            if dry_run.enabled() {
                // In `--dry-run` mode, show all changes.
                if let LockResult::Changed(previous, lock) = &lock {
                    let mut changed = false;
                    for event in LockEvent::detect_changes(previous.as_ref(), lock, dry_run) {
                        changed = true;
                        writeln!(printer.stderr(), "{event}")?;
                    }

                    // If we didn't report any version changes, but the lockfile changed, report back.
                    if !changed {
                        writeln!(printer.stderr(), "{}", "Lockfile changes detected".bold())?;
                    }
                } else {
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
        Err(err @ ProjectError::LockMismatch(..)) => {
            writeln!(printer.stderr(), "{}", err.to_string().bold())?;
            Ok(ExitStatus::Failure)
        }
        Err(ProjectError::Operation(err)) => {
            diagnostics::OperationDiagnostic::native_tls(client_builder.is_native_tls())
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
    Locked(&'env Interpreter, LockCheckSource),
    /// Use the existing lockfile without performing a resolution.
    Frozen(MissingLockfileSource),
}

/// A lock operation.
pub(super) struct LockOperation<'env> {
    mode: LockMode<'env>,
    constraints: Vec<NameRequirementSpecification>,
    refresh: Option<&'env Refresh>,
    settings: &'env ResolverSettings,
    client_builder: &'env BaseClientBuilder<'env>,
    state: &'env UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: Concurrency,
    cache: &'env Cache,
    workspace_cache: &'env WorkspaceCache,
    printer: Printer,
    preview: Preview,
}

impl<'env> LockOperation<'env> {
    /// Initialize a [`LockOperation`].
    pub(super) fn new(
        mode: LockMode<'env>,
        settings: &'env ResolverSettings,
        client_builder: &'env BaseClientBuilder<'env>,
        state: &'env UniversalState,
        logger: Box<dyn ResolveLogger>,
        concurrency: Concurrency,
        cache: &'env Cache,
        workspace_cache: &'env WorkspaceCache,
        printer: Printer,
        preview: Preview,
    ) -> Self {
        Self {
            mode,
            constraints: vec![],
            refresh: None,
            settings,
            client_builder,
            state,
            logger,
            concurrency,
            cache,
            workspace_cache,
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

    /// Set the refresh strategy for the [`LockOperation`].
    #[must_use]
    pub(super) fn with_refresh(mut self, refresh: &'env Refresh) -> Self {
        self.refresh = Some(refresh);
        self
    }

    /// Perform a [`LockOperation`].
    pub(super) async fn execute(self, target: LockTarget<'_>) -> Result<LockResult, ProjectError> {
        match self.mode {
            LockMode::Frozen(source) => {
                // Read the existing lockfile, but don't attempt to lock the project.
                let existing = target
                    .read()
                    .await?
                    .ok_or(ProjectError::MissingLockfile(source))?;
                // Check if the discovered workspace members match the locked workspace members.
                if let LockTarget::Workspace(workspace) = target {
                    for package_name in workspace.packages().keys() {
                        existing
                            .find_by_name(package_name)
                            .map_err(|_| ProjectError::LockWorkspaceMismatch(package_name.clone()))?
                            .ok_or_else(|| {
                                ProjectError::LockWorkspaceMismatch(package_name.clone())
                            })?;
                    }
                }
                Ok(LockResult::Unchanged(existing))
            }
            LockMode::Locked(interpreter, lock_source) => {
                // Read the existing lockfile.
                let existing = target
                    .read()
                    .await?
                    .ok_or(ProjectError::MissingLockfile(lock_source.into()))?;

                // Perform the lock operation, but don't write the lockfile to disk.
                let result = Box::pin(do_lock(
                    target,
                    interpreter,
                    Some(existing),
                    self.constraints,
                    self.refresh,
                    self.settings,
                    self.client_builder,
                    self.state,
                    self.logger,
                    self.concurrency,
                    self.cache,
                    self.workspace_cache,
                    self.printer,
                    self.preview,
                ))
                .await?;

                // If the lockfile changed, return an error.
                if let LockResult::Changed(prev, cur) = result {
                    return Err(ProjectError::LockMismatch(
                        prev.map(Box::new),
                        Box::new(cur),
                        lock_source,
                    ));
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
                let result = Box::pin(do_lock(
                    target,
                    interpreter,
                    existing,
                    self.constraints,
                    self.refresh,
                    self.settings,
                    self.client_builder,
                    self.state,
                    self.logger,
                    self.concurrency,
                    self.cache,
                    self.workspace_cache,
                    self.printer,
                    self.preview,
                ))
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
    refresh: Option<&Refresh>,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
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
        config_settings_package,
        build_isolation,
        extra_build_dependencies,
        extra_build_variables,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
        sources,
        torch_backend: _,
    } = settings;

    // Collect the requirements, etc.
    let members = target.members();
    let packages = target.packages();
    let required_members = target.required_members();
    let requirements = target.requirements();
    let overrides = target.overrides();
    let excludes = target.exclude_dependencies();
    let constraints = target.constraints();
    let build_constraints = target.build_constraints();
    let dependency_groups = target.dependency_groups()?;
    let source_trees = vec![];

    // If necessary, lower the overrides and constraints.
    let requirements = target.lower(
        requirements,
        index_locations,
        sources,
        client_builder.credentials_cache(),
    )?;
    let overrides = target.lower(
        overrides,
        index_locations,
        sources,
        client_builder.credentials_cache(),
    )?;
    let constraints = target.lower(
        constraints,
        index_locations,
        sources,
        client_builder.credentials_cache(),
    )?;
    let build_constraints = target.lower(
        build_constraints,
        index_locations,
        sources,
        client_builder.credentials_cache(),
    )?;
    let dependency_groups = dependency_groups
        .into_iter()
        .map(|(name, group)| {
            let requirements = target.lower(
                group.requirements,
                index_locations,
                sources,
                client_builder.credentials_cache(),
            )?;
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

    // Check if any conflicts contain project-level conflicts
    if !preview.is_enabled(PreviewFeature::PackageConflicts)
        && conflicts.iter().any(|set| {
            set.iter()
                .any(|item| matches!(item.kind(), ConflictKind::Project))
        })
    {
        warn_user_once!(
            "Declaring conflicts for packages (`package = ...`) is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::PackageConflicts
        );
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
            warn_user_once!(
                "The workspace `requires-python` value (`{requires_python}`) does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `{default}`)."
            );
        } else if requires_python.is_exact_without_patch() {
            warn_user_once!(
                "The workspace `requires-python` value (`{requires_python}`) contains an exact match without a patch version. When omitted, the patch version is implicitly `0` (e.g., `{requires_python}.0`). Did you mean `{requires_python}.*`?"
            );
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

    // Initialize the client.
    let client_builder = client_builder.clone().keyring(*keyring_provider);

    for index in target.indexes() {
        if let Some(credentials) = index.credentials() {
            if let Some(root_url) = index.root_url() {
                client_builder.store_credentials(&root_url, credentials.clone());
            }
            client_builder.store_credentials(index.raw_url(), credentials);
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(*index_strategy)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = match build_isolation {
        uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
        uv_configuration::BuildIsolation::Shared => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            BuildIsolation::Shared(&environment)
        }
        uv_configuration::BuildIsolation::SharedPackage(packages) => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            BuildIsolation::SharedPackage(&environment, packages)
        }
    };

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease_mode(*prerelease)
        .fork_strategy(*fork_strategy)
        .exclude_newer(exclude_newer.clone())
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .required_environments(required_environments.cloned().unwrap_or_default())
        .build();
    let hasher = HashStrategy::Generate(HashGeneration::Url);

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_hasher = HashStrategy::default();
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::new();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), cache);
        let entries = client
            .fetch_all(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    // Lower the extra build dependencies.
    let extra_build_requires = match &target {
        LockTarget::Workspace(workspace) => LoweredExtraBuildDependencies::from_workspace(
            extra_build_dependencies.clone(),
            workspace,
            index_locations,
            sources,
            client.credentials_cache(),
        )?,
        LockTarget::Script(script) => {
            // Try to get extra build dependencies from the script metadata
            script_extra_build_requires((*script).into(), settings, client.credentials_cache())?
        }
    }
    .into_inner();

    // Convert to the `Constraints` format.
    let dispatch_constraints = Constraints::from_requirements(build_constraints.iter().cloned());

    // Extract build dependency preferences from the existing lock file, if available.
    // These are used as hints so the resolver prefers previously locked versions.
    let build_preferences = if let Some(ref lock) = existing_lock {
        let mut prefs = BTreeMap::new();
        for package in lock.packages() {
            let build_deps = package.build_dependencies();
            if !build_deps.is_empty() {
                let deps = build_deps
                    .iter()
                    .map(|dep| (dep.name().clone(), dep.version().clone()))
                    .collect();
                prefs.insert((package.name().clone(), package.version().cloned()), deps);
            }
        }
        BuildPreferences::new(prefs)
    } else {
        BuildPreferences::default()
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &dispatch_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.fork().into_inner(),
        *index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        &extra_build_requires,
        extra_build_variables,
        *link_mode,
        build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources.clone(),
        workspace_cache.clone(),
        concurrency,
        preview,
    )
    .with_build_preferences(build_preferences);

    // Use universal resolution for build dependencies when locking, so the
    // resolved build deps work on all platforms.
    let build_dispatch = if preview.is_enabled(PreviewFeature::LockBuildDependencies) {
        build_dispatch.with_universal_resolution()
    } else {
        build_dispatch
    };

    let database = DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads);

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = if let Some(existing_lock) = existing_lock {
        match ValidatedLock::validate(
            existing_lock,
            target.install_path(),
            packages,
            &members,
            required_members,
            &requirements,
            &dependency_groups,
            &constraints,
            &overrides,
            &excludes,
            &build_constraints,
            &conflicts,
            environments,
            required_environments,
            dependency_metadata,
            interpreter,
            &requires_python,
            index_locations,
            upgrade,
            refresh,
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
                excludes.clone(),
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
                interpreter.markers(),
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
                excludes.clone(),
                build_constraints,
                dependency_groups,
                dependency_metadata.values().cloned(),
            )
            .relative_to(target.install_path())?;

            let previous = existing_lock.map(ValidatedLock::into_lock);
            let mut lock = Lock::from_resolution(&resolution, target.install_path())?
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

            // Only record build dependencies in the lock file when the preview feature is enabled.
            if preview.is_enabled(PreviewFeature::LockBuildDependencies) {
                let build_resolutions = build_dispatch.build_resolutions().snapshot();
                lock = lock.with_build_resolutions(&build_resolutions, target.install_path())?;
            }

            if previous.as_ref().is_some_and(|previous| *previous == lock) {
                Ok(LockResult::Unchanged(lock))
            } else {
                Ok(LockResult::Changed(previous, lock))
            }
        }
    }
}

#[derive(Debug)]
enum ValidatedLock {
    /// An existing lockfile was provided, but its contents should be ignored.
    Unusable(Lock),
    /// An existing lockfile was provided, and the locked versions should be preferred if possible,
    /// though the forks should be ignored.
    Versions(Lock),
    /// An existing lockfile was provided, and the locked versions and forks should be preferred if
    /// possible, even though the lockfile does not satisfy the workspace requirements.
    Preferable(Lock),
    /// An existing lockfile was provided, and it satisfies the workspace requirements.
    Satisfies(Lock),
}

impl ValidatedLock {
    /// Validate a [`Lock`] against the workspace requirements.
    async fn validate<Context: BuildContext>(
        lock: Lock,
        install_path: &Path,
        packages: &BTreeMap<PackageName, WorkspaceMember>,
        members: &[PackageName],
        required_members: &BTreeMap<PackageName, Editability>,
        requirements: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Vec<Requirement>>,
        constraints: &[Requirement],
        overrides: &[Requirement],
        excludes: &[PackageName],
        build_constraints: &[Requirement],
        conflicts: &Conflicts,
        environments: Option<&SupportedEnvironments>,
        required_environments: Option<&SupportedEnvironments>,
        dependency_metadata: &DependencyMetadata,
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
        index_locations: &IndexLocations,
        upgrade: &Upgrade,
        refresh: Option<&Refresh>,
        options: &Options,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, Context>,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        // Perform checks in a deliberate order, such that the most extreme conditions are tested
        // first (i.e., every check that returns `Self::Unusable`, followed by every check that
        // returns `Self::Versions`, followed by every check that returns `Self::Preferable`, and
        // finally `Self::Satisfies`).
        if lock.resolution_mode() != options.resolution_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in resolution mode: `{}` vs. `{}`",
                lock.resolution_mode().cyan(),
                options.resolution_mode.cyan()
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
        if let Some(change) = lock.exclude_newer().compare(&options.exclude_newer) {
            // If a relative value is used, we won't invalidate on every tick of the clock unless
            // the span duration changed or some other operation causes a new resolution
            if !change.is_relative_timestamp_change() {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to {change}",
                );
                return Ok(Self::Preferable(lock));
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
                // This is handled below, after some checks regarding fork
                // markers. In particular, we'd like to return `Preferable`
                // here, but we shouldn't if the fork markers cannot be
                // reused.
            }
        }

        // NOTE: It's important that this appears before any possible path that
        // returns `Self::Preferable`. In particular, if our fork markers are
        // bunk, then we shouldn't return a result that indicates we should try
        // to re-use the existing fork markers.
        if let Err((fork_markers_union, environments_union)) = lock.check_marker_coverage() {
            warn_user!(
                "Resolving despite existing lockfile due to fork markers not covering the supported environments: `{}` vs `{}`",
                fork_markers_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
                environments_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
            );
            return Ok(Self::Versions(lock));
        }

        // NOTE: Similarly as above, this should also appear before any
        // possible code path that can return `Self::Preferable`.
        if let Err((fork_markers_union, requires_python_marker)) =
            lock.requires_python_coverage(requires_python)
        {
            warn_user!(
                "Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `{}` vs `{}`",
                fork_markers_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
                requires_python_marker
                    .try_to_string()
                    .unwrap_or("true".to_string()),
            );
            return Ok(Self::Versions(lock));
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
                "Resolving despite existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
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
                "Resolving despite existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
            );
            return Ok(Self::Versions(lock));
        }

        // If the conflicting group config has changed, we have to perform a clean resolution.
        if conflicts != lock.conflicts() {
            debug!(
                "Resolving despite existing lockfile due to change in conflicting groups: `{:?}` vs. `{:?}`",
                conflicts,
                lock.conflicts(),
            );
            return Ok(Self::Versions(lock));
        }

        // If the Requires-Python bound has changed, we have to perform a clean resolution, since
        // the set of `resolution-markers` may no longer cover the entire supported Python range.
        if lock.requires_python().range() != requires_python.range() {
            debug!(
                "Resolving despite existing lockfile due to change in Python requirement: `{}` vs. `{}`",
                lock.requires_python(),
                requires_python,
            );
            return if lock.fork_markers().is_empty() {
                Ok(Self::Preferable(lock))
            } else {
                Ok(Self::Versions(lock))
            };
        }

        // If the pre-release mode has changed, we have to re-resolve, but can retain the existing
        // versions and forks.
        if lock.prerelease_mode() != options.prerelease_mode {
            let _ = writeln!(
                printer.stderr(),
                "Resolving despite existing lockfile due to change in pre-release mode: `{}` vs. `{}`",
                lock.prerelease_mode().cyan(),
                options.prerelease_mode.cyan()
            );
            return Ok(Self::Preferable(lock));
        }

        // If the user specified `--upgrade-package`, then at best we can prefer some of
        // the existing versions.
        if let Upgrade::Packages(_) = upgrade {
            debug!("Resolving despite existing lockfile due to `--upgrade-package`");
            return Ok(Self::Preferable(lock));
        }

        // If the user specified `--refresh`, then we have to re-resolve.
        if matches!(refresh, Some(Refresh::All(..) | Refresh::Packages(..))) {
            debug!("Resolving despite existing lockfile due to `--refresh`");
            return Ok(Self::Preferable(lock));
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
                required_members,
                requirements,
                constraints,
                overrides,
                excludes,
                build_constraints,
                dependency_groups,
                dependency_metadata,
                indexes,
                interpreter.tags()?,
                interpreter.markers(),
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
                    "Resolving despite existing lockfile due to mismatched members:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedEditable(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (expected: `editable`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (unexpected: `editable`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVirtual(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (expected: `virtual`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (unexpected: `virtual`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDynamic(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to static version: `{name}` (expected a dynamic version)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to dynamic version: `{name}` (expected a static version)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVersion(name, expected, actual) => {
                if let Some(actual) = actual {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched version: `{name}` (expected: `{expected}`, found: `{actual}`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched version: `{name}` (expected: `{expected}`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedRequirements(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched requirements:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedConstraints(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched constraints:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedOverrides(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched overrides:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedExcludes(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched excludes:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedBuildConstraints(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched build constraints:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDependencyGroups(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched dependency groups:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedStaticMetadata(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched static metadata:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRoot(name) => {
                debug!("Resolving despite existing lockfile due to missing root package: `{name}`");
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRemoteIndex(name, version, index) => {
                debug!(
                    "Resolving despite existing lockfile due to missing remote index: `{name}` `{version}` from `{index}`"
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingLocalIndex(name, version, index) => {
                debug!(
                    "Resolving despite existing lockfile due to missing local index: `{name}` `{version}` from `{}`",
                    index.display()
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageRequirements(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched requirements for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched requirements for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageDependencyGroups(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched dependency groups for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched dependency groups for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageProvidesExtra(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched extras for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched extras for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingVersion(name) => {
                debug!("Resolving despite existing lockfile due to missing version: `{name}`");
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LockEventVersion<'lock> {
    /// The version of the package, or `None` if the package has a dynamic version.
    version: Option<&'lock Version>,
    /// The short Git SHA of the package, if it was installed from a Git repository.
    sha: Option<&'lock str>,
}

impl<'lock> From<&'lock Package> for LockEventVersion<'lock> {
    fn from(value: &'lock Package) -> Self {
        Self {
            version: value.version(),
            sha: value.git_sha().map(GitOid::as_tiny_str),
        }
    }
}

impl std::fmt::Display for LockEventVersion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.version, self.sha) {
            (Some(version), Some(sha)) => write!(f, "v{version} ({sha})"),
            (Some(version), None) => write!(f, "v{version}"),
            (None, Some(sha)) => write!(f, "(dynamic) ({sha})"),
            (None, None) => write!(f, "(dynamic)"),
        }
    }
}

/// A modification to a lockfile.
#[derive(Debug, Clone)]
enum LockEvent<'lock> {
    Update(
        DryRun,
        PackageName,
        BTreeSet<LockEventVersion<'lock>>,
        BTreeSet<LockEventVersion<'lock>>,
    ),
    Add(DryRun, PackageName, BTreeSet<LockEventVersion<'lock>>),
    Remove(DryRun, PackageName, BTreeSet<LockEventVersion<'lock>>),
}

impl<'lock> LockEvent<'lock> {
    /// Detect the change events between an (optional) existing and updated lockfile.
    fn detect_changes(
        existing_lock: Option<&'lock Lock>,
        new_lock: &'lock Lock,
        dry_run: DryRun,
    ) -> impl Iterator<Item = Self> {
        // Identify the package-versions in the existing lockfile.
        let mut existing_packages: FxHashMap<&PackageName, BTreeSet<LockEventVersion>> =
            if let Some(existing_lock) = existing_lock {
                existing_lock.packages().iter().fold(
                    FxHashMap::with_capacity_and_hasher(
                        existing_lock.packages().len(),
                        FxBuildHasher,
                    ),
                    |mut acc, package| {
                        acc.entry(package.name())
                            .or_default()
                            .insert(LockEventVersion::from(package));
                        acc
                    },
                )
            } else {
                FxHashMap::default()
            };

        // Identify the package-versions in the updated lockfile.
        let mut new_packages: FxHashMap<&PackageName, BTreeSet<LockEventVersion>> =
            new_lock.packages().iter().fold(
                FxHashMap::with_capacity_and_hasher(new_lock.packages().len(), FxBuildHasher),
                |mut acc, package| {
                    acc.entry(package.name())
                        .or_default()
                        .insert(LockEventVersion::from(package));
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
        match self {
            Self::Update(dry_run, name, existing_versions, new_versions) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let new_versions = new_versions
                    .iter()
                    .map(std::string::ToString::to_string)
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
                    .map(std::string::ToString::to_string)
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
                    .map(std::string::ToString::to_string)
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
