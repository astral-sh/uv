use crate::Installable;
use crate::LockError;
use crate::lock::Package;
use crate::lock::PackageId;
use crate::lock::Source;
use crate::lock::export::ExportableRequirement;
use crate::lock::export::ExportableRequirements;
use cyclonedx_bom::models::component::Classification;
use cyclonedx_bom::models::dependency::Dependencies;
use cyclonedx_bom::models::dependency::Dependency;
use cyclonedx_bom::models::metadata::Metadata;
use cyclonedx_bom::models::tool::{Tool, Tools};
use cyclonedx_bom::prelude::NormalizedString;
use cyclonedx_bom::prelude::{Bom, Component, Components};
use std::collections::HashMap;
use uv_configuration::{
    DependencyGroupsWithDefaults, EditableMode, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_normalize::PackageName;

pub fn from_lock<'lock>(
    target: &impl Installable<'lock>,
    prune: &[PackageName],
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    annotate: bool,
    #[allow(unused_variables)] editable: Option<EditableMode>,
    install_options: &'lock InstallOptions,
) -> Result<Bom, LockError> {
    // Extract the packages from the lock file.
    let ExportableRequirements(mut nodes) =
        ExportableRequirements::from_lock(target, prune, extras, groups, annotate, install_options);

    nodes.sort_unstable_by_key(|node| &node.package.id);

    let root = target.lock().root();

    // ID counter for bom-ref generation
    let mut id_counter = 1;
    let mut package_to_bom_ref = HashMap::<&PackageId, Component>::new();

    let metadata = Metadata {
        component: root.map(|package| {
            let res =
                create_component_from_package(package, Classification::Application, id_counter);
            package_to_bom_ref.insert(&package.id, res.clone());
            id_counter += 1;
            res
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

    // Convert dependency packages to CycloneDX components.
    let components = dependencies
        .map(|node| {
            let res =
                create_component_from_package(node.package, Classification::Library, id_counter);
            package_to_bom_ref.insert(&node.package.id, res.clone());
            id_counter += 1;
            res
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

/// Creates a bom-ref string in the format "{id}-{package_name}@{version}"
/// or "{id}-{package_name}" if no version is provided.
fn create_bom_ref(id: usize, name: &str, version: Option<&str>) -> String {
    if let Some(version) = version {
        format!("{}-{}@{}", id, name, version)
    } else {
        format!("{}-{}", id, name)
    }
}

/// Extract version string from a package, returning empty string if no version
fn get_version_string(package: &Package) -> Option<String> {
    package.id.version.as_ref().map(|v| v.to_string())
}

/// Extract package name string from a package
fn get_package_name(package: &Package) -> String {
    package.id.name.to_string()
}

/// Generate a Package URL (PURL) from a package
fn create_purl(package: &Package) -> Option<String> {
    let name = get_package_name(package);
    let version = get_version_string(package);

    match &package.id.source {
        Source::Registry(_) => {
            if let Some(version) = version {
                Some(format!("pkg:pypi/{}@{}", name, version))
            } else {
                Some(format!("pkg:pypi/{}", name))
            }
        }
        Source::Git(url, _) => {
            if let Some(version) = version {
                Some(format!(
                    "pkg:generic/{}@{}?download_url={}",
                    name,
                    version,
                    url.as_ref()
                ))
            } else {
                Some(format!(
                    "pkg:generic/{}?download_url={}",
                    name,
                    url.as_ref()
                ))
            }
        }
        Source::Direct(url, _) => {
            if let Some(version) = version {
                Some(format!(
                    "pkg:generic/{}@{}?download_url={}",
                    name,
                    version,
                    url.as_ref()
                ))
            } else {
                Some(format!(
                    "pkg:generic/{}?download_url={}",
                    name,
                    url.as_ref()
                ))
            }
        }
        // No PURL for local sources Path, Directory, Editable, Virtual.
        Source::Path(_) | Source::Directory(_) | Source::Editable(_) | Source::Virtual(_) => None,
    }
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

fn create_dependencies_from_mapping<'lock>(
    nodes: &[ExportableRequirement<'lock>],
    package_to_component: &HashMap<&PackageId, Component>,
) -> Dependencies {
    let mut dependencies = Vec::new();

    // Add dependencies for all other packages
    for node in nodes {
        if let Some(package_bom_ref) = package_to_component.get(&node.package.id) {
            let mut package_deps = Vec::new();

            // Add regular dependencies
            for dep in &node.package.dependencies {
                if let Some(component) = package_to_component.get(&dep.package_id) {
                    package_deps.push(component);
                }
            }

            // Add optional dependencies (extras)
            for (_extra_name, deps) in &node.package.optional_dependencies {
                for dep in deps {
                    if let Some(component) = package_to_component.get(&dep.package_id) {
                        package_deps.push(component);
                    }
                }
            }

            // Add dependency groups
            for (_group_name, deps) in &node.package.dependency_groups {
                for dep in deps {
                    if let Some(component) = package_to_component.get(&dep.package_id) {
                        package_deps.push(component);
                    }
                }
            }

            // Remove duplicates and sort
            // package_deps.sort();
            package_deps.sort_by_key(|p| &p.bom_ref);
            let bom_refs = package_deps
                .iter()
                .map(|p| p.bom_ref.clone().expect("bom-ref should always exist"))
                .collect();
            package_deps.dedup();

            dependencies.push(Dependency {
                dependency_ref: package_bom_ref
                    .bom_ref
                    .clone()
                    .expect("bom-ref should always exist"),
                dependencies: bom_refs,
            });
        }
    }
    Dependencies(dependencies)
}
