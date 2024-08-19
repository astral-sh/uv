use std::borrow::Cow;
use std::collections::BTreeSet;

use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use pypi_types::ResolverMarkerEnvironment;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::lock::{Dependency, PackageId};
use crate::Lock;

#[derive(Debug)]
pub struct TreeDisplay<'env> {
    /// The root nodes in the [`Lock`].
    roots: Vec<&'env PackageId>,
    /// The edges in the [`Lock`].
    ///
    /// While the dependencies exist on the [`Lock`] directly, if `--invert` is enabled, the
    /// direction must be inverted when constructing the tree.
    dependencies: FxHashMap<&'env PackageId, Vec<Cow<'env, Dependency>>>,
    optional_dependencies:
        FxHashMap<&'env PackageId, FxHashMap<ExtraName, Vec<Cow<'env, Dependency>>>>,
    dev_dependencies: FxHashMap<&'env PackageId, FxHashMap<GroupName, Vec<Cow<'env, Dependency>>>>,
    /// Maximum display depth of the dependency tree
    depth: usize,
    /// Prune the given packages from the display of the dependency tree.
    prune: Vec<PackageName>,
    /// Display only the specified packages.
    package: Vec<PackageName>,
    /// Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,
}

impl<'env> TreeDisplay<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed packages.
    pub fn new(
        lock: &'env Lock,
        markers: Option<&'env ResolverMarkerEnvironment>,
        depth: usize,
        prune: Vec<PackageName>,
        package: Vec<PackageName>,
        no_dedupe: bool,
        invert: bool,
    ) -> Self {
        let mut non_roots = FxHashSet::default();

        // Index all the dependencies. We could read these from the `Lock` directly, but we have to
        // support `--invert`, so we might as well build them up in either case.
        let mut dependencies: FxHashMap<_, Vec<_>> = FxHashMap::default();
        let mut optional_dependencies: FxHashMap<_, FxHashMap<_, Vec<_>>> = FxHashMap::default();
        let mut dev_dependencies: FxHashMap<_, FxHashMap<_, Vec<_>>> = FxHashMap::default();

        for packages in &lock.packages {
            for dependency in &packages.dependencies {
                let parent = if invert {
                    &dependency.package_id
                } else {
                    &packages.id
                };
                let child = if invert {
                    Cow::Owned(Dependency {
                        package_id: packages.id.clone(),
                        extra: dependency.extra.clone(),
                        simplified_marker: dependency.simplified_marker.clone(),
                        complexified_marker: dependency.complexified_marker.clone(),
                    })
                } else {
                    Cow::Borrowed(dependency)
                };

                non_roots.insert(child.package_id.clone());

                // Skip dependencies that don't apply to the current environment.
                if let Some(environment_markers) = markers {
                    if !dependency
                        .complexified_marker
                        .evaluate(environment_markers, &[])
                    {
                        continue;
                    }
                }

                dependencies.entry(parent).or_default().push(child);
            }

            for (extra, dependencies) in &packages.optional_dependencies {
                for dependency in dependencies {
                    let parent = if invert {
                        &dependency.package_id
                    } else {
                        &packages.id
                    };
                    let child = if invert {
                        Cow::Owned(Dependency {
                            package_id: packages.id.clone(),
                            extra: dependency.extra.clone(),
                            simplified_marker: dependency.simplified_marker.clone(),
                            complexified_marker: dependency.complexified_marker.clone(),
                        })
                    } else {
                        Cow::Borrowed(dependency)
                    };

                    non_roots.insert(child.package_id.clone());

                    // Skip dependencies that don't apply to the current environment.
                    if let Some(environment_markers) = markers {
                        if !dependency
                            .complexified_marker
                            .evaluate(environment_markers, &[])
                        {
                            continue;
                        }
                    }

                    optional_dependencies
                        .entry(parent)
                        .or_default()
                        .entry(extra.clone())
                        .or_default()
                        .push(child);
                }
            }

            for (group, dependencies) in &packages.dev_dependencies {
                for dependency in dependencies {
                    let parent = if invert {
                        &dependency.package_id
                    } else {
                        &packages.id
                    };
                    let child = if invert {
                        Cow::Owned(Dependency {
                            package_id: packages.id.clone(),
                            extra: dependency.extra.clone(),
                            simplified_marker: dependency.simplified_marker.clone(),
                            complexified_marker: dependency.complexified_marker.clone(),
                        })
                    } else {
                        Cow::Borrowed(dependency)
                    };

                    non_roots.insert(child.package_id.clone());

                    // Skip dependencies that don't apply to the current environment.
                    if let Some(environment_markers) = markers {
                        if !dependency
                            .complexified_marker
                            .evaluate(environment_markers, &[])
                        {
                            continue;
                        }
                    }

                    dev_dependencies
                        .entry(parent)
                        .or_default()
                        .entry(group.clone())
                        .or_default()
                        .push(child);
                }
            }
        }

        // Compute the root nodes.
        let roots = lock
            .packages
            .iter()
            .map(|dist| &dist.id)
            .filter(|id| !non_roots.contains(*id))
            .collect::<Vec<_>>();

        Self {
            roots,
            dependencies,
            optional_dependencies,
            dev_dependencies,
            depth,
            prune,
            package,
            no_dedupe,
        }
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    fn visit(
        &'env self,
        node: Node<'env>,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let line = {
            let mut line = format!("{}", node.package_id().name);

            if let Some(extras) = node.extras().filter(|extras| !extras.is_empty()) {
                line.push_str(&format!("[{}]", extras.iter().join(",")));
            }

            line.push_str(&format!(" v{}", node.package_id().version));

            match node {
                Node::Root(_) => line,
                Node::Dependency(_) => line,
                Node::OptionalDependency(extra, _) => format!("{line} (extra: {extra})"),
                Node::DevDependency(group, _) => format!("{line} (group: {group})"),
            }
        };

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(node.package_id()) {
            if !self.no_dedupe || path.contains(&node.package_id()) {
                return if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{} (*)", line)]
                };
            }
        }

        let dependencies: Vec<Node<'env>> = self
            .dependencies
            .get(node.package_id())
            .into_iter()
            .flatten()
            .map(|dep| Node::Dependency(dep.as_ref()))
            .chain(
                self.optional_dependencies
                    .get(node.package_id())
                    .into_iter()
                    .flatten()
                    .flat_map(|(extra, deps)| {
                        deps.iter()
                            .map(move |dep| Node::OptionalDependency(extra, dep))
                    }),
            )
            .chain(
                self.dev_dependencies
                    .get(node.package_id())
                    .into_iter()
                    .flatten()
                    .flat_map(|(group, deps)| {
                        deps.iter().map(move |dep| Node::DevDependency(group, dep))
                    }),
            )
            .filter(|dep| !self.prune.contains(&dep.package_id().name))
            .collect::<Vec<_>>();

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(
            node.package_id(),
            dependencies.iter().map(Node::package_id).collect(),
        );
        path.push(node.package_id());

        for (index, dep) in dependencies.iter().enumerate() {
            // For sub-visited packages, add the prefix to make the tree display user-friendly.
            // The key observation here is you can group the tree as follows when you're at the
            // root of the tree:
            // root_package
            // ├── level_1_0          // Group 1
            // │   ├── level_2_0      ...
            // │   │   ├── level_3_0  ...
            // │   │   └── level_3_1  ...
            // │   └── level_2_1      ...
            // ├── level_1_1          // Group 2
            // │   ├── level_2_2      ...
            // │   └── level_2_3      ...
            // └── level_1_2          // Group 3
            //     └── level_2_4      ...
            //
            // The lines in Group 1 and 2 have `├── ` at the top and `|   ` at the rest while
            // those in Group 3 have `└── ` at the top and `    ` at the rest.
            // This observation is true recursively even when looking at the subtree rooted
            // at `level_1_0`.
            let (prefix_top, prefix_rest) = if dependencies.len() - 1 == index {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };
            for (visited_index, visited_line) in self.visit(*dep, visited, path).iter().enumerate()
            {
                let prefix = if visited_index == 0 {
                    prefix_top
                } else {
                    prefix_rest
                };
                lines.push(format!("{prefix}{visited_line}"));
            }
        }

        path.pop();

        lines
    }

    /// Depth-first traverse the nodes to render the tree.
    fn render(&self) -> Vec<String> {
        let mut visited = FxHashMap::default();
        let mut path = Vec::new();
        let mut lines = Vec::new();

        if self.package.is_empty() {
            for id in &self.roots {
                path.clear();
                lines.extend(self.visit(Node::Root(id), &mut visited, &mut path));
            }
        } else {
            let by_package: FxHashMap<_, _> = self.roots.iter().map(|id| (&id.name, id)).collect();
            let mut first = true;
            for package in &self.package {
                if std::mem::take(&mut first) {
                    lines.push(String::new());
                }
                if let Some(id) = by_package.get(package) {
                    path.clear();
                    lines.extend(self.visit(Node::Root(id), &mut visited, &mut path));
                }
            }
        }

        lines
    }
}

