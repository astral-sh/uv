use std::collections::HashMap;

use cyclonedx_bom::models::component::Classification;
use cyclonedx_bom::models::dependency::{Dependencies, Dependency};
use cyclonedx_bom::models::metadata::Metadata;
use cyclonedx_bom::models::tool::{Tool, Tools};
use cyclonedx_bom::prelude::{Bom, Component, Components, NormalizedString};
use itertools::Itertools;

use uv_configuration::{
    DependencyGroupsWithDefaults, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_normalize::PackageName;

use crate::lock::export::{ExportableRequirement, ExportableRequirements};
use crate::lock::{Package, PackageId, Source};
use crate::{Installable, LockError};

pub fn from_lock<'lock>(
    target: &impl Installable<'lock>,
    prune: &[PackageName],
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    annotate: bool,
    install_options: &'lock InstallOptions,
) -> Result<Bom, LockError> {
    // Extract the packages from the lock file.
    let ExportableRequirements(mut nodes) =
        ExportableRequirements::from_lock(target, prune, extras, groups, annotate, install_options);

    nodes.sort_unstable_by_key(|node| &node.package.id);

    let root = target.lock().root();

    // Used as prefix in bom-ref generation, to ensure uniqueness
    let mut id_counter = 1;
    let mut package_to_bom_ref = HashMap::<&PackageId, Component>::new();

    let metadata = Metadata {
        component: root.map(|package| {
            create_and_register_component(
                package,
                Classification::Application,
                &mut id_counter,
                &mut package_to_bom_ref,
            )
        }),
        timestamp: cyclonedx_bom::prelude::DateTime::now().ok(),
        tools: Some(Tools::List(vec![Tool {
            vendor: Some(NormalizedString::new("Astral Software Inc.")),
            name: Some(NormalizedString::new("uv")),
            version: Some(NormalizedString::new(uv_version::version())),
            hashes: None,
            external_references: None,
        }])),
        ..Metadata::default()
    };

    let dependencies = nodes
        .iter()
        .filter(|node| root.is_none_or(|package| package.id != node.package.id));

    let components = dependencies
        .map(|node| {
            create_and_register_component(
                node.package,
                Classification::Library,
                &mut id_counter,
                &mut package_to_bom_ref,
            )
        })
        .collect();

    let dependencies = create_dependencies_from_mapping(&nodes, &package_to_bom_ref);

    let bom = Bom {
        metadata: Some(metadata),
        components: Some(Components(components)),
        dependencies: Some(dependencies),
        ..Bom::default()
    };

    Ok(bom)
}

/// Create and register a CycloneDX component, updating the counter and map
fn create_and_register_component<'a>(
    package: &'a Package,
    classification: Classification,
    id_counter: &mut usize,
    package_to_bom_ref: &mut HashMap<&'a PackageId, Component>,
) -> Component {
    let component = create_component_from_package(package, classification, *id_counter);
    package_to_bom_ref.insert(&package.id, component.clone());
    *id_counter += 1;
    component
}

/// Creates a bom-ref string in the format "{id}-{package_name}@{version}" or "{id}-{package_name}" if no version is provided.
fn create_bom_ref(id: usize, name: &str, version: Option<&str>) -> String {
    if let Some(version) = version {
        format!("{id}-{name}@{version}")
    } else {
        format!("{id}-{name}")
    }
}

/// Extract version string from a package
fn get_version_string(package: &Package) -> Option<String> {
    package.id.version.as_ref().map(|v| v.to_string())
}

/// Extract package name string from a package
fn get_package_name(package: &Package) -> String {
    package.id.name.to_string()
}

/// Generate a Package URL (purl) from a package
fn create_purl(package: &Package) -> Option<String> {
    let name = get_package_name(package);
    let version = get_version_string(package);

    let (purl_type, qualifiers) = match &package.id.source {
        Source::Registry(_) => ("pypi", String::new()),
        Source::Git(url, _) | Source::Direct(url, _) => {
            ("generic", format!("?download_url={}", url.as_ref()))
        }
        // No purl for local sources
        Source::Path(_) | Source::Directory(_) | Source::Editable(_) | Source::Virtual(_) => {
            return None;
        }
    };

    let version_specifier = version.map_or_else(String::new, |v| format!("@{v}"));

    Some(format!(
        "pkg:{purl_type}/{name}{version_specifier}{qualifiers}"
    ))
}

/// Create a CycloneDX component from a package node with the given classification and ID
fn create_component_from_package(
    package: &Package,
    classification: Classification,
    id: usize,
) -> Component {
    let name = get_package_name(package);
    let version = get_version_string(package);
    let bom_ref = create_bom_ref(id, &name, version.as_deref());
    let purl = create_purl(package).and_then(|purl_string| purl_string.parse().ok());

    Component {
        component_type: classification,
        name: NormalizedString::new(&name),
        version: version.as_deref().map(NormalizedString::new),
        bom_ref: Some(bom_ref),
        purl,
        mime_type: None,
        supplier: None,
        author: None,
        publisher: None,
        group: None,
        description: None,
        scope: None,
        hashes: None,
        licenses: None,
        copyright: None,
        cpe: None,
        swid: None,
        modified: None,
        pedigree: None,
        external_references: None,
        properties: None,
        components: None,
        evidence: None,
        signature: None,
        model_card: None,
        data: None,
    }
}

fn create_dependencies_from_mapping(
    nodes: &[ExportableRequirement<'_>],
    package_to_component: &HashMap<&PackageId, Component>,
) -> Dependencies {
    let dependencies = nodes.iter().filter_map(|node| {
        package_to_component
            .get(&node.package.id)
            .map(|package_bom_ref| {
                let immediate_deps = &node.package.dependencies;
                let optional_deps = node.package.optional_dependencies.values().flatten();
                let dep_groups = node.package.dependency_groups.values().flatten();

                let package_deps = immediate_deps
                    .iter()
                    .chain(optional_deps)
                    .chain(dep_groups)
                    .filter_map(|dep| package_to_component.get(&dep.package_id));

                let bom_refs = package_deps
                    .map(|p| p.bom_ref.clone().expect("bom-ref should always exist"))
                    .sorted_unstable()
                    .unique()
                    .collect();

                Dependency {
                    dependency_ref: package_bom_ref
                        .bom_ref
                        .clone()
                        .expect("bom-ref should always exist"),
                    dependencies: bom_refs,
                }
            })
    });
    Dependencies(dependencies.collect())
}
