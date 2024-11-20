use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::path::{Component, Path, PathBuf};

use either::Either;
use petgraph::visit::IntoNodeReferences;
use petgraph::Graph;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use url::Url;

use uv_configuration::{DevGroupsManifest, EditableMode, ExtrasSpecification, InstallOptions};
use uv_distribution_filename::{DistExtension, SourceDistExtension};
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::ExtraName;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitUrl};

use crate::graph_ops::marker_reachability;
use crate::lock::{Package, PackageId, Source};
use crate::{InstallTarget, LockError};

/// An export of a [`Lock`] that renders in `requirements.txt` format.
#[derive(Debug)]
pub struct RequirementsTxtExport<'lock> {
    nodes: Vec<Requirement<'lock>>,
    hashes: bool,
    editable: EditableMode,
}

impl<'lock> RequirementsTxtExport<'lock> {
    pub fn from_lock(
        target: InstallTarget<'lock>,
        extras: &ExtrasSpecification,
        dev: &DevGroupsManifest,
        editable: EditableMode,
        hashes: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        let size_guess = target.lock().packages.len();
        let mut petgraph = Graph::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();

        let root = petgraph.add_node(Node::Root);

        // Add the workspace package to the queue.
        for root_name in target.packages() {
            let dist = target
                .lock()
                .find_by_name(root_name)
                .expect("found too many packages matching root")
                .expect("could not find root");

            if dev.prod() {
                // Add the workspace package to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dist.id) {
                    entry.insert(petgraph.add_node(Node::Package(dist)));
                }

                // Add an edge from the root.
                let index = inverse[&dist.id];
                petgraph.add_edge(root, index, MarkerTree::TRUE);

                // Push its dependencies on the queue.
                queue.push_back((dist, None));
                match extras {
                    ExtrasSpecification::None => {}
                    ExtrasSpecification::All => {
                        for extra in dist.optional_dependencies.keys() {
                            queue.push_back((dist, Some(extra)));
                        }
                    }
                    ExtrasSpecification::Some(extras) => {
                        for extra in extras {
                            queue.push_back((dist, Some(extra)));
                        }
                    }
                }
            }

            // Add any development dependencies.
            for dep in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if dev.contains(group) {
                        Some(deps)
                    } else {
                        None
                    }
                })
                .flatten()
            {
                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dep.package_id) {
                    entry.insert(petgraph.add_node(Node::Package(dep_dist)));
                }

                // Add an edge from the root. Development dependencies may be installed without
                // installing the workspace package itself (which can never have markers on it
                // anyway), so they're directly connected to the root.
                let dep_index = inverse[&dep.package_id];
                petgraph.add_edge(
                    root,
                    dep_index,
                    dep.simplified_marker.as_simplified_marker_tree().clone(),
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
                    }
                }
            }
        }

        // Create all the relevant nodes.
        while let Some((package, extra)) = queue.pop_front() {
            let index = inverse[&package.id];

            let deps = if let Some(extra) = extra {
                Either::Left(
                    package
                        .optional_dependencies
                        .get(extra)
                        .into_iter()
                        .flatten(),
                )
            } else {
                Either::Right(package.dependencies.iter())
            };

            for dep in deps {
                let dep_dist = target.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dep.package_id) {
                    entry.insert(petgraph.add_node(Node::Package(dep_dist)));
                }

                // Add the edge.
                let dep_index = inverse[&dep.package_id];
                petgraph.add_edge(
                    index,
                    dep_index,
                    dep.simplified_marker.as_simplified_marker_tree().clone(),
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
                    }
                }
            }
        }

        let mut reachability = marker_reachability(&petgraph, &[]);

        // Collect all packages.
        let mut nodes = petgraph
            .node_references()
            .filter_map(|(index, node)| match node {
                Node::Root => None,
                Node::Package(package) => Some((index, package)),
            })
            .filter(|(_index, package)| {
                install_options.include_package(
                    &package.id.name,
                    target.project_name(),
                    target.lock().members(),
                )
            })
            .map(|(index, package)| Requirement {
                package,
                marker: reachability.remove(&index).unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        // Sort the nodes, such that unnamed URLs (editables) appear at the top.
        nodes.sort_unstable_by(|a, b| {
            RequirementComparator::from(a.package).cmp(&RequirementComparator::from(b.package))
        });

        Ok(Self {
            nodes,
            hashes,
            editable,
        })
    }
}

