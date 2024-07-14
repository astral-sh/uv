use distribution_types::{InstalledDist, Name};
use pypi_types::Requirement;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_installer::SitePackages;
use uv_python::{Interpreter, PythonEnvironment};
use uv_requirements::RequirementsSpecification;
use uv_tool::entrypoint_paths;

use crate::commands::{project, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Resolve any [`UnnamedRequirements`].
pub(super) async fn resolve_requirements(
    requirements: impl Iterator<Item = &str>,
    interpreter: &Interpreter,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<Vec<Requirement>> {
    // Parse the requirements.
    let requirements = {
        let mut parsed = vec![];
        for requirement in requirements {
            parsed.push(RequirementsSpecification::parse_package(requirement)?);
        }
        parsed
    };

    // Resolve the parsed requirements.
    project::resolve_names(
        requirements,
        interpreter,
        settings,
        state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
}

/// Return all packages which contain an executable for the given executable name.
pub(super) fn matching_packages(
    name: &str,
    environment: &PythonEnvironment,
) -> anyhow::Result<Vec<InstalledDist>> {
    let site_packages = SitePackages::from_environment(environment)?;
    let entrypoints = site_packages
        .iter()
        .filter_map(|package| {
            match entrypoint_paths(environment, package.name(), package.version()).ok() {
                Some(entrypoints) => {
                    if entrypoints.iter().any(|e| {
                        if let Some(stripped) = e.0.strip_suffix(".exe") {
                            stripped == name
                        } else {
                            e.0 == name
                        }
                    }) {
                        Some(package.clone())
                    } else {
                        None
                    }
                }
                None => None,
            }
        })
        .collect();

    Ok(entrypoints)
}
