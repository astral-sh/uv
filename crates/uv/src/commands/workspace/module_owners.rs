use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, DryRun, ExtrasSpecification,
    ExtrasSpecificationWithDefaults, InstallOptions, Reinstall,
};
use uv_distribution_types::{Dist, Name, ResolvedDist};
use uv_fs::PortablePathBuf;
use uv_installer::SitePackages;
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::Preview;
use uv_pypi_types::ModuleName;
use uv_python::PythonEnvironment;
use uv_resolver::{Installable, Lock, Metadata};
use uv_settings::MalwareCheckSettings;
use uv_workspace::{Workspace, WorkspaceCache};

use crate::commands::pip::loggers::DefaultInstallLogger;
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::{resolution_markers, resolution_tags};
use crate::commands::project::UniversalState;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::sync::do_sync;
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverSettings};

/// Sync all locked extras and groups as needed, then map importable modules to package IDs.
///
/// This uses a sufficient (inexact) sync so required distributions are available to inspect
/// without removing unrelated packages from an existing environment. Only distributions in the
/// selected resolution are assigned package IDs, so those unrelated packages are not reported.
pub(crate) async fn collect_module_owners(
    workspace: &Workspace,
    lock: &Lock,
    venv: &PythonEnvironment,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &UniversalState,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    preview: Preview,
    malware_settings: &MalwareCheckSettings,
) -> Result<BTreeMap<ModuleName, Vec<String>>> {
    let target = InstallTarget::Workspace { workspace, lock };
    let extras = ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default());
    let groups = DependencyGroups::from_all_groups().with_defaults(DefaultGroups::default());
    let Some(package_ids) = selected_package_ids(target, venv, &extras, &groups, settings)? else {
        return Ok(BTreeMap::new());
    };

    let reinstall = Reinstall::None;
    let installer_settings = InstallerSettingsRef {
        index_locations: &settings.index_locations,
        index_strategy: settings.index_strategy,
        keyring_provider: settings.keyring_provider,
        dependency_metadata: &settings.dependency_metadata,
        config_setting: &settings.config_setting,
        config_settings_package: &settings.config_settings_package,
        build_isolation: &settings.build_isolation,
        extra_build_dependencies: &settings.extra_build_dependencies,
        extra_build_variables: &settings.extra_build_variables,
        exclude_newer: &settings.exclude_newer,
        link_mode: settings.link_mode,
        compile_bytecode: false,
        reinstall: &reinstall,
        build_options: &settings.build_options,
        sources: settings.sources.clone(),
    };

    do_sync(
        target,
        venv,
        &extras,
        &groups,
        None,
        InstallOptions::default(),
        Modifications::Sufficient,
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
        malware_settings,
    )
    .await?;

    find_module_owners_in_environment(venv, &package_ids)
}

/// Map the modules in an existing environment to package IDs from the lockfile.
pub(crate) fn find_module_owners(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    settings: &ResolverSettings,
) -> Result<BTreeMap<ModuleName, Vec<String>>> {
    let Some(package_ids) = selected_package_ids(target, venv, extras, groups, settings)? else {
        return Ok(BTreeMap::new());
    };

    find_module_owners_in_environment(venv, &package_ids)
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

/// Map modules in an existing environment to their selected package IDs.
fn find_module_owners_in_environment(
    venv: &PythonEnvironment,
    package_ids: &BTreeMap<PackageName, String>,
) -> Result<BTreeMap<ModuleName, Vec<String>>> {
    let mut owners = BTreeMap::<ModuleName, BTreeSet<String>>::new();
    for dist in SitePackages::from_environment(venv)?.iter() {
        let Some(package_id) = package_ids.get(dist.name()) else {
            continue;
        };
        // TODO: Editable installs often only record a `.pth` file; we'll
        // need to handle them specially.
        for module in dist.read_modules(venv.interpreter().extension_suffixes())? {
            owners.entry(module).or_default().insert(package_id.clone());
        }
    }

    Ok(owners
        .into_iter()
        .map(|(module, owners)| (module, owners.into_iter().collect()))
        .collect())
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