impl std::fmt::Display for RequirementsTxtExport<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write out each package.
        for Requirement { package, marker } in &self.nodes {
            match &package.id.source {
                Source::Registry(_) => {
                    write!(f, "{}=={}", package.id.name, package.id.version)?;
                }
                Source::Git(url, git) => {
                    // Remove the fragment and query from the URL; they're already present in the
                    // `GitSource`.
                    let mut url = url.to_url();
                    url.set_fragment(None);
                    url.set_query(None);

                    // Reconstruct the `GitUrl` from the `GitSource`.
                    let git_url = uv_git::GitUrl::from_commit(
                        url,
                        GitReference::from(git.kind.clone()),
                        git.precise,
                    );

                    // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                    let url = Url::from(ParsedGitUrl {
                        url: git_url.clone(),
                        subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                    });

                    write!(f, "{} @ {}", package.id.name, url)?;
                }
                Source::Direct(url, direct) => {
                    let subdirectory = direct.subdirectory.as_ref().map(PathBuf::from);
                    let url = Url::from(ParsedArchiveUrl {
                        url: url.to_url(),
                        subdirectory: subdirectory.clone(),
                        ext: DistExtension::Source(SourceDistExtension::TarGz),
                    });
                    write!(f, "{} @ {}", package.id.name, url)?;
                }
                Source::Path(path) | Source::Directory(path) => {
                    if path.is_absolute() {
                        write!(f, "{}", Url::from_file_path(path).unwrap())?;
                    } else {
                        write!(f, "{}", anchor(path).portable_display())?;
                    }
                }
                Source::Editable(path) => match self.editable {
                    EditableMode::Editable => {
                        write!(f, "-e {}", anchor(path).portable_display())?;
                    }
                    EditableMode::NonEditable => {
                        if path.is_absolute() {
                            write!(f, "{}", Url::from_file_path(path).unwrap())?;
                        } else {
                            write!(f, "{}", anchor(path).portable_display())?;
                        }
                    }
                },
                Source::Virtual(_) => {
                    continue;
                }
            }

            if let Some(contents) = marker.contents() {
                write!(f, " ; {contents}")?;
            }

            if self.hashes {
                let mut hashes = package.hashes();
                hashes.sort_unstable();
                if !hashes.is_empty() {
                    for hash in &hashes {
                        writeln!(f, " \\")?;
                        write!(f, "    --hash=")?;
                        write!(f, "{hash}")?;
                    }
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

/// A node in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node<'lock> {
    Root,
    Package(&'lock Package),
}

/// A flat requirement, with its associated marker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Requirement<'lock> {
    package: &'lock Package,
    marker: MarkerTree,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RequirementComparator<'lock> {
    Editable(&'lock Path),
    Path(&'lock Path),
    Package(&'lock PackageId),
}

impl<'lock> From<&'lock Package> for RequirementComparator<'lock> {
    fn from(value: &'lock Package) -> Self {
        match &value.id.source {
            Source::Path(path) | Source::Directory(path) => Self::Path(path),
            Source::Editable(path) => Self::Editable(path),
            _ => Self::Package(&value.id),
        }
    }
}

/// Modify a relative [`Path`] to anchor it at the current working directory.
///
/// For example, given `foo/bar`, returns `./foo/bar`.
fn anchor(path: &Path) -> Cow<'_, Path> {
    match path.components().next() {
        None => Cow::Owned(PathBuf::from(".")),
        Some(Component::CurDir | Component::ParentDir) => Cow::Borrowed(path),
        _ => Cow::Owned(PathBuf::from("./").join(path)),
    }
}
