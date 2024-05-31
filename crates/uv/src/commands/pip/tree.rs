use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use pypi_types::VerbatimParsedUrl;
use tracing::debug;

use distribution_types::{Diagnostic, InstalledDist, Name};
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::{PythonEnvironment, SystemPython};
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Display the installed packages in the current environment as a dependency tree.
pub(crate) fn pip_tree(
    strict: bool,
    python: Option<&str>,
    system: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let system = if system {
        SystemPython::Required
    } else {
        SystemPython::Allowed
    };
    let venv = PythonEnvironment::find(python, system, preview, cache)?;

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;
    DisplayDependencyGraph::new(&site_packages, printer).render();
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
        line.push_str("└──");
        line.push(' ');
    }
    line.push_str((*installed_dist.name()).to_string().as_str());
    line.push_str(" v");
    line.push_str((*installed_dist.version()).to_string().as_str());

    if is_visited {
        line.push_str(" (*)");
    }
    line
}
#[derive(Debug)]
struct DisplayDependencyGraph<'a> {
    // Map from package name to the installed distribution.
    package_index: HashMap<&'a PackageName, &'a InstalledDist>,
    site_packages: &'a SitePackages,
    non_root_packages: HashSet<PackageName>,
    printer: Printer,
}

impl<'a> DisplayDependencyGraph<'a> {
    /// Create a new [`DisplayDependencyGraph`] for the given graph.
    fn new(site_packages: &'a SitePackages, printer: Printer) -> DisplayDependencyGraph<'a> {
        let mut package_index = HashMap::new();
        let mut non_root_packages = HashSet::new();
        for site_package in site_packages.iter() {
            package_index.insert(site_package.name(), site_package);
        }
        for site_package in site_packages.iter() {
            for required in required_with_no_extra(site_package) {
                non_root_packages.insert(required.name.clone());
            }
        }
        Self {
            package_index,
            site_packages,
            non_root_packages,
            printer,
        }
    }

    // Visit and print the given installed distribution and those required by it.
    fn visit(&self, installed_dist: &InstalledDist, indent: usize, visited: &mut HashSet<String>) {
        let is_visited = visited.contains(&installed_dist.name().to_string());
        let line = render_line(installed_dist, indent, is_visited);
        writeln!(self.printer.stdout(), "{line}").unwrap();
        if is_visited {
            return;
        }
        visited.insert(installed_dist.name().to_string());
        for required in &required_with_no_extra(installed_dist) {
            if self.package_index.contains_key(&required.name) {
                self.visit(self.package_index[&required.name], indent + 1, visited);
            }
        }
    }

    // Recursively visit the nodes to render the tree.
    // The starting nodes are the ones without incoming edges.
    fn render(&self) {
        let mut visited: HashSet<String> = HashSet::new();
        for site_package in self.site_packages.iter() {
            if !self.non_root_packages.contains(site_package.name()) {
                self.visit(site_package, 0, &mut visited);
            }
        }
    }
}
