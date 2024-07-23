use distribution_types::{InstalledDist, Name};
use uv_installer::SitePackages;
use uv_python::PythonEnvironment;
use uv_tool::entrypoint_paths;

/// Return all packages which contain an executable with the given name.
pub(super) fn matching_packages(
    name: &str,
    environment: &PythonEnvironment,
) -> anyhow::Result<Vec<InstalledDist>> {
    let site_packages = SitePackages::from_environment(environment)?;
    let packages = site_packages
        .iter()
        .filter_map(|package| {
            entrypoint_paths(environment, package.name(), package.version())
                .ok()
                .and_then(|entrypoints| {
                    entrypoints
                        .iter()
                        .any(|entrypoint| {
                            entrypoint
                                .0
                                .strip_suffix(std::env::consts::EXE_SUFFIX)
                                .is_some_and(|stripped| stripped == name)
                        })
                        .then(|| package.clone())
                })
        })
        .collect();

    Ok(packages)
}
