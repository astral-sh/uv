use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rustc_hash::FxHashSet;
use tracing::debug;

use uv_cache::{Cache, Refresh};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_command_support::Printer;
use uv_configuration::{
    Concurrency, ExtrasSpecification, Override, PackageOverride, Reinstall, Upgrade,
};
use uv_dispatch::{BuildDispatch, UniversalState};
use uv_distribution::{DistributionDatabase, FirstPartyPackages, LoweredExtraBuildDependencies};
use uv_distribution_types::{
    HashCollection, NameRequirementSpecification, RequiresPython, ResolutionRecorder,
    UnresolvedRequirementSpecification,
};
use uv_git::ResolvedRepositoryReference;
use uv_lock::{GroupMetadata, Lock, ResolverManifest};
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{ConflictKind, SupportedEnvironments};
use uv_python_interpreter::{Interpreter, PythonEnvironment};
use uv_requirements::{ExtrasResolver, script_extra_build_requires};
use uv_resolve_operations::Error as ResolveError;
use uv_resolve_operations::locked_requirements::{LockedRequirements, read_lock_requirements};
use uv_resolve_operations::loggers::{ResolveLogger, SummaryResolveLogger};
use uv_resolve_operations::reporters::ResolverReporter;
use uv_resolver::{
    FlatIndex, OptionsBuilder, PythonRequirement, ResolverEnvironment, UniversalMarker,
};
use uv_settings::{LockedSource, ResolverSettings};
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_warnings::{warn_user, warn_user_once, warn_user_with_chain};
use uv_workspace::WorkspaceCache;

use crate::lock_target::find_lock_format_error;
use crate::{LockError, LockTarget, LockValidationError, MissingLockfileSource, ValidatedLock};

/// The result of running a lock operation.
#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub enum LockResult {
    /// The lock was unchanged.
    Unchanged(Lock),
    /// The lock was changed.
    Changed(Option<Lock>, Lock),
}

impl LockResult {
    /// Return the resolved lockfile.
    pub fn lock(&self) -> &Lock {
        match self {
            Self::Unchanged(lock) => lock,
            Self::Changed(_, lock) => lock,
        }
    }

    /// Consume the result and return the resolved lockfile.
    pub fn into_lock(self) -> Lock {
        match self {
            Self::Unchanged(lock) => lock,
            Self::Changed(_, lock) => lock,
        }
    }
}

