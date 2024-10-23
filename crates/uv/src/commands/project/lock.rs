#![allow(clippy::single_match_else)]

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;

use anstream::eprint;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, Constraints, ExtrasSpecification, LowerBound, Reinstall,
    TrustedHost, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    DependencyMetadata, Index, IndexLocations, NameRequirementSpecification,
    UnresolvedRequirementSpecification,
};
use uv_git::ResolvedRepositoryReference;
use uv_normalize::{PackageName, DEV_DEPENDENCIES};
use uv_pep440::Version;
use uv_pypi_types::{Requirement, SupportedEnvironments};
use uv_python::{Interpreter, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_requirements::ExtrasResolver;
use uv_resolver::{
    FlatIndex, InMemoryIndex, Lock, Options, OptionsBuilder, PythonRequirement, RequiresPython,
    ResolverManifest, ResolverMarkers, SatisfiesResult,
};
use uv_types::{BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::{DefaultResolveLogger, ResolveLogger, SummaryResolveLogger};
use crate::commands::project::{
    find_requires_python, ProjectError, ProjectInterpreter, SharedState,
};
use crate::commands::reporters::ResolverReporter;
use crate::commands::{diagnostics, pip, ExitStatus};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};

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
pub(crate) async fn lock(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    python: Option<String>,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(project_dir, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = ProjectInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Perform the lock operation.
    match do_safe_lock(
        locked,
        frozen,
        &workspace,
        &interpreter,
        settings.as_ref(),
        LowerBound::Warn,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await
    {
        Ok(lock) => {
            if let LockResult::Changed(Some(previous), lock) = &lock {
                report_upgrades(previous, lock, printer)?;
            }
            Ok(ExitStatus::Success)
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            diagnostics::no_solution(&err);
            Ok(ExitStatus::Failure)
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::FetchAndBuild(dist, err),
        ))) => {
            diagnostics::fetch_and_build(dist, err);
            Ok(ExitStatus::Failure)
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::Build(dist, err),
        ))) => {
            diagnostics::build(dist, err);
            Ok(ExitStatus::Failure)
        }

        Err(err) => Err(err.into()),
    }
}

