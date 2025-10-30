use std::collections::HashMap;
use std::path::Path;

use cyclonedx_bom::{
    models::{
        component::Classification,
        dependency::{Dependencies, Dependency},
        metadata::Metadata,
        property::{Properties, Property},
        tool::{Tool, Tools},
    },
    prelude::{Bom, Component, Components, NormalizedString},
};
use itertools::Itertools;
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use rustc_hash::FxHashSet;

use uv_configuration::{
    DependencyGroupsWithDefaults, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_fs::PortablePath;
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;

use crate::lock::export::{ExportableRequirement, ExportableRequirements};
use crate::lock::{Package, PackageId, Source};
use crate::{Installable, LockError};

/// Character set for percent-encoding PURL components, copied from packageurl.rs (<https://github.com/scm-rs/packageurl.rs/blob/a725aa0ab332934c350641508017eb09ddfa0813/src/purl.rs#L18>).
const PURL_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    .add(b';')
    .add(b'=')
    .add(b'+')
    .add(b'@')
    .add(b'\\')
    .add(b'[')
    .add(b']')
    .add(b'^')
    .add(b'|');

/// Creates `CycloneDX` components, registering them in a `HashMap` so that they can be retrieved by `PackageId`.
/// Also ensures uniqueness when generating bom-refs by using a numeric prefix which is incremented for each component.
#[derive(Default)]
struct ComponentBuilder<'a> {
    id_counter: usize, // Used as prefix in bom-ref generation, to ensure uniqueness
    package_to_component_map: HashMap<&'a PackageId, Component>,
}

impl<'a> ComponentBuilder<'a> {
    /// Creates a bom-ref string in the format "{id}-{package_name}@{version}" or "{id}-{package_name}" if no version is provided.
    fn create_bom_ref(&mut self, name: &str, version: Option<&str>) -> String {
        self.id_counter += 1;
        let id = self.id_counter;
        if let Some(version) = version {
            format!("{id}-{name}@{version}")
        } else {
            format!("{id}-{name}")
        }
    }

    /// Extract version string from a package.
    fn get_version_string(package: &Package) -> Option<String> {
        package
            .id
            .version
            .as_ref()
            .map(std::string::ToString::to_string)
    }

    /// Extract package name string from a package.
    fn get_package_name(package: &Package) -> &str {
        package.id.name.as_str()
    }

    /// Generate a Package URL (purl) from a package. Returns `None` for local sources.
    fn create_purl(package: &Package) -> Option<String> {
        let name = percent_encode(Self::get_package_name(package).as_bytes(), PURL_ENCODE_SET);

        let version = Self::get_version_string(package).map_or_else(String::new, |v| {
            format!("@{}", percent_encode(v.as_bytes(), PURL_ENCODE_SET))
        });

        let (purl_type, qualifiers) = match &package.id.source {
            Source::Registry(_) => ("pypi", vec![]),
            Source::Git(url, _) => ("generic", vec![("vcs_url", url.as_ref())]),
            Source::Direct(url, _) => ("generic", vec![("download_url", url.as_ref())]),
            // No purl for local sources
            Source::Path(_) | Source::Directory(_) | Source::Editable(_) | Source::Virtual(_) => {
                return None;
            }
        };

        let qualifiers = if qualifiers.is_empty() {
            String::new()
        } else {
            Self::format_qualifiers(&qualifiers)
        };

        Some(format!("pkg:{purl_type}/{name}{version}{qualifiers}"))
    }

    fn format_qualifiers(qualifiers: &[(&str, &str)]) -> String {
        let joined_qualifiers = qualifiers
            .iter()
            .map(|(key, value)| {
                format!(
                    "{key}={}",
                    percent_encode(value.as_bytes(), PURL_ENCODE_SET)
                )
            })
            .join("&");
        format!("?{joined_qualifiers}")
    }

    fn create_component(
        &mut self,
        package: &'a Package,
        package_type: PackageType,
        marker: Option<&MarkerTree>,
    ) -> Component {
        let component = self.create_component_from_package(package, package_type, marker);
        self.package_to_component_map
            .insert(&package.id, component.clone());
        component
    }