#[derive(Debug, Copy, Clone)]
enum Node<'env> {
    Root(&'env PackageId),
    Dependency(&'env Dependency),
    OptionalDependency(&'env ExtraName, &'env Dependency),
    DevDependency(&'env GroupName, &'env Dependency),
}

impl<'env> Node<'env> {
    fn package_id(&self) -> &'env PackageId {
        match self {
            Self::Root(id) => id,
            Self::Dependency(dep) => &dep.package_id,
            Self::OptionalDependency(_, dep) => &dep.package_id,
            Self::DevDependency(_, dep) => &dep.package_id,
        }
    }

    fn extras(&self) -> Option<&BTreeSet<ExtraName>> {
        match self {
            Self::Root(_) => None,
            Self::Dependency(dep) => Some(&dep.extra),
            Self::OptionalDependency(_, dep) => Some(&dep.extra),
            Self::DevDependency(_, dep) => Some(&dep.extra),
        }
    }
}

impl std::fmt::Display for TreeDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use owo_colors::OwoColorize;

        let mut deduped = false;
        for line in self.render() {
            deduped |= line.contains('*');
            writeln!(f, "{line}")?;
        }

        if deduped {
            let message = if self.no_dedupe {
                "(*) Package tree is a cycle and cannot be shown".italic()
            } else {
                "(*) Package tree already displayed".italic()
            };
            writeln!(f, "{message}")?;
        }

        Ok(())
    }
}
