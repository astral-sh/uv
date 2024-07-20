use std::fmt::Write;

use anyhow::Result;
use indexmap::IndexMap;
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::debug;

use distribution_types::{Diagnostic, Name};
use pep508_rs::MarkerEnvironment;
use pypi_types::RequirementSource;
use uv_cache::Cache;
use uv_distribution::Metadata;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_python::EnvironmentPreference;
use uv_python::PythonEnvironment;
use uv_python::PythonRequest;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Display the installed packages in the current environment as a dependency tree.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) fn pip_tree(
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    show_version_specifiers: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    debug!(
        "Using Python {} environment at {}",
        environment.interpreter().python_version(),
        environment.python_executable().user_display().cyan()
    );

    // Read packages from the virtual environment.
    let site_packages = SitePackages::from_environment(&environment)?;
    let mut packages: IndexMap<_, Vec<_>> = IndexMap::new();
    for package in site_packages.iter() {
        let metadata = Metadata::from_metadata23(package.metadata()?);
        packages
            .entry(package.name().clone())
            .or_default()
            .push(metadata);
    }

    // Render the tree.
    let rendered_tree = DisplayDependencyGraph::new(
        depth.into(),
        prune,
        package,
        no_dedupe,
        invert,
        show_version_specifiers,
        environment.interpreter().markers(),
        packages,
    )
    .render()
    .join("\n");

    writeln!(printer.stdout(), "{rendered_tree}")?;

    if rendered_tree.contains("(*)") {
        let message = if no_dedupe {
            "(*) Package tree is a cycle and cannot be shown".italic()
        } else {
            "(*) Package tree already displayed".italic()
        };
        writeln!(printer.stdout(), "{message}")?;
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
pub(crate) struct DisplayDependencyGraph {
    packages: IndexMap<PackageName, Vec<Metadata>>,
    /// Maximum display depth of the dependency tree
    depth: usize,
    /// Prune the given packages from the display of the dependency tree.
    prune: Vec<PackageName>,
    /// Display only the specified packages.
    package: Vec<PackageName>,
    /// Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,
    /// Map from package name to its requirements.
    ///
    /// If `--invert` is given the map is inverted.
    requirements: FxHashMap<PackageName, Vec<PackageName>>,
    /// Map from requirement package name-to-parent-to-dependency metadata.
    dependencies: FxHashMap<PackageName, FxHashMap<PackageName, Dependency>>,
}

impl DisplayDependencyGraph {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    pub(crate) fn new(
        depth: usize,
        prune: Vec<PackageName>,
        package: Vec<PackageName>,
        no_dedupe: bool,
        invert: bool,
        show_version_specifiers: bool,
        markers: &MarkerEnvironment,
        packages: IndexMap<PackageName, Vec<Metadata>>,
    ) -> Self {
        let mut requirements: FxHashMap<_, Vec<_>> = FxHashMap::default();
        let mut dependencies: FxHashMap<PackageName, FxHashMap<PackageName, Dependency>> =
            FxHashMap::default();

        // Add all transitive requirements.
        for metadata in packages.values().flatten() {
            // Ignore any optional dependencies.
            for required in metadata.requires_dist.iter().filter(|requirement| {
                requirement
                    .marker
                    .as_ref()
                    .map_or(true, |m| m.evaluate(markers, &[]))
            }) {
                let dependency = if invert {
                    Dependency::Inverted(
                        required.name.clone(),
                        metadata.name.clone(),
                        required.source.clone(),
                    )
                } else {
                    Dependency::Normal(
                        metadata.name.clone(),
                        required.name.clone(),
                        required.source.clone(),
                    )
                };

                requirements
                    .entry(dependency.parent().clone())
                    .or_default()
                    .push(dependency.child().clone());

                if show_version_specifiers {
                    dependencies
                        .entry(dependency.parent().clone())
                        .or_default()
                        .insert(dependency.child().clone(), dependency);
                }
            }
        }
        Self {
            packages,
            depth,
            prune,
            package,
            no_dedupe,
            requirements,
            dependencies,
        }
    }

    /// Perform a depth-first traversal of the given distribution and its dependencies.
    fn visit<'env>(
        &'env self,
        metadata: &'env Metadata,
        visited: &mut FxHashMap<&'env PackageName, Vec<PackageName>>,
        path: &mut Vec<&'env PackageName>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let package_name = &metadata.name;
        let mut line = format!("{} v{}", package_name, metadata.version);

        // If the current package is not top-level (i.e., it has a parent), include the specifiers.
        if let Some(last) = path.last().copied() {
            if let Some(dependency) = self
                .dependencies
                .get(last)
                .and_then(|deps| deps.get(package_name))
            {
                line.push(' ');
                line.push_str(&format!("[{dependency}]"));
            }
        }

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(package_name) {
            if !self.no_dedupe || path.contains(&package_name) {
                return if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{} (*)", line)]
                };
            }
        }

        let requirements = self
            .requirements
            .get(package_name)
            .into_iter()
            .flatten()
            .filter(|&req| {
                // Skip if the current package is not one of the installed distributions.
                !self.prune.contains(req) && self.packages.contains_key(req)
            })
            .cloned()
            .collect::<Vec<_>>();

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(package_name, requirements.clone());
        path.push(package_name);

        for (index, req) in requirements.iter().enumerate() {
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
            let (prefix_top, prefix_rest) = if requirements.len() - 1 == index {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };

            for distribution in self.packages.get(req).into_iter().flatten() {
                for (visited_index, visited_line) in
                    self.visit(distribution, visited, path).iter().enumerate()
                {
                    let prefix = if visited_index == 0 {
                        prefix_top
                    } else {
                        prefix_rest
                    };

                    lines.push(format!("{prefix}{visited_line}"));
                }
            }
        }
        path.pop();

        lines
    }

    /// Depth-first traverse the nodes to render the tree.
    pub(crate) fn render(&self) -> Vec<String> {
        let mut visited: FxHashMap<&PackageName, Vec<PackageName>> = FxHashMap::default();
        let mut path: Vec<&PackageName> = Vec::new();
        let mut lines: Vec<String> = Vec::new();

        if self.package.is_empty() {
            // The root nodes are those that are not required by any other package.
            let children: FxHashSet<_> = self.requirements.values().flatten().collect();
            for package in self.packages.values().flatten() {
                // If the current package is not required by any other package, start the traversal
                // with the current package as the root.
                if !children.contains(&package.name) {
                    path.clear();
                    lines.extend(self.visit(package, &mut visited, &mut path));
                }
            }
        } else {
            for (index, package) in self.package.iter().enumerate() {
                if index != 0 {
                    lines.push(String::new());
                }

                for package in self.packages.get(package).into_iter().flatten() {
                    path.clear();
                    lines.extend(self.visit(package, &mut visited, &mut path));
                }
            }
        }

        lines
    }
}

#[derive(Debug)]
enum Dependency {
    /// Show dependencies from parent to the child package that it requires.
    Normal(PackageName, PackageName, RequirementSource),
    /// Show dependencies from the child package to the parent that requires it.
    Inverted(PackageName, PackageName, RequirementSource),
}

impl Dependency {
    /// Return the parent in the tree.
    fn parent(&self) -> &PackageName {
        match self {
            Self::Normal(parent, _, _) => parent,
            Self::Inverted(parent, _, _) => parent,
        }
    }

    /// Return the child in the tree.
    fn child(&self) -> &PackageName {
        match self {
            Self::Normal(_, child, _) => child,
            Self::Inverted(_, child, _) => child,
        }
    }
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal(_, _, source) => {
                let version = match source.version_or_url() {
                    None => "*".to_string(),
                    Some(version) => version.to_string(),
                };
                write!(f, "required: {version}")
            }
            Self::Inverted(parent, _, source) => {
                let version = match source.version_or_url() {
                    None => "*".to_string(),
                    Some(version) => version.to_string(),
                };
                write!(f, "requires: {parent} {version}")
            }
        }
    }
}
