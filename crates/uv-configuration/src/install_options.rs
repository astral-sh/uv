use std::collections::BTreeSet;

use tracing::debug;

use pep508_rs::PackageName;

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Omit the project itself from the resolution.
    pub no_install_project: bool,
    /// Omit all workspace members (including the project itself) from the resolution.
    pub no_install_workspace: bool,
    /// Omit the specified packages from the resolution.
    pub no_install_package: Vec<PackageName>,
}

impl InstallOptions {
    pub fn new(
        no_install_project: bool,
        no_install_workspace: bool,
        no_install_package: Vec<PackageName>,
    ) -> Self {
        Self {
            no_install_project,
            no_install_workspace,
            no_install_package,
        }
    }

    /// Returns `true` if a package passes the install filters.
    pub fn include_package(
        &self,
        package: &PackageName,
        project_name: Option<&PackageName>,
        members: &BTreeSet<PackageName>,
    ) -> bool {
        // If `--no-install-project` is set, remove the project itself. The project is always
        // part of the workspace.
        if self.no_install_project || self.no_install_workspace {
            if let Some(project_name) = project_name {
                if package == project_name {
                    return false;
                }
            } else {
                debug!("Ignoring `--no-install-project` for virtual workspace");
            };
        }

        // If `--no-install-workspace` is set, remove the project and any workspace members.
        if self.no_install_workspace && members.contains(package) {
            return false;
        }

        // If `--no-install-package` is provided, remove the requested packages.
        if self.no_install_package.contains(package) {
            return false;
        }

        true
    }
}
