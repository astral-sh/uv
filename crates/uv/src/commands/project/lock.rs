#![allow(clippy::single_match_else)]

use std::collections::BTreeSet;
use std::fmt::Write;

use anstream::eprint;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};
use tracing::debug;

use distribution_types::{Diagnostic, UnresolvedRequirementSpecification};
use pep440_rs::Version;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, Reinstall, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::DEV_DEPENDENCIES;
use uv_fs::CWD;
use uv_git::ResolvedRepositoryReference;
use uv_normalize::PackageName;
use uv_python::{Interpreter, PythonFetch, PythonPreference, PythonRequest};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_resolver::{
    FlatIndex, Lock, OptionsBuilder, PythonRequirement, RequiresPython, ResolverMarkers,
};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::project::{find_requires_python, FoundInterpreter, ProjectError, SharedState};
use crate::commands::{pip, ExitStatus};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};

/// The result of running a lock operation.
#[derive(Debug, Clone)]
pub(crate) struct LockResult {
    /// The previous lock, if any.
    pub(crate) previous: Option<Lock>,
    /// The updated lock.
    pub(crate) lock: Lock,
}

/// Resolve the project requirements into a lockfile.
pub(crate) async fn lock(
    locked: bool,
    frozen: bool,
    python: Option<String>,
    settings: ResolverSettings,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv lock` is experimental and may change without warning");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = FoundInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_fetch,
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
        &SharedState::default(),
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
    {
        Ok(lock) => {
            if let Some(previous) = lock.previous.as_ref() {
                report_upgrades(previous, &lock.lock, printer)?;
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
    state: &SharedState,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<LockResult, ProjectError> {
    if frozen {
        // Read the existing lockfile, but don't attempt to lock the project.
        let existing = read(workspace)
            .await?
            .ok_or_else(|| ProjectError::MissingLockfile)?;
        Ok(LockResult {
            previous: None,
            lock: existing,
        })
    } else if locked {
        // Read the existing lockfile.
        let existing = read(workspace)
            .await?
            .ok_or_else(|| ProjectError::MissingLockfile)?;

        // Perform the lock operation, but don't write the lockfile to disk.
        let lock = do_lock(
            workspace,
            interpreter,
            Some(&existing),
            settings,
            state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        // If the locks disagree, return an error.
        if lock != existing {
            return Err(ProjectError::LockMismatch);
        }

        Ok(LockResult {
            previous: Some(existing),
            lock,
        })
    } else {
        // Read the existing lockfile.
        let existing = read(workspace).await?;

        // Perform the lock operation.
        let lock = do_lock(
            workspace,
            interpreter,
            existing.as_ref(),
            settings,
            state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        if !existing.as_ref().is_some_and(|existing| *existing == lock) {
            commit(&lock, workspace).await?;
        }

        Ok(LockResult {
            previous: existing,
            lock,
        })
    }
}

/// Lock the project requirements into a lockfile.
async fn do_lock(
    workspace: &Workspace,
    interpreter: &Interpreter,
    existing_lock: Option<&Lock>,
    settings: ResolverSettingsRef<'_>,
    state: &SharedState,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<Lock, ProjectError> {
    // Extract the project settings.
    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        config_setting,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
    } = settings;

    // When locking, include the project itself (as editable).
    let requirements = workspace
        .members_as_requirements()
        .into_iter()
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
        warn_user!("No `requires-python` value found in the workspace. Defaulting to `{default}`.");
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

    let options = OptionsBuilder::new()
        .resolution_mode(resolution)
        .prerelease_mode(prerelease)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build();
    let hasher = HashStrategy::Generate;

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_isolation = BuildIsolation::default();
    let extras = ExtrasSpecification::default();
    let setup_py = SetupPyStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    // If any of the resolution-determining settings changed, invalidate the lock.
    let existing_lock = existing_lock.filter(|lock| {
        if lock.resolution_mode() != options.resolution_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in resolution mode: `{}` vs. `{}`",
                lock.resolution_mode().cyan(),
                options.resolution_mode.cyan()
            );
            return false;
        }
        if lock.prerelease_mode() != options.prerelease_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in prerelease mode: `{}` vs. `{}`",
                lock.prerelease_mode().cyan(),
                options.prerelease_mode.cyan()
            );
            return false;
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
                return false;
            }
            (Some(existing), None) => {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to removal of timestamp cutoff: `{}`",
                    existing.cyan(),
                );
                return false;
            }
            (None, Some(provided)) => {
                let _ = writeln!(
                    printer.stderr(),
                    "Ignoring existing lockfile due to addition of timestamp cutoff: `{}`",
                    provided.cyan()
                );
                return false;
            }
        }
        true
    });

    // If an existing lockfile exists, build up a set of preferences.
    let LockedRequirements { preferences, git } = existing_lock
        .as_ref()
        .map(|lock| read_lock_requirements(lock, upgrade))
        .unwrap_or_default();

    // Populate the Git resolver.
    for ResolvedRepositoryReference { reference, sha } in git {
        state.git.insert(reference, sha);
    }

    let start = std::time::Instant::now();

    let requires_python = find_requires_python(workspace)?;
    let existing_lock = existing_lock.filter(|lock| {
        match (lock.requires_python(), requires_python.as_ref()) {
            // If the Requires-Python bound in the lockfile is weaker or equivalent to the
            // Requires-Python bound in the workspace, we should have the necessary wheels to perform
            // a locked resolution.
            (None, Some(_)) => true,
            (Some(locked), Some(specified)) if locked.bound() == specified.bound() => true,

            // On the other hand, if the bound in the lockfile is stricter, meaning the
            // bound has since been weakened, we have to perform a clean resolution to ensure
            // we fetch the necessary wheels.
            _ => false,
        }
    });

    // When we run the same resolution from the lockfile again, we could get a different result the
    // second time due to the preferences causing us to skip a fork point (see
    // "preferences-dependent-forking" packse scenario). To avoid this, we store the forks in the
    // lockfile. We read those after all the lockfile filters, to allow the forks to change when
    // the environment changed, e.g. the python bound check above can lead to different forking.
    let resolver_markers =
        ResolverMarkers::universal(existing_lock.and_then(|lock| lock.fork_markers().clone()));

    let resolution = match existing_lock {
        None => None,

        // If we are ignoring pinned versions in the lockfile, we need to do a full resolution.
        Some(_) if upgrade.is_all() => None,

        // Otherwise, we can try to resolve using metadata in the lockfile.
        //
        // When resolving from the lockfile we can still download and install new distributions,
        // but we rely on the lockfile for the metadata of any existing distributions. If we have
        // any outdated metadata we fall back to a clean resolve.
        Some(lock) => {
            debug!("Resolving with existing `uv.lock`");

            // Prefill the index with the lockfile metadata.
            let index = lock.to_index(workspace.install_path(), upgrade)?;

            // Create a build dispatch.
            let build_dispatch = BuildDispatch::new(
                &client,
                cache,
                interpreter,
                index_locations,
                &flat_index,
                &index,
                &state.git,
                &state.in_flight,
                index_strategy,
                setup_py,
                config_setting,
                build_isolation,
                link_mode,
                build_options,
                exclude_newer,
                concurrency,
                preview,
            );

            // Resolve the requirements.
            pip::operations::resolve(
                requirements.clone(),
                constraints.clone(),
                overrides.clone(),
                dev.clone(),
                source_trees.clone(),
                None,
                &extras,
                preferences.clone(),
                EmptyInstalledPackages,
                &hasher,
                &Reinstall::default(),
                upgrade,
                None,
                resolver_markers,
                python_requirement.clone(),
                &client,
                &flat_index,
                &index,
                &build_dispatch,
                concurrency,
                options,
                printer,
                preview,
                true,
            )
            .await
            .inspect_err(|err| debug!("Resolution with `uv.lock` failed: {err}"))
            .ok()
            .filter(|resolution| {
                // Ensure no diagnostics were emitted that may be caused by stale metadata in the lockfile.
                if resolution.diagnostics().is_empty() {
                    return true;
                }

                debug!("Resolution with `uv.lock` failed due to diagnostics:");
                for diagnostic in resolution.diagnostics() {
                    debug!("{}", diagnostic.message());
                }

                false
            })
        }
    };

    let resolution = match resolution {
        // Resolution from the lockfile succeeded.
        Some(resolution) => resolution,

        // The lockfile did not contain enough information to obtain a resolution, fallback
        // to a fresh resolve.
        None => {
            debug!("Starting clean resolution");

            // Create a build dispatch.
            let build_dispatch = BuildDispatch::new(
                &client,
                cache,
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
                concurrency,
                preview,
            );

            // Resolve the requirements.
            pip::operations::resolve(
                requirements,
                constraints,
                overrides,
                dev,
                source_trees,
                None,
                &extras,
                preferences,
                EmptyInstalledPackages,
                &hasher,
                &Reinstall::default(),
                upgrade,
                None,
                ResolverMarkers::universal(None),
                python_requirement,
                &client,
                &flat_index,
                &state.index,
                &build_dispatch,
                concurrency,
                options,
                printer,
                preview,
                true,
            )
            .await?
        }
    };

    // Print the success message after completing resolution.
    pip::operations::resolution_success(&resolution, start, printer)?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(Lock::from_resolution_graph(&resolution)?)
}

/// Write the lockfile to disk.
pub(crate) async fn commit(lock: &Lock, workspace: &Workspace) -> Result<(), ProjectError> {
    let encoded = lock.to_toml()?;
    fs_err::tokio::write(workspace.install_path().join("uv.lock"), encoded).await?;
    Ok(())
}

/// Read the lockfile from the workspace.
///
/// Returns `Ok(None)` if the lockfile does not exist.
pub(crate) async fn read(workspace: &Workspace) -> Result<Option<Lock>, ProjectError> {
    match fs_err::tokio::read_to_string(&workspace.install_path().join("uv.lock")).await {
        Ok(encoded) => match toml::from_str::<Lock>(&encoded) {
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
    let existing_distributions: FxHashMap<&PackageName, BTreeSet<&Version>> =
        existing_lock.distributions().iter().fold(
            FxHashMap::with_capacity_and_hasher(existing_lock.distributions().len(), FxBuildHasher),
            |mut acc, distribution| {
                acc.entry(distribution.name())
                    .or_default()
                    .insert(distribution.version());
                acc
            },
        );

    let new_distributions: FxHashMap<&PackageName, BTreeSet<&Version>> =
        new_lock.distributions().iter().fold(
            FxHashMap::with_capacity_and_hasher(new_lock.distributions().len(), FxBuildHasher),
            |mut acc, distribution| {
                acc.entry(distribution.name())
                    .or_default()
                    .insert(distribution.version());
                acc
            },
        );

    for name in existing_distributions
        .keys()
        .chain(new_distributions.keys())
        .collect::<BTreeSet<_>>()
    {
        match (
            existing_distributions.get(name),
            new_distributions.get(name),
        ) {
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
