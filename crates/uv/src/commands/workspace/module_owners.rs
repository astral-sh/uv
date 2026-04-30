use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions, Reinstall,
};
use uv_distribution_types::Name;
use uv_installer::SitePackages;
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::Preview;
use uv_pypi_types::ModuleName;
use uv_python::PythonEnvironment;
use uv_resolver::{Installable, Lock, Metadata};
use uv_workspace::{Workspace, WorkspaceCache};

use crate::commands::pip::loggers::DefaultInstallLogger;
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::{resolution_markers, resolution_tags};
use crate::commands::project::UniversalState;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::sync::do_sync;
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverSettings};

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
) -> Result<BTreeMap<ModuleName, Vec<String>>> {
    let target = InstallTarget::Workspace { workspace, lock };
    let marker_env = resolution_markers(None, None, venv.interpreter());
    let tags = resolution_tags(None, None, venv.interpreter())?;
    let extras = ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default());
    let groups = DependencyGroups::from_args(
        false,
        false,
        false,
        Vec::new(),
        Vec::new(),
        false,
        Vec::new(),
        true,
    )
    .with_defaults(DefaultGroups::default());

    let resolution = target.to_resolution(
        &marker_env,
        &tags,
        &extras,
        &groups,
        &settings.build_options,
        &InstallOptions::default(),
    )?;
    if resolution.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut package_ids = BTreeMap::<PackageName, BTreeSet<String>>::new();
    for dist in resolution.distributions() {
        package_ids
            .entry(dist.name().clone())
            .or_default()
            .insert(Metadata::package_node_id(workspace, dist)?);
    }

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
    )
    .await?;

    let mut owners = BTreeMap::<ModuleName, BTreeSet<String>>::new();
    for dist in SitePackages::from_environment(venv)?.iter() {
        let Some(package_ids) = package_ids.get(dist.name()) else {
            continue;
        };
        for module in dist.read_modules()? {
            owners
                .entry(module)
                .or_default()
                .extend(package_ids.iter().cloned());
        }
    }

    Ok(owners
        .into_iter()
        .map(|(module, owners)| (module, owners.into_iter().collect()))
        .collect())
}