    fn create_synthetic_root_component(&mut self) -> Component {
        let name = "uv-workspace";
        let bom_ref = self.create_bom_ref(name, None);

        // No need to register as we manually add dependencies in `if all_packages` check in `from_lock`
        Component {
            component_type: Classification::Application,
            name: NormalizedString::new(name),
            version: None,
            bom_ref: Some(bom_ref),
            purl: None,
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

    #[allow(clippy::needless_pass_by_value)]
    fn create_component_from_package(
        &mut self,
        package: &Package,
        package_type: PackageType,
        marker: Option<&MarkerTree>,
    ) -> Component {
        let name = Self::get_package_name(package);
        let version = Self::get_version_string(package);
        let bom_ref = self.create_bom_ref(name, version.as_deref());
        let purl = Self::create_purl(package).and_then(|purl_string| purl_string.parse().ok());
        let mut properties = vec![];

        let classification = match package_type {
            PackageType::Root => Classification::Application,
            PackageType::Workspace(path) => {
                properties.push(Property::new(
                    "uv:workspace:path",
                    &PortablePath::from(path).to_string(),
                ));
                Classification::Application
            }
            PackageType::Dependency => Classification::Library,
        };

        if let Some(marker_contents) = marker.and_then(|marker| marker.contents()) {
            properties.push(Property::new(
                "uv:package:marker",
                &marker_contents.to_string(),
            ));
        }

        Component {
            component_type: classification,
            name: NormalizedString::new(name),
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
            properties: if !properties.is_empty() {
                Some(Properties(properties))
            } else {
                None
            },
            components: None,
            evidence: None,
            signature: None,
            model_card: None,
            data: None,
        }
    }

    fn get_component(&self, id: &PackageId) -> Option<&Component> {
        self.package_to_component_map.get(id)
    }
}

pub fn from_lock<'lock>(
    target: &impl Installable<'lock>,
    prune: &[PackageName],
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    annotate: bool,
    install_options: &'lock InstallOptions,
    preview: Preview,
    all_packages: bool,
) -> Result<Bom, LockError> {
    if !preview.is_enabled(PreviewFeatures::SBOM_EXPORT) {
        warn_user!(
            "`uv export --format=cyclonedx1.5` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::SBOM_EXPORT
        );
    }

    // Extract the packages from the lock file.
    let ExportableRequirements(mut nodes) = ExportableRequirements::from_lock(
        target,
        prune,
        extras,
        groups,
        annotate,
        install_options,
    )?;

    nodes.sort_unstable_by_key(|node| &node.package.id);

    // CycloneDX requires exactly one root component in `metadata.component`.
    let root = match target.roots().collect::<Vec<_>>().as_slice() {
        // Single root: use it directly
        [single_root] => nodes
            .iter()
            .find(|node| &node.package.id.name == *single_root)
            .map(|node| node.package),
        // Multiple roots or no roots: use fallback
        _ => None,
    }
    .or_else(|| target.lock().root()); // Fallback to project root

    let mut component_builder = ComponentBuilder::default();

    let mut metadata = Metadata {
        component: root
            .map(|package| component_builder.create_component(package, PackageType::Root, None)),
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

    let workspace_member_ids = nodes
        .iter()
        .filter_map(|node| {
            if target.lock().members().contains(&node.package.id.name)
                && node.package.id.source.is_local()
            {
                Some(&node.package.id)
            } else {
                None
            }
        })
        .collect::<FxHashSet<_>>();

    let mut components = nodes
        .iter()
        .filter(|node| root.is_none_or(|root_pkg| root_pkg.id != node.package.id)) // Filter out root package as this is included in `metadata`
        .map(|node| {
            let package_type = if workspace_member_ids.contains(&node.package.id) {
                let path = match &node.package.id.source {
                    Source::Path(path)
                    | Source::Directory(path)
                    | Source::Editable(path)
                    | Source::Virtual(path) => path,
                    Source::Registry(_) | Source::Git(_, _) | Source::Direct(_, _) => {
                        // Workspace packages are always local dependencies
                        unreachable!(
                            "Workspace member {:?} has non-local source {:?}",
                            node.package.id.name, node.package.id.source,
                        )
                    }
                };
                PackageType::Workspace(path)
            } else {
                PackageType::Dependency
            };
            component_builder.create_component(node.package, package_type, Some(&node.marker))
        })
        .collect::<Vec<_>>();

    let mut dependencies = create_dependencies(&nodes, &component_builder);

    // With `--all-packages`, use synthetic root which depends on workspace root and all workspace members.
    // This ensures that we don't have any dangling components resulting from workspace packages not depended on by the workspace root.
    if all_packages {
        let synthetic_root = component_builder.create_synthetic_root_component();
        let synthetic_root_bom_ref = synthetic_root
            .bom_ref
            .clone()
            .expect("bom-ref should always exist");
        let workspace_root = metadata.component.replace(synthetic_root);

        if let Some(workspace_root) = workspace_root {
            components.push(workspace_root);
        }

        dependencies.push(Dependency {
            dependency_ref: synthetic_root_bom_ref,
            dependencies: components
                .iter()
                .filter_map(|c| {
                    if c.component_type == Classification::Application {
                        c.bom_ref.clone()
                    } else {
                        None
                    }
                })
                .collect(),
        });
    }

    let bom = Bom {
        metadata: Some(metadata),
        components: Some(Components(components)),
        dependencies: Some(Dependencies(dependencies)),
        ..Bom::default()
    };

    Ok(bom)
}

fn create_dependencies(
    nodes: &[ExportableRequirement<'_>],
    component_builder: &ComponentBuilder,
) -> Vec<Dependency> {
    nodes
        .iter()
        .map(|node| {
            let component = component_builder
                .get_component(&node.package.id)
                .expect("All nodes should have been added to map");

            let immediate_deps = &node.package.dependencies;
            let optional_deps = node.package.optional_dependencies.values().flatten();
            let dep_groups = node.package.dependency_groups.values().flatten();

            let package_deps = immediate_deps
                .iter()
                .chain(optional_deps)
                .chain(dep_groups)
                .filter_map(|dep| component_builder.get_component(&dep.package_id));

            let bom_refs = package_deps
                .map(|p| p.bom_ref.clone().expect("bom-ref should always exist"))
                .sorted_unstable()
                .unique()
                .collect();

            Dependency {
                dependency_ref: component
                    .bom_ref
                    .clone()
                    .expect("bom-ref should always exist"),
                dependencies: bom_refs,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PackageType<'a> {
    Root,
    Workspace(&'a Path),
    Dependency,
}
