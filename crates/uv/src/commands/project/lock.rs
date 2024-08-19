#![allow(clippy::single_match_else)]

use std::collections::BTreeSet;
use std::fmt::Write;

use anstream::eprint;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use distribution_types::{IndexLocations, UnresolvedRequirementSpecification};
use pep440_rs::Version;
use pypi_types::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, Reinstall, SetupPyStrategy, Upgrade};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::CWD;
use uv_git::ResolvedRepositoryReference;
use uv_normalize::{PackageName, DEV_DEPENDENCIES};
use uv_python::{Interpreter, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_requirements::NamedRequirementsResolver;
use uv_resolver::{
    FlatIndex, Lock, Options, OptionsBuilder, PythonRequirement, RequiresPython, ResolverManifest,
    ResolverMarkers, SatisfiesResult,
};
use uv_types::{BuildContext, BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::{DefaultResolveLogger, ResolveLogger, SummaryResolveLogger};
use crate::commands::project::{find_requires_python, FoundInterpreter, ProjectError, SharedState};
use crate::commands::reporters::ResolverReporter;
use crate::commands::{pip, ExitStatus};
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
    locked: bool,
    frozen: bool,
    python: Option<String>,
    settings: ResolverSettings,

    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = FoundInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Perform the lock operation.
    match do_safe_lock(
        locked,
        frozen,
        &workspace,
        &interpreter,
        settings.as_ref(),
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
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
            let report = miette::Report::msg(format!("{err}")).context(err.header());
            eprint!("{report:?}");
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
    logger: Box<dyn ResolveLogger>,

    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<LockResult, ProjectError> {
    // Use isolate state for universal resolution. When resolving, we don't enforce that the
    // prioritized distributions match the current platform. So if we lock here, then try to
    // install from the same state, and we end up performing a resolution during the sync (i.e.,
    // for the build dependencies of a source distribution), we may try to use incompatible
    // distributions.
    // TODO(charlie): In universal resolution, we should still track version compatibility! We
    // just need to accept versions that are platform-incompatible. That would also make us more
    // likely to (e.g.) download a wheel that we'll end up using when installing. This would
    // make it safe to share the state.
    let state = SharedState::default();

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
            &state,
            logger,
            connectivity,
            concurrency,
            native_tls,
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
            &state,
            logger,
            connectivity,
            concurrency,
            native_tls,
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
    state: &SharedState,
    logger: Box<dyn ResolveLogger>,

    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
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
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
        sources,
    } = settings;

    // When locking, include the project itself (as editable).
    let requirements = workspace
        .members_requirements()
        .chain(workspace.root_requirements())
        .map(UnresolvedRequirementSpecification::from)
        .collect::<Vec<_>>();
    let overrides = workspace
        .overrides()
        .into_iter()
        .map(UnresolvedRequirementSpecification::from)
        .collect::<Vec<_>>();
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
        if members.len() == 1 && !workspace.is_virtual() {
            members.clear();
        }

        members
    };

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    let requires_python = find_requires_python(workspace)?;

    let requires_python = if let Some(requires_python) = requires_python {
        if requires_python.is_unbounded() {
            let default =
                RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
            warn_user!("The workspace `requires-python` value does not contain a lower bound: `{requires_python}`. Set a lower bound to indicate the minimum compatible Python version (e.g., `{default}`).");
        }
        requires_python
    } else {
        let default =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
        if workspace.only_virtual() {
            debug!("No `requires-python` in virtual-only workspace. Defaulting to `{default}`.");
        } else {
            warn_user!(
                "No `requires-python` value found in the workspace. Defaulting to `{default}`."
            );
        }
        default
    };

    let python_requirement = PythonRequirement::from_requires_python(interpreter, &requires_python);

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
    let build_constraints = [];
    let extras = ExtrasSpecification::default();
    let setup_py = SetupPyStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        &state.index,
        &state.git,
        &state.in_flight,
        index_strategy,
        setup_py,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        exclude_newer,
        sources,
        concurrency,
    );

    let database = DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads);

    // Annoyingly, we have to resolve any unnamed overrides upfront.
    let overrides = NamedRequirementsResolver::new(
        overrides,
        &hasher,
        &state.index,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = if let Some(existing_lock) = existing_lock {
        match ValidatedLock::validate(
            existing_lock,
            workspace,
            &members,
            &constraints,
            &overrides,
            interpreter,
            &requires_python,
            index_locations,
            upgrade,
            &options,
            &database,
            printer,
        )
        .await
        {
            Ok(result) => Some(result),
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
            debug!("Starting clean resolution");

            // If an existing lockfile exists, build up a set of preferences.
            let LockedRequirements { preferences, git } = existing_lock
                .as_ref()
                .and_then(|lock| match &lock {
                    ValidatedLock::Preferable(lock) => Some(lock),
                    ValidatedLock::Satisfies(lock) => Some(lock),
                    ValidatedLock::Unusable(_) => None,
                })
                .map(|lock| read_lock_requirements(lock, upgrade))
                .unwrap_or_default();

            // Populate the Git resolver.
            for ResolvedRepositoryReference { reference, sha } in git {
                debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
                state.git.insert(reference, sha);
            }

            // When we run the same resolution from the lockfile again, we could get a different result the
            // second time due to the preferences causing us to skip a fork point (see
            // "preferences-dependent-forking" packse scenario). To avoid this, we store the forks in the
            // lockfile. We read those after all the lockfile filters, to allow the forks to change when
            // the environment changed, e.g. the python bound check above can lead to different forking.
            let resolver_markers = ResolverMarkers::universal(if upgrade.is_all() {
                // We're discarding all preferences, so we're also discarding the existing forks.
                vec![]
            } else {
                existing_lock
                    .as_ref()
                    .map(|existing_lock| existing_lock.lock().fork_markers().to_vec())
                    .unwrap_or_default()
            });

            // Resolve the requirements.
            let resolution = pip::operations::resolve(
                requirements,
                constraints.clone(),
                overrides
                    .iter()
                    .cloned()
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                dev,
                source_trees,
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

            let previous = existing_lock.map(ValidatedLock::into_lock);
            let lock = Lock::from_resolution_graph(&resolution)?
                .with_manifest(ResolverManifest::new(members, constraints, overrides));

            Ok(LockResult::Changed(previous, lock))
        }
    }
}

#[derive(Debug)]
enum ValidatedLock {
    /// An existing lockfile was provided, but its contents should be ignored.
    Unusable(Lock),
    /// An existing lockfile was provided, and it satisfies the workspace requirements.
    Satisfies(Lock),
    /// An existing lockfile was provided, and the locked versions should be preferred if possible,
    /// even though the lockfile does not satisfy the workspace requirements.
    Preferable(Lock),
}

impl ValidatedLock {
    /// Validate a [`Lock`] against the workspace requirements.
    async fn validate<Context: BuildContext>(
        lock: Lock,
        workspace: &Workspace,
        members: &[PackageName],
        constraints: &[Requirement],
        overrides: &[Requirement],
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
        index_locations: &IndexLocations,
        upgrade: &Upgrade,
        options: &Options,
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

        // If the user specified `--upgrade`, then at best we can prefer some of the existing
        // versions.
        if !upgrade.is_none() {
            debug!("Ignoring existing lockfile due to `--upgrade`");
            return Ok(Self::Preferable(lock));
        }

        // If the Requires-Python bound in the lockfile is weaker or equivalent to the
        // Requires-Python bound in the workspace, we should have the necessary wheels to perform
        // a locked resolution.
        if let Some(locked) = lock.requires_python() {
            if locked.bound() != requires_python.bound() {
                // On the other hand, if the bound in the lockfile is stricter, meaning the
                // bound has since been weakened, we have to perform a clean resolution to ensure
                // we fetch the necessary wheels.
                debug!("Ignoring existing lockfile due to change in `requires-python`");

                // It's fine to prefer the existing versions, though.
                return Ok(Self::Preferable(lock));
            }
        }

        // If the user provided at least one index URL (from the command line, or from a configuration
        // file), don't use the existing lockfile if it references any registries that are no longer
        // included in the current configuration.
        //
        // However, iIf _no_ indexes were provided, we assume that the user wants to reuse the existing
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
                constraints,
                overrides,
                indexes,
                interpreter.tags()?,
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
            SatisfiesResult::MissingRoot(name) => {
                debug!("Ignoring existing lockfile due to missing root package: `{name}`");
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingIndex(name, version, index) => {
                debug!(
                    "Ignoring existing lockfile due to missing index: `{name}` `{version}` from `{index}`"
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingMetadata(name, version) => {
                debug!(
                    "Ignoring existing lockfile due to missing metadata for: `{name}=={version}`"
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

    /// Return the inner [`Lock`].
    fn lock(&self) -> &Lock {
        match self {
            ValidatedLock::Unusable(lock) => lock,
            ValidatedLock::Satisfies(lock) => lock,
            ValidatedLock::Preferable(lock) => lock,
        }
    }

    /// Convert the [`ValidatedLock`] into a [`Lock`].
    #[must_use]
    fn into_lock(self) -> Lock {
        match self {
            ValidatedLock::Unusable(lock) => lock,
            ValidatedLock::Satisfies(lock) => lock,
            ValidatedLock::Preferable(lock) => lock,
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
async fn read(workspace: &Workspace) -> Result<Option<Lock>, ProjectError> {
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
