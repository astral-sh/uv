#![allow(clippy::single_match_else)]

use anstream::eprint;
use distribution_types::{Diagnostic, UnresolvedRequirementSpecification, VersionId};
use owo_colors::OwoColorize;
use pep440_rs::Version;
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::collections::BTreeSet;
use std::{fmt::Write, path::Path};
use tracing::debug;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, Reinstall, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::{Workspace, DEV_DEPENDENCIES};
use uv_git::ResolvedRepositoryReference;
use uv_normalize::PackageName;
use uv_python::{Interpreter, PythonFetch, PythonPreference, PythonRequest};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_resolver::{FlatIndex, Lock, OptionsBuilder, PythonRequirement, RequiresPython};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::project::{find_requires_python, FoundInterpreter, ProjectError, SharedState};
use crate::commands::{pip, ExitStatus};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};

/// Resolve the project requirements into a lockfile.
pub(crate) async fn lock(
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
        warn_user_once!("`uv lock` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&std::env::current_dir()?, None).await?;

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

    // Read the existing lockfile.
    let existing = read(&workspace).await?;

    // Perform the lock operation.
    match do_lock(
        &workspace,
        &interpreter,
        existing.as_ref(),
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
            if !existing.is_some_and(|existing| existing == lock) {
                commit(&lock, &workspace).await?;
            }
            Ok(ExitStatus::Success)
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            let report = miette::Report::msg(format!("{err}"))
                .context("No solution found when resolving dependencies:");
            eprint!("{report:?}");
            Ok(ExitStatus::Failure)
        }
        Err(err) => Err(err.into()),
    }
}

/// Lock the project requirements into a lockfile.
pub(super) async fn do_lock(
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
    let constraints = vec![];
    let dev = vec![DEV_DEPENDENCIES.clone()];
    let source_trees = vec![];

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    let requires_python = find_requires_python(workspace)?;

    let requires_python = if let Some(requires_python) = requires_python {
        if requires_python.is_unbounded() {
            let default =
                RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
            warn_user!("The workspace `requires-python` field does not contain a lower bound: `{requires_python}`. Set a lower bound to indicate the minimum compatible Python version (e.g., `{default}`).");
        }
        requires_python
    } else {
        let default =
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version());
        warn_user!("No `requires-python` field found in the workspace. Defaulting to `{default}`.");
        default
    };

    let python_requirement = PythonRequirement::from_requires_python(interpreter, &requires_python);

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
                None,
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
            debug!("Starting clean resolution.");

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
                None,
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

    let new_lock = Lock::from_resolution_graph(&resolution)?;

    // Notify the user of any dependency updates
    if !upgrade.is_none() {
        if let Some(existing_lock) = existing_lock {
            report_upgrades(existing_lock, &new_lock, workspace.install_path(), printer)?;
        }
    }

    Ok(new_lock)
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
fn report_upgrades(
    existing_lock: &Lock,
    new_lock: &Lock,
    workspace_root: &Path,
    printer: Printer,
) -> anyhow::Result<()> {
    let existing_distributions: FxHashMap<PackageName, BTreeSet<Version>> =
        existing_lock.distributions().iter().fold(
            FxHashMap::with_capacity_and_hasher(existing_lock.distributions().len(), FxBuildHasher),
            |mut acc, distribution| {
                if let Ok(VersionId::NameVersion(name, version)) =
                    distribution.version_id(workspace_root)
                {
                    acc.entry(name).or_default().insert(version);
                }
                acc
            },
        );

    let new_distribution_names: FxHashMap<PackageName, BTreeSet<Version>> =
        new_lock.distributions().iter().fold(
            FxHashMap::with_capacity_and_hasher(new_lock.distributions().len(), FxBuildHasher),
            |mut acc, distribution| {
                if let Ok(VersionId::NameVersion(name, version)) =
                    distribution.version_id(workspace_root)
                {
                    acc.entry(name).or_default().insert(version);
                }
                acc
            },
        );

    for (name, new_versions) in new_distribution_names {
        if let Some(existing_versions) = existing_distributions.get(&name) {
            if new_versions != *existing_versions {
                let existing_versions = existing_versions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let new_versions = new_versions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    printer.stderr(),
                    "{} {name} v{existing_versions} -> v{new_versions}",
                    "Updating".green().bold()
                )?;
            }
        }
    }

    Ok(())
}
