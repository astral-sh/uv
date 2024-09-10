use std::collections::hash_map::Entry;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};

use either::Either;
use petgraph::visit::IntoNodeReferences;
use petgraph::{Directed, Graph};
use rustc_hash::{FxHashMap, FxHashSet};
use url::Url;

use distribution_filename::{DistExtension, SourceDistExtension};
use pep508_rs::MarkerTree;
use pypi_types::{ParsedArchiveUrl, ParsedGitUrl};
use uv_configuration::{ExtrasSpecification, InstallOptions};
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::graph_ops::marker_reachability;
use crate::lock::{Package, PackageId, Source};
use crate::{Lock, LockError};

type LockGraph<'lock> = Graph<&'lock Package, Edge, Directed>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node<'lock> {
    package: &'lock Package,
    marker: MarkerTree,
}

/// An export of a [`Lock`] that renders in `requirements.txt` format.
#[derive(Debug)]
pub struct RequirementsTxtExport<'lock> {
    nodes: Vec<Node<'lock>>,
    hashes: bool,
}

impl<'lock> RequirementsTxtExport<'lock> {
    pub fn from_lock(
        lock: &'lock Lock,
        root_name: &PackageName,
        extras: &ExtrasSpecification,
        dev: &[GroupName],
        hashes: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        let size_guess = lock.packages.len();
        let mut petgraph = LockGraph::with_capacity(size_guess, size_guess);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut inverse = FxHashMap::default();

        // Add the workspace package to the queue.
        let root = lock
            .find_by_name(root_name)
            .expect("found too many packages matching root")
            .expect("could not find root");

        // Add the base package.
        queue.push_back((root, None));

        // Add any extras.
        match extras {
            ExtrasSpecification::None => {}
            ExtrasSpecification::All => {
                for extra in root.optional_dependencies.keys() {
                    queue.push_back((root, Some(extra)));
                }
            }
            ExtrasSpecification::Some(extras) => {
                for extra in extras {
                    queue.push_back((root, Some(extra)));
                }
            }
        }

        // Add the root package to the graph.
        inverse.insert(&root.id, petgraph.add_node(root));

        // Create all the relevant nodes.
        let mut seen = FxHashSet::default();

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
                Either::Right(package.dependencies.iter().chain(
                    dev.iter().flat_map(|group| {
                        package.dev_dependencies.get(group).into_iter().flatten()
                    }),
                ))
            };

            for dep in deps {
                let dep_dist = lock.find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                if let Entry::Vacant(entry) = inverse.entry(&dep.package_id) {
                    entry.insert(petgraph.add_node(dep_dist));
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
        let mut nodes: Vec<Node> = petgraph
            .node_references()
            .filter(|(_index, package)| {
                install_options.include_package(&package.id.name, root_name, lock.members())
            })
            .map(|(index, package)| Node {
                package,
                marker: reachability.remove(&index).unwrap_or_default(),
            })
            .collect::<Vec<_>>();

        // Sort the nodes, such that unnamed URLs (editables) appear at the top.
        nodes.sort_unstable_by(|a, b| {
            NodeComparator::from(a.package).cmp(&NodeComparator::from(b.package))
        });

        Ok(Self { nodes, hashes })
    }
}

impl std::fmt::Display for RequirementsTxtExport<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write out each package.
        for Node { package, marker } in &self.nodes {
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
                    if path.as_os_str().is_empty() {
                        write!(f, ".")?;
                    } else if path.is_absolute() {
                        write!(f, "{}", Url::from_file_path(path).unwrap())?;
                    } else {
                        write!(f, "{}", path.portable_display())?;
                    }
                }
                Source::Editable(path) => {
                    if path.as_os_str().is_empty() {
                        write!(f, "-e .")?;
                    } else {
                        write!(f, "-e {}", path.portable_display())?;
                    }
                }
                Source::Virtual(_) => {
                    continue;
                }
            }

            if let Some(contents) = marker.contents() {
                write!(f, " ; {contents}")?;
            }

            if self.hashes {
                let hashes = package.hashes();
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

/// The edges of the [`LockGraph`].
type Edge = MarkerTree;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NodeComparator<'lock> {
    Editable(&'lock Path),
    Path(&'lock Path),
    Package(&'lock PackageId),
}

impl<'lock> From<&'lock Package> for NodeComparator<'lock> {
    fn from(value: &'lock Package) -> Self {
        match &value.id.source {
            Source::Path(path) | Source::Directory(path) => Self::Path(path),
            Source::Editable(path) => Self::Editable(path),
            _ => Self::Package(&value.id),
        }
    }
}