/// Perform a lock operation, respecting the `--locked` and `--frozen` parameters.
pub(super) async fn do_safe_lock(
    locked: bool,
    frozen: bool,
    workspace: &Workspace,
    interpreter: &Interpreter,
    settings: ResolverSettingsRef<'_>,
    bounds: LowerBound,
    state: &SharedState,
    logger: Box<dyn ResolveLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<LockResult, ProjectError> {
    if frozen {
        // Read the existing lockfile, but don't attempt to lock the project.
        let existing = read(workspace)
            .await?
            .ok_or_else(|| ProjectError::MissingLockfile)?;
        Ok(LockResult::Unchanged(existing))
    } else if locked {
        // Read the existing lockfile.
        let existing = read(workspace)
            .await?
            .ok_or_else(|| ProjectError::MissingLockfile)?;

        // Perform the lock operation, but don't write the lockfile to disk.
        let result = do_lock(
            workspace,
            interpreter,
            Some(existing),
            settings,
            bounds,
            state,
            logger,
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        // If the lockfile changed, return an error.
        if matches!(result, LockResult::Changed(_, _)) {
            return Err(ProjectError::LockMismatch);
        }

        Ok(result)
    } else {
        // Read the existing lockfile.
        let existing = read(workspace).await?;

        // Perform the lock operation.
        let result = do_lock(
            workspace,
            interpreter,
            existing,
            settings,
            bounds,
            state,
            logger,
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        // If the lockfile changed, write it to disk.
        if let LockResult::Changed(_, lock) = &result {
            commit(lock, workspace).await?;
        }

        Ok(result)
    }
}

/// Lock the project requirements into a lockfile.
async fn do_lock(
    workspace: &Workspace,
    interpreter: &Interpreter,
    existing_lock: Option<Lock>,
    settings: ResolverSettingsRef<'_>,
    bounds: LowerBound,
    state: &SharedState,
    logger: Box<dyn ResolveLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<LockResult, ProjectError> {
    let start = std::time::Instant::now();

    // Extract the project settings.
    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
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
    let requirements = workspace.non_project_requirements().collect::<Vec<_>>();
    let overrides = workspace.overrides().into_iter().collect::<Vec<_>>();
    let constraints = workspace.constraints();
    let dev = vec![DEV_DEPENDENCIES.clone()];
    let source_trees = vec![];

    // Collect the list of members.
    let members = {
        let mut members = workspace.packages().keys().cloned().collect::<Vec<_>>();
        members.sort();

        // If this is a non-virtual project with a single member, we can omit it from the lockfile.
        // If any members are added or removed, it will inherently mismatch. If the member is
        // renamed, it will also mismatch.
        if members.len() == 1 && !workspace.is_non_project() {
            members.clear();
        }

        members
    };

    // Collect the list of supported environments.
    let environments = {
        let environments = workspace.environments();

        // Ensure that the environments are disjoint.
        if let Some(environments) = &environments {
            for (lhs, rhs) in environments
                .as_markers()
                .iter()
                .zip(environments.as_markers().iter().skip(1))
            {
                if !lhs.is_disjoint(rhs) {
                    let mut hint = lhs.negate();
                    hint.and(rhs.clone());

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

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    let requires_python = find_requires_python(workspace)?;

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
            uv_auth::store_credentials(index.raw_url(), credentials);
        }
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
    let hasher = HashStrategy::Generate;

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let extras = ExtrasSpecification::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
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
        bounds,
        sources,
        concurrency,
    );

    let database = DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads);

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = if let Some(existing_lock) = existing_lock {
        match ValidatedLock::validate(
            existing_lock,
            workspace,
            &members,
            &requirements,
            &constraints,
            &overrides,
            environments,
            dependency_metadata,
            interpreter,
            &requires_python,
            index_locations,
            build_options,
            upgrade,
            &options,
            &hasher,
            &state.index,
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
                .map(|lock| read_lock_requirements(lock, upgrade))
                .unwrap_or_default();

            // Populate the Git resolver.
            for ResolvedRepositoryReference { reference, sha } in git {
                debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
                state.git.insert(reference, sha);
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
            let resolver_markers = ResolverMarkers::universal(
                forks_lock
                    .map(|lock| lock.fork_markers().to_vec())
                    .unwrap_or_else(|| {
                        environments
                            .cloned()
                            .map(SupportedEnvironments::into_markers)
                            .unwrap_or_default()
                    }),
            );

            // Resolve the requirements.
            let resolution = pip::operations::resolve(
                ExtrasResolver::new(&hasher, &state.index, database)
                    .with_reporter(ResolverReporter::from(printer))
                    .resolve(workspace.members_requirements())
                    .await?
                    .into_iter()
                    .chain(requirements.iter().cloned())
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                constraints
                    .iter()
                    .cloned()
                    .map(NameRequirementSpecification::from)
                    .collect(),
                overrides
                    .iter()
                    .cloned()
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                dev,
                source_trees,
                // The root is always null in workspaces, it "depends on" the projects
                None,
                Some(workspace.packages().keys().cloned().collect()),
                &extras,
                preferences,
                EmptyInstalledPackages,
                &hasher,
                &Reinstall::default(),
                upgrade,
                None,
                resolver_markers,
                python_requirement,
                &client,
                &flat_index,
                &state.index,
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
                dependency_metadata.values().cloned(),
            )
            .relative_to(workspace)?;

            let previous = existing_lock.map(ValidatedLock::into_lock);
            let lock = Lock::from_resolution_graph(&resolution, workspace.install_path())?
                .with_manifest(manifest)
                .with_supported_environments(
                    environments
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
        workspace: &Workspace,
        members: &[PackageName],
        requirements: &[Requirement],
        constraints: &[Requirement],
        overrides: &[Requirement],
        environments: Option<&SupportedEnvironments>,
        dependency_metadata: &DependencyMetadata,
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
        index_locations: &IndexLocations,
        build_options: &BuildOptions,
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
            .cloned()
            .map(|marker| lock.simplify_environment(marker))
            .collect::<Vec<_>>();
        if expected != actual {
            debug!(
                "Ignoring existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
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
                workspace,
                members,
                requirements,
                constraints,
                overrides,
                dependency_metadata,
                indexes,
                build_options,
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
                    "Ignoring existing lockfile due to mismatched members:\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedSources(name, expected) => {
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
                    "Ignoring existing lockfile due to mismatched requirements:\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedConstraints(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched constraints:\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedOverrides(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched overrides:\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedStaticMetadata(expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched static metadata:\n  Expected: {:?}\n  Actual: {:?}",
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
            SatisfiesResult::MismatchedRequiresDist(name, version, expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched `requires-dist` for: `{name}=={version}`\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDevDependencies(name, version, expected, actual) => {
                debug!(
                    "Ignoring existing lockfile due to mismatched dev dependencies for: `{name}=={version}`\n  Expected: {:?}\n  Actual: {:?}",
                    expected, actual
                );
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

/// Write the lockfile to disk.
async fn commit(lock: &Lock, workspace: &Workspace) -> Result<(), ProjectError> {
    let encoded = lock.to_toml()?;
    fs_err::tokio::write(workspace.install_path().join("uv.lock"), encoded).await?;
    Ok(())
}

/// Read the lockfile from the workspace.
///
/// Returns `Ok(None)` if the lockfile does not exist.
pub(crate) async fn read(workspace: &Workspace) -> Result<Option<Lock>, ProjectError> {
    match fs_err::tokio::read_to_string(&workspace.install_path().join("uv.lock")).await {
        Ok(encoded) => match toml::from_str(&encoded) {
            Ok(lock) => Ok(Some(lock)),
            Err(err) => {
                eprint!("Failed to parse lockfile; ignoring locked requirements: {err}");
                Ok(None)
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Reports on the versions that were upgraded in the new lockfile.
fn report_upgrades(existing_lock: &Lock, new_lock: &Lock, printer: Printer) -> anyhow::Result<()> {
    let existing_packages: FxHashMap<&PackageName, BTreeSet<&Version>> =
        existing_lock.packages().iter().fold(
            FxHashMap::with_capacity_and_hasher(existing_lock.packages().len(), FxBuildHasher),
            |mut acc, package| {
                acc.entry(package.name())
                    .or_default()
                    .insert(package.version());
                acc
            },
        );

    let new_distributions: FxHashMap<&PackageName, BTreeSet<&Version>> =
        new_lock.packages().iter().fold(
            FxHashMap::with_capacity_and_hasher(new_lock.packages().len(), FxBuildHasher),
            |mut acc, package| {
                acc.entry(package.name())
                    .or_default()
                    .insert(package.version());
                acc
            },
        );

    for name in existing_packages
        .keys()
        .chain(new_distributions.keys())
        .collect::<BTreeSet<_>>()
    {
        match (existing_packages.get(name), new_distributions.get(name)) {
            (Some(existing_versions), Some(new_versions)) => {
                if existing_versions != new_versions {
                    let existing_versions = existing_versions
                        .iter()
                        .map(|version| format!("v{version}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let new_versions = new_versions
                        .iter()
                        .map(|version| format!("v{version}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        printer.stderr(),
                        "{} {name} {existing_versions} -> {new_versions}",
                        "Updated".green().bold()
                    )?;
                }
            }
            (Some(existing_versions), None) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(|version| format!("v{version}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    printer.stderr(),
                    "{} {name} {existing_versions}",
                    "Removed".red().bold()
                )?;
            }
            (None, Some(new_versions)) => {
                let new_versions = new_versions
                    .iter()
                    .map(|version| format!("v{version}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    printer.stderr(),
                    "{} {name} {new_versions}",
                    "Added".green().bold()
                )?;
            }
            (None, None) => {
                unreachable!("The key `{name}` should exist in at least one of the maps");
            }
        }
    }

    Ok(())
}
