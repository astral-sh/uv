use std::path::Path;

use anstream::print;
use anyhow::Result;
use futures::{stream, StreamExt};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{Connectivity, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, DevGroupsSpecification, LowerBound, TargetTriple, TrustedHost,
};
use uv_distribution_types::IndexCapabilities;
use uv_pep440::Version;
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest, PythonVersion};
use uv_resolver::{PackageMap, TreeDisplay};
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::lock::LockMode;
use crate::commands::project::{
    default_dependency_groups, DependencyGroupsTarget, ProjectInterpreter,
};
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
    project_dir: &Path,
    dev: DevGroupsSpecification,
    locked: bool,
    frozen: bool,
    universal: bool,
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    outdated: bool,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python: Option<String>,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(project_dir, &DiscoveryOptions::default()).await?;

    // Validate that any referenced dependency groups are defined in the workspace.
    if !frozen {
        let target = DependencyGroupsTarget::Workspace(&workspace);
        target.validate(&dev)?;
    }

    // Determine the default groups to include.
    let defaults = default_dependency_groups(workspace.pyproject_toml())?;

    // Find an interpreter for the project, unless `--frozen` and `--universal` are both set.
    let interpreter = if frozen && universal {
        None
    } else {
        Some(
            ProjectInterpreter::discover(
                &workspace,
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_downloads,
                connectivity,
                native_tls,
                allow_insecure_host,
                no_config,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        )
    };

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = SharedState::default();

    // Update the lockfile, if necessary.
    let lock = project::lock::do_safe_lock(
        mode,
        &workspace,
        settings.as_ref(),
        LowerBound::Allow,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_lock();

    // Determine the markers to use for resolution.
    let markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    // If necessary, look up the latest version of each package.
    let latest = if outdated {
        let ResolverSettings {
            index_locations: _,
            index_strategy: _,
            keyring_provider,
            resolution: _,
            prerelease: _,
            dependency_metadata: _,
            config_setting: _,
            no_build_isolation: _,
            no_build_isolation_package: _,
            exclude_newer: _,
            link_mode: _,
            upgrade: _,
            build_options: _,
            sources: _,
        } = &settings;

        let capabilities = IndexCapabilities::default();

        // Initialize the registry client.
        let client =
            RegistryClientBuilder::new(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
                .native_tls(native_tls)
                .connectivity(connectivity)
                .keyring(*keyring_provider)
                .allow_insecure_host(allow_insecure_host.to_vec())
                .build();

        // Initialize the client to fetch the latest version of each package.
        let client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease: lock.prerelease_mode(),
            exclude_newer: lock.exclude_newer(),
            requires_python: lock.requires_python(),
            tags: None,
        };

        // Fetch the latest version for each package.
        stream::iter(lock.packages())
            .filter_map(|package| async {
                let index = package.index(workspace.install_path()).ok()??;
                let filename = client
                    .find_latest(package.name(), Some(&index))
                    .await
                    .ok()??;
                if filename.version() == package.version() {
                    None
                } else {
                    Some((package.clone(), filename.into_version()))
                }
            })
            .collect::<PackageMap<Version>>()
            .await
    } else {
        PackageMap::default()
    };

    // Render the tree.
    let tree = TreeDisplay::new(
        &lock,
        markers.as_ref(),
        &latest,
        depth.into(),
        &prune,
        &package,
        &dev.with_defaults(defaults),
        no_dedupe,
        invert,
    );

    print!("{tree}");

    Ok(ExitStatus::Success)
}
