use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::Printer;
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, DryRun, ExtrasSpecification,
    ExtrasSpecificationWithDefaults, InstallOptions, Modifications, Reinstall,
};
use uv_dispatch::UniversalState;
use uv_distribution_types::{Dist, InstalledDist, Name, ResolvedDist};
use uv_environment_operations::install_target::InstallTarget;
use uv_environment_operations::malware::MalwareCheckContext;
use uv_environment_operations::sync_from_lock;
use uv_fs::PortablePathBuf;
use uv_install_operations::loggers::DefaultInstallLogger;
use uv_installer::SitePackages;
use uv_lock::{Installable, Metadata};
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::Preview;
use uv_pypi_types::ModuleName;
use uv_python_interpreter::PythonEnvironment;
use uv_resolve_operations::{resolution_markers, resolution_tags};
use uv_settings::{InstallerSettingsRef, MalwareCheckSettings, ResolverSettings};
use uv_workspace::WorkspaceCache;

/// Installed distributions and their module ownership.
pub(super) struct CollectedEnvironment {
    /// All installed distributions, including unmanaged packages.
    pub(super) packages: SitePackages,
    /// Unmanaged distributions with discoverable modules.
    pub(super) unmanaged_distributions: Vec<InstalledDist>,
    /// Maps importable module names to the IDs of their owning distributions.
    pub(super) module_owners: BTreeMap<ModuleName, Vec<String>>,
}

/// Collect installed distributions and module ownership, optionally synchronizing first.
///
/// Synchronization includes all locked extras and groups. By default, it retains unmanaged packages
/// outside the selected resolution; exact synchronization removes them.
pub(super) async fn collect_environment(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &UniversalState,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    preview: Preview,
    malware_settings: &MalwareCheckSettings,
    sync: Option<Modifications>,
) -> Result<CollectedEnvironment> {
    let (extras, groups) = target_selection(target);
    let package_ids = selected_package_ids(target, venv, &extras, &groups, settings)?;

    if let Some(modifications) = sync
        && match modifications {
            Modifications::Sufficient => package_ids.is_some(),
            Modifications::Exact => true,
        }
    {
        let reinstall = Reinstall::None;
        let installer_settings = InstallerSettingsRef {
            index_locations: &settings.index_locations,
            index_strategy: settings.index_strategy,
            keyring_provider: settings.keyring_provider,
            dependency_metadata: &settings.dependency_metadata,
            config_setting: &settings.config_setting,
            config_settings_package: &settings.config_settings_package,
            build_isolation: &settings.build_isolation,
            build_hash_checking: settings.build_hash_checking,
            extra_build_dependencies: &settings.extra_build_dependencies,
            extra_build_variables: &settings.extra_build_variables,
            exclude_newer: &settings.exclude_newer,
            link_mode: settings.link_mode,
            compile_bytecode: false,
            reinstall: &reinstall,
            build_options: &settings.build_options,
            sources: settings.sources.clone(),
        };

        sync_from_lock(
            target,
            venv,
            &extras,
            &groups,
            None,
            InstallOptions::default(),
            modifications,
            None,
            installer_settings,
            client_builder,
            &state.fork(),
            Box::new(DefaultInstallLogger),
            false,
            concurrency,
            cache,
            workspace_cache,
            DryRun::Disabled,
            Printer::Silent,
            preview,
            MalwareCheckContext::from(malware_settings),
        )
        .await?;
    }

    inspect_environment(venv, &package_ids.unwrap_or_default())
}

/// Select the package IDs that can own modules in the target resolution.
fn selected_package_ids(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    settings: &ResolverSettings,
) -> Result<Option<BTreeMap<PackageName, String>>> {
    let marker_env = resolution_markers(None, None, venv.interpreter());
    let tags = resolution_tags(None, None, venv.interpreter())?;

    let resolution = target.to_resolution(
        &marker_env,
        &tags,
        extras,
        groups,
        &settings.build_options,
        &InstallOptions::default(),
    )?;
    if resolution.is_empty() {
        return Ok(None);
    }

    let workspace_root = PortablePathBuf::from(target.install_path());
    let mut package_ids = BTreeMap::<PackageName, String>::new();
    for dist in resolution.distributions().filter(|dist| !is_virtual(dist)) {
        package_ids.insert(
            dist.name().clone(),
            Metadata::package_node_id(&workspace_root, dist)?,
        );
    }
    Ok(Some(package_ids))
}

/// Collect installed distributions and associate their modules with selected or unmanaged package IDs.
fn inspect_environment(
    venv: &PythonEnvironment,
    package_ids: &BTreeMap<PackageName, String>,
) -> Result<CollectedEnvironment> {
    let packages = SitePackages::from_environment(venv)?;
    let mut unmanaged_distributions = Vec::new();
    let mut owners = BTreeMap::<ModuleName, BTreeSet<String>>::new();
    for dist in packages.iter() {
        let selected_package_id = package_ids.get(dist.name());
        // TODO: Editable installs often only record a `.pth` file; we'll
        // need to handle them specially.
        let modules = match dist.read_modules(venv.interpreter().extension_suffixes()) {
            Ok(modules) => modules,
            Err(err) if selected_package_id.is_none() => {
                // Incomplete module metadata in an unmanaged package should not prevent
                // inventory collection or module discovery for other packages.
                tracing::warn!(
                    "Failed to discover modules for unmanaged package `{}`: {err}",
                    dist.name()
                );
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        if modules.is_empty() {
            continue;
        }
        let package_id = if let Some(package_id) = selected_package_id {
            package_id.clone()
        } else {
            unmanaged_distributions.push(dist.clone());
            Metadata::unmanaged_package_node_id(dist)
        };
        for module in modules {
            owners.entry(module).or_default().insert(package_id.clone());
        }
    }

    Ok(CollectedEnvironment {
        packages,
        unmanaged_distributions,
        module_owners: owners
            .into_iter()
            .map(|(module, owners)| (module, owners.into_iter().collect()))
            .collect(),
    })
}

fn target_selection(
    target: InstallTarget<'_>,
) -> (
    ExtrasSpecificationWithDefaults,
    DependencyGroupsWithDefaults,
) {
    match target {
        InstallTarget::Script { .. } => (
            ExtrasSpecification::default().with_defaults(DefaultExtras::default()),
            DependencyGroups::default().with_defaults(DefaultGroups::default()),
        ),
        InstallTarget::Project { .. }
        | InstallTarget::Projects { .. }
        | InstallTarget::Workspace { .. }
        | InstallTarget::Lockfile { .. }
        | InstallTarget::NonProjectWorkspace { .. } => (
            ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default()),
            DependencyGroups::from_all_groups().with_defaults(DefaultGroups::default()),
        ),
    }
}

fn is_virtual(dist: &ResolvedDist) -> bool {
    let ResolvedDist::Installable { dist, .. } = dist else {
        return false;
    };
    let Dist::Source(source) = dist.as_ref() else {
        return false;
    };
    source.is_virtual()
}