/// The validation and persistence policy for a lock operation.
#[derive(Debug, Clone, Copy)]
pub enum LockMode<'env> {
    /// Write the lockfile to disk.
    Write(&'env Interpreter),
    /// Perform a resolution, but don't write the lockfile to disk.
    DryRun(&'env Interpreter),
    /// Error if the lockfile is not up-to-date with the project requirements.
    Locked(&'env Interpreter, LockedSource),
    /// Use the existing lockfile without performing a resolution.
    Frozen(MissingLockfileSource),
}

/// A lock operation.
pub struct LockOperation<'env> {
    mode: LockMode<'env>,
    constraints: Vec<NameRequirementSpecification>,
    first_party_exclusions: BTreeSet<PackageName>,
    refresh: Option<&'env Refresh>,
    check_lockfile_contents: bool,
    settings: &'env ResolverSettings,
    client_builder: &'env BaseClientBuilder<'env>,
    state: &'env UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: &'env Concurrency,
    cache: &'env Cache,
    workspace_cache: &'env WorkspaceCache,
    printer: Printer,
    preview: Preview,
}

impl<'env> LockOperation<'env> {
    /// Initialize a [`LockOperation`].
    pub fn new(
        mode: LockMode<'env>,
        settings: &'env ResolverSettings,
        client_builder: &'env BaseClientBuilder<'env>,
        state: &'env UniversalState,
        logger: Box<dyn ResolveLogger>,
        concurrency: &'env Concurrency,
        cache: &'env Cache,
        workspace_cache: &'env WorkspaceCache,
        printer: Printer,
        preview: Preview,
    ) -> Self {
        Self {
            mode,
            constraints: vec![],
            first_party_exclusions: BTreeSet::new(),
            refresh: None,
            check_lockfile_contents: false,
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
    pub fn with_constraints(mut self, constraints: Vec<NameRequirementSpecification>) -> Self {
        self.constraints = constraints;
        self
    }

    /// Exclude workspace packages that will not be installed from the first-party build exemption.
    #[must_use]
    pub fn with_first_party_exclusions(mut self, exclusions: BTreeSet<PackageName>) -> Self {
        self.first_party_exclusions = exclusions;
        self
    }

    /// Set the refresh strategy for the [`LockOperation`].
    #[must_use]
    pub fn with_refresh(mut self, refresh: &'env Refresh) -> Self {
        self.refresh = Some(refresh);
        self
    }

    /// Compare the serialized lock against the existing lockfile contents.
    #[must_use]
    pub fn with_lockfile_contents_check(mut self, enabled: bool) -> Self {
        self.check_lockfile_contents = enabled;
        self
    }

    /// Perform a [`LockOperation`].
    pub async fn execute(self, target: LockTarget<'_>) -> Result<LockResult, LockError> {
        if !matches!(&self.mode, LockMode::Frozen(_)) {
            target.validate_upgrade_groups(&self.settings.upgrade)?;
        }

        match self.mode {
            LockMode::Frozen(source) => {
                // Read the existing lockfile, but don't attempt to lock the project.
                Ok(LockResult::Unchanged(target.read_frozen(source).await?))
            }
            LockMode::Locked(interpreter, lock_source) => {
                // Read the existing lockfile.
                let lock_filename = target.lock_filename();
                let Some((existing, existing_contents)) = target.read_with_contents().await? else {
                    return Err(LockError::MissingLockfile(
                        lock_source.into(),
                        lock_filename,
                    ));
                };

                if self.preview.is_enabled(PreviewFeature::LockfileFormatCheck)
                    && let Some(line) = find_lock_format_error(&existing_contents)
                {
                    return Err(LockError::LockFormat(lock_filename, line, lock_source));
                }

                let check_lockfile_contents = if self.check_lockfile_contents {
                    Some(existing_contents)
                } else {
                    None
                };

                // Perform the lock operation, but don't write the lockfile to disk.
                let result = Box::pin(do_lock(
                    target,
                    interpreter,
                    Some(existing),
                    self.mode,
                    check_lockfile_contents,
                    self.constraints,
                    self.first_party_exclusions,
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
                    return Err(LockError::LockMismatch(
                        prev.map(Box::new),
                        Box::new(cur),
                        lock_source,
                    ));
                }

                Ok(result)
            }
            LockMode::Write(interpreter) | LockMode::DryRun(interpreter) => {
                // Read the existing lockfile.
                let (existing, existing_contents) = match target.read_with_contents().await {
                    Ok(Some((existing, existing_contents))) => {
                        (Some(existing), Some(existing_contents))
                    }
                    Ok(None) => (None, None),
                    Err(LockError::Lock(err)) => {
                        warn_user!(
                            "Failed to read existing lockfile; ignoring locked requirements: {err}"
                        );
                        (None, None)
                    }
                    Err(err) => return Err(err),
                };

                let check_lockfile_contents = if self.check_lockfile_contents {
                    existing_contents
                } else {
                    None
                };

                // Perform the lock operation.
                let result = Box::pin(do_lock(
                    target,
                    interpreter,
                    existing,
                    self.mode,
                    check_lockfile_contents,
                    self.constraints,
                    self.first_party_exclusions,
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
    mode: LockMode<'_>,
    check_lockfile_contents: Option<String>,
    external: Vec<NameRequirementSpecification>,
    first_party_exclusions: BTreeSet<PackageName>,
    refresh: Option<&Refresh>,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &UniversalState,
    logger: Box<dyn ResolveLogger>,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<LockResult, LockError> {
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
        cuda_driver_version: _,
        amd_gpu_architecture: _,
    } = settings;

    // Collect the requirements, etc.
    let members = target.members();
    let packages = target.packages();
    let required_members = target.required_members();
    let workspace_default_groups = match target {
        LockTarget::Workspace(workspace) => {
            if workspace.is_non_project() {
                Some(workspace.default_groups()?)
            } else {
                None
            }
        }
        LockTarget::Script(_) => None,
    };

    // Validate explicit defaults before omitting `["dev"]` from the lockfile. Unlike the
    // implicit default, an explicit `["dev"]` requires the `dev` group to exist.
    for member in packages.values() {
        member.default_groups()?;
    }

    let first_party_packages = match target {
        LockTarget::Workspace(workspace) => {
            FirstPartyPackages::from_workspace(workspace, &first_party_exclusions)
        }
        LockTarget::Script(_) => FirstPartyPackages::default(),
    };
    let requirements = target.requirements();
    let overrides = target.overrides();
    let excludes = target.exclude_dependencies();
    let constraints = target.constraints();
    let dependency_groups = target.dependency_groups()?;
    let workspace_group_metadata = dependency_groups
        .iter()
        .filter_map(|(name, group)| {
            group.requires_python.clone().map(|requires_python| {
                (
                    name.clone(),
                    GroupMetadata {
                        requires_python: Some(requires_python),
                    },
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let source_trees = vec![];

    // If necessary, lower the overrides and constraints.
    let requirements = target
        .lower(
            requirements,
            index_locations,
            sources,
            cache,
            workspace_cache,
            client_builder.credentials_cache(),
        )
        .await?;
    let overrides = {
        let mut lowered_overrides = Vec::new();
        for entry in overrides {
            match entry {
                Override::Requirement(requirement) => {
                    lowered_overrides.extend(
                        target
                            .lower(
                                vec![requirement],
                                index_locations,
                                sources,
                                cache,
                                workspace_cache,
                                client_builder.credentials_cache(),
                            )
                            .await?
                            .into_iter()
                            .map(Override::Requirement),
                    );
                }
                Override::Package(package) => {
                    lowered_overrides.push(Override::Package(PackageOverride {
                        package: package.package,
                        dependencies: target
                            .lower(
                                package.dependencies.into_vec(),
                                index_locations,
                                sources,
                                cache,
                                workspace_cache,
                                client_builder.credentials_cache(),
                            )
                            .await?
                            .into_boxed_slice(),
                    }));
                }
            }
        }
        lowered_overrides
    };
    let constraints = target
        .lower(
            constraints,
            index_locations,
            sources,
            cache,
            workspace_cache,
            client_builder.credentials_cache(),
        )
        .await?;
    let build_constraints = target
        .lower_build_constraints(
            index_locations,
            sources,
            cache,
            workspace_cache,
            client_builder.credentials_cache(),
        )
        .await?;
    let mut lowered_dependency_groups = BTreeMap::new();
    for (name, group) in dependency_groups {
        let requirements = target
            .lower(
                group.requirements,
                index_locations,
                sources,
                cache,
                workspace_cache,
                client_builder.credentials_cache(),
            )
            .await?;
        lowered_dependency_groups.insert(name, requirements);
    }
    let dependency_groups = lowered_dependency_groups;

    // Collect the conflicts.
    let mut conflicts = target.conflicts()?;
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
            for [lhs, rhs] in environments.as_markers().array_windows() {
                if !lhs.is_disjoint(*rhs) {
                    let hint = lhs.negate().and(*rhs);

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

                    return Err(LockError::OverlappingMarkers(lhs, rhs, hint));
                }
            }
        }

        environments
    };

    // Collect the list of required platforms.
    let required_environments = if let Some(required_environments) = target.required_environments()
    {
        // Ensure that the environments are disjoint.
        for [lhs, rhs] in required_environments.as_markers().array_windows() {
            if !lhs.is_disjoint(*rhs) {
                let hint = lhs.negate().and(*rhs);

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

                return Err(LockError::OverlappingMarkers(lhs, rhs, hint));
            }
        }

        Some(required_environments)
    } else {
        None
    };

    let minimum_libc_version = target.minimum_libc_version();
    if minimum_libc_version.is_some() && !preview.is_enabled(PreviewFeature::MinimumLibcVersion) {
        warn_user_once!(
            "Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::MinimumLibcVersion
        );
    }

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
                Err(LockError::DisjointEnvironment(
                    contents,
                    requires_python.specifiers().clone(),
                ))
            } else {
                Err(LockError::EmptyEnvironment)
            };
        }
    }

    // Determine the Python requirement.
    let python_requirement =
        PythonRequirement::from_requires_python(interpreter, requires_python.clone());

    // Initialize the client.
    let client_builder = client_builder.clone().keyring(*keyring_provider);

    for index in target.indexes() {
        if let Some(credentials) = index.credentials()? {
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
        .build()?;

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

    let lock_supported_environments = environments.cloned().unwrap_or_default();
    let lock_required_environments = required_environments.cloned().unwrap_or_default();
    let artifact_environments = SupportedEnvironments::from_markers(
        lock_supported_environments
            .iter()
            .copied()
            .chain(lock_required_environments.iter().copied())
            .collect(),
    );

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease(prerelease.clone())
        .fork_strategy(*fork_strategy)
        .exclude_newer(exclude_newer.clone())
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .artifact_environments(artifact_environments.clone())
        .minimum_libc_version(minimum_libc_version)
        .build();
    // Checking an existing lockfile may build metadata and install build dependencies. Verify any
    // artifacts recorded in that lockfile, including for an ordinary unlocked command.
    let (locked_hasher, locked_build_hasher) = if let Some(existing_lock) = existing_lock.as_ref() {
        let locked_hasher =
            existing_lock.hash_strategy(target.install_path(), &FxHashSet::default())?;
        let build_hasher = HashStrategy::from_constraints(
            &existing_lock.build_constraints(target.install_path()),
            Some(&interpreter.to_resolver_marker_environment()),
            uv_configuration::HashCheckingMode::Verify,
        )?;
        let locked_build_hasher = locked_hasher
            .clone()
            .with_constraint_hashes(&build_hasher)?;
        (locked_hasher, locked_build_hasher)
    } else {
        (HashStrategy::default(), HashStrategy::default())
    };
    // Re-resolving an outdated lock does not authorize replacing known artifacts. Only an
    // explicit unlocked upgrade releases the selected packages' hashes.
    let hash_upgrade = match mode {
        LockMode::Locked(..) => &Upgrade::default(),
        LockMode::Write(_) | LockMode::DryRun(_) | LockMode::Frozen(_) => upgrade,
    };
    let resolution_hasher = if hash_upgrade.is_none() {
        locked_hasher.clone()
    } else if let Some(existing_lock) = existing_lock.as_ref() {
        // An explicit upgrade allows replacing the selected packages' files, so do not require
        // them to match the hashes recorded in the lockfile.
        let upgrade_packages = existing_lock.upgrade_packages(hash_upgrade);
        existing_lock.hash_strategy(target.install_path(), &upgrade_packages)?
    } else {
        HashStrategy::default()
    };
    let hasher = HashStrategy::collect(HashCollection::Url)
        .with_verification(resolution_hasher.verification().clone());

    let build_hasher = HashStrategy::from_constraints(
        &build_constraints,
        Some(&interpreter.to_resolver_marker_environment()),
        uv_configuration::HashCheckingMode::Verify,
    )?;
    // Explicit build constraints apply even when fresh resolution can replace lockfile hashes.
    let resolution_build_hasher = match mode {
        LockMode::Locked(..) => locked_hasher.with_constraint_hashes(&build_hasher)?,
        LockMode::Write(_) | LockMode::DryRun(_) | LockMode::Frozen(_) => build_hasher,
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::new();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = FlatIndex::load(&client, cache, index_locations).await?;

    // Lower the extra build dependencies.
    let extra_build_requires = match &target {
        LockTarget::Workspace(workspace) => {
            LoweredExtraBuildDependencies::from_workspace(
                extra_build_dependencies.clone(),
                workspace,
                index_locations,
                sources,
                cache,
                workspace_cache,
                client.credentials_cache(),
            )
            .await?
        }
        LockTarget::Script(script) => {
            // Try to get extra build dependencies from the script metadata
            script_extra_build_requires(
                (*script).into(),
                sources,
                index_locations,
                cache,
                workspace_cache,
                client.credentials_cache(),
            )
            .await?
        }
    }
    .into_inner();

    // Create a build dispatch for fresh resolution.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
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
        &resolution_build_hasher,
        exclude_newer.clone(),
        sources.clone(),
        SourceTreeEditablePolicy::Project,
        workspace_cache.clone(),
        concurrency.clone(),
        preview,
    );

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = if let Some(existing_lock) = existing_lock {
        let validation_build_dispatch = build_dispatch.fork(&locked_build_hasher);
        let database = DistributionDatabase::new(
            &client,
            &validation_build_dispatch,
            concurrency.downloads_semaphore.clone(),
        )
        .with_first_party_packages(&first_party_packages);
        match Box::pin(ValidatedLock::validate(
            existing_lock,
            target.install_path(),
            packages,
            &members,
            required_members,
            &requirements,
            &dependency_groups,
            &workspace_group_metadata,
            workspace_default_groups.as_ref(),
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
            preview,
            printer,
        ))
        .await
        {
            Ok(result) => Some(result),
            Err(LockValidationError::Lock(err)) if err.is_resolution() || err.is_no_build() => {
                // Resolver errors are not recoverable, as such errors can leave the resolver in a
                // broken state. Specifically, tasks that fail with an error can be left as pending.
                //
                // Disabled builds are user policy errors. Static local projects are validated
                // before this point, so reaching this case means validation genuinely needs
                // metadata that cannot be obtained under `--no-build`.
                return Err(err.into());
            }
            Err(LockValidationError::Lock(err)) if err.is_not_pep625() => {
                // A non-PEP 625-compliant sdist in the lockfile will also be rejected by a fresh
                // resolve, so short-circuit rather than doing the extra work.
                return Err(err.into());
            }
            Err(err) => {
                warn_user_with_chain!(
                    anyhow::Error::from(err)
                        .context("Failed to validate existing lockfile")
                        .as_ref()
                );
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
        Some(
            ValidatedLock::Unusable(_) | ValidatedLock::Versions(_) | ValidatedLock::Preferable(_),
        )
        | None => {
            let recorder = if preview.is_enabled(PreviewFeature::ResolutionInputs) {
                Some(ResolutionRecorder::default())
            } else {
                None
            };
            let database = DistributionDatabase::new(
                &client,
                &build_dispatch,
                concurrency.downloads_semaphore.clone(),
            )
            .with_recorder(recorder.clone())
            .with_first_party_packages(&first_party_packages);

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

            // Expand the available extras for each workspace member.
            let member_requirements = ExtrasResolver::new(&hasher, state.index(), database)
                .with_reporter(Arc::new(ResolverReporter::from(printer)))
                .resolve(target.members_requirements())
                .await
                .map_err(ResolveError::from)?;
            let workspace_members = member_requirements
                .iter()
                .map(|requirement| (requirement.name.clone(), requirement.source.clone()))
                .collect();

            // Resolve the requirements.
            let (resolution, _) = uv_resolve_operations::resolve(
                member_requirements
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
                Vec::new(),
                overrides.clone(),
                excludes.clone(),
                source_trees,
                // The root is always null in workspaces, it "depends on" the projects
                None,
                workspace_members,
                &extras,
                &groups,
                preferences,
                None,
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
                recorder.clone(),
                Box::new(SummaryResolveLogger),
                printer,
            )
            .await?;

            // Print the success message after completing resolution.
            logger.on_complete(resolution.len(), start, printer)?;

            // Notify the user of any resolution diagnostics.
            uv_resolve_operations::diagnose_resolution(resolution.diagnostics(), printer)?;

            let manifest = ResolverManifest::new(
                members,
                requirements,
                constraints,
                overrides,
                excludes.clone(),
                build_constraints.specifications().cloned(),
                dependency_groups,
                dependency_metadata.values().cloned(),
            )
            .relative_to(target.install_path())?;

            let previous = existing_lock.map(ValidatedLock::into_lock);
            let lock = Lock::from_resolution(
                &resolution,
                manifest,
                target.install_path(),
                lock_supported_environments.clone().into_markers(),
                index_locations,
                preview.is_enabled(PreviewFeature::LockWithoutMetadata),
            )?
            .with_conflicts(conflicts)
            .with_required_environments(lock_required_environments.into_markers())
            .with_member_default_groups(
                packages
                    .iter()
                    .filter_map(|(name, member)| {
                        member
                            .pyproject_toml()
                            .configured_default_groups()
                            .cloned()
                            .map(|groups| (name.clone(), groups))
                    })
                    .collect(),
            )
            .with_workspace_default_groups(workspace_default_groups)
            .with_member_group_metadata(packages)?
            .with_workspace_group_metadata(workspace_group_metadata);

            let lock = if let Some(recorder) = recorder {
                lock.prune_unused(recorder.take())
            } else if preview.is_enabled(PreviewFeature::MissingExcludeNewerPackageLock) {
                lock.without_unused_exclude_newer_packages()
            } else {
                lock
            };

            let unchanged = if let Some(check_lockfile_contents) = check_lockfile_contents {
                previous.is_some() && check_lockfile_contents == lock.to_toml()?.as_str()
            } else {
                previous.as_ref().is_some_and(|previous| *previous == lock)
            };

            if unchanged {
                Ok(LockResult::Unchanged(lock))
            } else {
                Ok(LockResult::Changed(previous, lock))
            }
        }
    }
}
