use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions, Reinstall,
};
use uv_distribution_types::Name;
use uv_install_wheel::read_record;
use uv_installer::SitePackages;
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::Preview;
use uv_python::PythonEnvironment;
use uv_resolver::{Installable, Lock};
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
) -> Result<BTreeMap<String, Vec<PackageName>>> {
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

    let package_names = resolution
        .distributions()
        .map(|dist| dist.name().clone())
        .collect::<BTreeSet<_>>();

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

    let mut owners = BTreeMap::<String, BTreeSet<PackageName>>::new();
    for dist in SitePackages::from_environment(venv)?
        .iter()
        .filter(|dist| package_names.contains(dist.name()))
    {
        for module in inspect_installed_modules(dist.install_path())? {
            owners
                .entry(module)
                .or_default()
                .insert(dist.name().clone());
        }
    }

    Ok(owners
        .into_iter()
        .map(|(module, owners)| (module, owners.into_iter().collect()))
        .collect())
}

fn inspect_installed_modules(dist_info: &Path) -> Result<BTreeSet<String>> {
    if !has_extension(dist_info, "dist-info") {
        return Ok(BTreeSet::new());
    }

    let mut modules = BTreeSet::new();

    let top_level = dist_info.join("top_level.txt");
    if let Ok(contents) = fs_err::read_to_string(top_level) {
        for line in contents.lines() {
            add_module_name(line.trim(), &mut modules);
        }
    }

    let record_path = dist_info.join("RECORD");
    let record = read_record(fs_err::File::open(&record_path)?)?;
    for entry in record {
        add_record_module(&entry.path, &mut modules);
    }

    Ok(modules)
}

fn add_record_module(path: &str, modules: &mut BTreeSet<String>) {
    let components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let Some((file_name, parents)) = components.split_last() else {
        return;
    };

    if components
        .iter()
        .any(|component| has_extension(component, "dist-info"))
    {
        return;
    }
    if components
        .first()
        .is_some_and(|component| has_extension(component, "data"))
    {
        return;
    }

    let mut module_components = parents.to_vec();
    if *file_name == "__init__.py" {
        // The parent path is the package.
    } else if let Some(stem) = file_name.strip_suffix(".py") {
        module_components.push(stem);
    } else if let Some(stem) = extension_module_stem(file_name) {
        if stem != "__init__" {
            module_components.push(stem);
        }
    } else {
        return;
    }

    add_module_components(&module_components, modules);
}

fn extension_module_stem(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_suffix(".so")
        .or_else(|| file_name.strip_suffix(".pyd"))?;
    stem.split('.').next().filter(|stem| !stem.is_empty())
}

fn has_extension(path: impl AsRef<Path>, extension: &str) -> bool {
    path.as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
}

fn add_module_name(module: &str, modules: &mut BTreeSet<String>) {
    if module.is_empty() {
        return;
    }
    let components = module.split('.').collect::<Vec<_>>();
    add_module_components(&components, modules);
}

fn add_module_components(components: &[&str], modules: &mut BTreeSet<String>) {
    if components.is_empty() || !components.iter().all(|component| is_identifier(component)) {
        return;
    }

    for index in 1..=components.len() {
        modules.insert(components[..index].join("."));
    }
}

fn is_identifier(component: &str) -> bool {
    let mut chars = component.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|char| char == '_' || char.is_ascii_alphanumeric())
}
