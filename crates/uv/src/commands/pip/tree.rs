use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use tracing::debug;

use distribution_types::{Diagnostic, InstalledDist, Name};
use pep508_rs::{MarkerEnvironment, Requirement};
use pypi_types::VerbatimParsedUrl;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_toolchain::EnvironmentPreference;
use uv_toolchain::PythonEnvironment;
use uv_toolchain::ToolchainRequest;

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
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(ToolchainRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    debug!(
        "Using Python {} environment at {}",
        environment.interpreter().python_version(),
        environment.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;

    let rendered_tree = DisplayDependencyGraph::new(
        &site_packages,
        depth.into(),
        prune,
        package,
        no_dedupe,
        invert,
        environment.interpreter().markers(),
    )?
    .render()?
    .join("\n");

    writeln!(printer.stdout(), "{rendered_tree}")?;

    if rendered_tree.contains('*') {
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

/// Filter out all required packages of the given distribution if they
/// are required by an extra.
///
/// For example, `requests==2.32.3` requires `charset-normalizer`, `idna`, `urllib`, and `certifi` at
/// all times, `PySocks` on `socks` extra and `chardet` on `use_chardet_on_py3` extra.
/// This function will return `["charset-normalizer", "idna", "urllib", "certifi"]` for `requests`.
fn filtered_requirements<'env>(
    dist: &'env InstalledDist,
    markers: &'env MarkerEnvironment,
) -> Result<impl Iterator<Item = Requirement<VerbatimParsedUrl>> + 'env> {
    Ok(dist
        .metadata()?
        .requires_dist
        .into_iter()
        .filter(|requirement| {
            requirement
                .marker
                .as_ref()
                .map_or(true, |m| m.evaluate(markers, &[]))
        }))
}

#[derive(Debug)]
struct DisplayDependencyGraph<'env> {
    // Installed packages.
    site_packages: &'env SitePackages,
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
    requirements: HashMap<PackageName, Vec<PackageName>>,
}

impl<'env> DisplayDependencyGraph<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    fn new(
        site_packages: &'env SitePackages,
        depth: usize,
        prune: Vec<PackageName>,
        package: Vec<PackageName>,
        no_dedupe: bool,
        invert: bool,
        markers: &'env MarkerEnvironment,
    ) -> Result<DisplayDependencyGraph<'env>> {
        let mut requirements: HashMap<_, Vec<_>> = HashMap::new();

        // Add all transitive requirements.
        for site_package in site_packages.iter() {
            for required in filtered_requirements(site_package, markers)? {
                if invert {
                    requirements
                        .entry(required.name.clone())
                        .or_default()
                        .push(site_package.name().clone());
                } else {
                    requirements
                        .entry(site_package.name().clone())
                        .or_default()
                        .push(required.name.clone());
                }
            }
        }

        Ok(Self {
            site_packages,
            depth,
            prune,
            package,
            no_dedupe,
            requirements,
        })
    }

    /// Perform a depth-first traversal of the given distribution and its dependencies.
    fn visit(
        &self,
        installed_dist: &'env InstalledDist,
        visited: &mut FxHashMap<&'env PackageName, Vec<PackageName>>,
        path: &mut Vec<&'env PackageName>,
    ) -> Result<Vec<String>> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Ok(Vec::new());
        }

        let package_name = installed_dist.name();
        let line = format!("{} v{}", package_name, installed_dist.version());

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(package_name) {
            if !self.no_dedupe || path.contains(&package_name) {
                return Ok(if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{} (*)", line)]
                });
            }
        }

        let requirements = self
            .requirements
            .get(installed_dist.name())
            .into_iter()
            .flatten()
            .filter(|req| {
                // Skip if the current package is not one of the installed distributions.
                !self.prune.contains(req) && self.site_packages.contains_package(req)
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

            for distribution in self.site_packages.get_packages(req) {
                for (visited_index, visited_line) in
                    self.visit(distribution, visited, path)?.iter().enumerate()
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

        Ok(lines)
    }

    /// Depth-first traverse the nodes to render the tree.
    fn render(&self) -> Result<Vec<String>> {
        let mut visited: FxHashMap<&PackageName, Vec<PackageName>> = FxHashMap::default();
        let mut path: Vec<&PackageName> = Vec::new();
        let mut lines: Vec<String> = Vec::new();

        if self.package.is_empty() {
            // The root nodes are those that are not required by any other package.
            let children: HashSet<_> = self.requirements.values().flatten().collect();
            for site_package in self.site_packages.iter() {
                // If the current package is not required by any other package, start the traversal
                // with the current package as the root.
                if !children.contains(site_package.name()) {
                    path.clear();
                    lines.extend(self.visit(site_package, &mut visited, &mut path)?);
                }
            }
        } else {
            for (index, package) in self.package.iter().enumerate() {
                if index != 0 {
                    lines.push(String::new());
                }
                for installed_dist in self.site_packages.get_packages(package) {
                    path.clear();
                    lines.extend(self.visit(installed_dist, &mut visited, &mut path)?);
                }
            }
        }
        Ok(lines)
    }
}
