use std::fmt::Write;

use distribution_types::{Diagnostic, InstalledDist, Name};
use owo_colors::OwoColorize;
use tracing::debug;
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_toolchain::EnvironmentPreference;
use uv_toolchain::PythonEnvironment;
use uv_toolchain::ToolchainRequest;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use std::collections::{HashMap, HashSet};

use pypi_types::VerbatimParsedUrl;

/// Display the installed packages in the current environment as a dependency tree.
pub(crate) fn pip_tree(
    strict: bool,
    python: Option<&str>,
    system: bool,
    _preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
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
    let site_packages = SitePackages::from_executable(&environment)?;

    writeln!(
        printer.stdout(),
        "{}",
        DisplayDependencyGraph::new(&site_packages)
            .render()
            .join("\n")
    )
    .unwrap();

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

// Filter out all required packages of the given distribution if they
// are required by an extra.
// For example, `requests==2.32.3` requires `charset-normalizer`, `idna`, `urllib`, and `certifi` at
// all times, `PySocks` on `socks` extra and `chardet` on `use_chardet_on_py3` extra.
// This function will return `["charset-normalizer", "idna", "urllib", "certifi"]` for `requests`.
fn required_with_no_extra(dist: &InstalledDist) -> Vec<pep508_rs::Requirement<VerbatimParsedUrl>> {
    let metadata = dist.metadata().unwrap();
    return metadata
        .requires_dist
        .into_iter()
        .filter(|r| {
            r.marker.is_none()
                || !r
                    .marker
                    .as_ref()
                    .unwrap()
                    .evaluate_optional_environment(None, &metadata.provides_extras[..])
        })
        .collect::<Vec<_>>();
}

// Render the line for the given installed distribution in the dependency tree.
fn render_line(installed_dist: &InstalledDist, indent: usize, is_visited: bool) -> String {
    let mut line = String::new();
    if indent > 0 {
        line.push_str("    ".repeat(indent - 1).as_str());
        line.push_str("└── ");
    }
    write!(
        &mut line,
        "{} v{}",
        installed_dist.name(),
        installed_dist.version()
    )
    .unwrap();

    if is_visited {
        line.push_str(" (*)");
    }
    line
}
#[derive(Debug)]
struct DisplayDependencyGraph<'a> {
    site_packages: &'a SitePackages,
    // Map from package name to the installed distribution.
    dist_by_package_name: HashMap<&'a PackageName, &'a InstalledDist>,
    // Set of package names that are required by at least one installed distribution.
    // It is used to determine the starting nodes when recursing the
    // dependency graph.
    required_packages: HashSet<PackageName>,
}

impl<'a> DisplayDependencyGraph<'a> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    fn new(site_packages: &'a SitePackages) -> DisplayDependencyGraph<'a> {
        let mut dist_by_package_name = HashMap::new();
        let mut required_packages = HashSet::new();
        for site_package in site_packages.iter() {
            dist_by_package_name.insert(site_package.name(), site_package);
        }
        for site_package in site_packages.iter() {
            for required in required_with_no_extra(site_package) {
                required_packages.insert(required.name.clone());
            }
        }

        Self {
            site_packages,
            dist_by_package_name,
            required_packages,
        }
    }

    // Depth-first traversal of the given distribution and its dependencies.
    fn visit(
        &self,
        installed_dist: &InstalledDist,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        let package_name = installed_dist.name().to_string();
        let is_visited = visited.contains(&package_name);
        lines.push(render_line(installed_dist, path.len(), is_visited));
        if is_visited {
            return lines;
        }

        path.push(package_name.clone());
        visited.insert(package_name.clone());
        for required in &required_with_no_extra(installed_dist) {
            if self.dist_by_package_name.contains_key(&required.name) {
                let visited_lines =
                    self.visit(self.dist_by_package_name[&required.name], visited, path);
                lines.extend(visited_lines);
            }
        }
        path.pop();
        lines
    }

    // Depth-first traverse the nodes to render the tree.
    // The starting nodes are the ones without incoming edges.
    fn render(&self) -> Vec<String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut lines: Vec<String> = Vec::new();
        for site_package in self.site_packages.iter() {
            // If the current package is not required by any other package, start the traversal
            // with the current package as the root.
            if !self.required_packages.contains(site_package.name()) {
                lines.extend(self.visit(site_package, &mut visited, &mut Vec::new()));
            }
        }
        lines
    }
}
