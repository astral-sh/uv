use std::collections::BTreeSet;

use rustc_hash::FxHashSet;
use tracing::debug;

use distribution_types::{Name, Resolution};
use pep508_rs::PackageName;

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub no_install_project: bool,
    pub no_install_workspace: bool,
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

    pub fn filter_resolution(
        &self,
        resolution: Resolution,
        project_name: Option<&PackageName>,
        members: &BTreeSet<PackageName>,
    ) -> Resolution {
        // If `--no-install-project` is set, remove the project itself.
        let resolution = self.apply_no_install_project(resolution, project_name);

        // If `--no-install-workspace` is set, remove the project and any workspace members.
        let resolution = self.apply_no_install_workspace(resolution, members);

        // If `--no-install-package` is provided, remove the requested packages.
        self.apply_no_install_package(resolution)
    }

    fn apply_no_install_project(
        &self,
        resolution: Resolution,
        project_name: Option<&PackageName>,
    ) -> Resolution {
        if !self.no_install_project {
            return resolution;
        }

        let Some(project_name) = project_name else {
            debug!("Ignoring `--no-install-project` for virtual workspace");
            return resolution;
        };

        resolution.filter(|dist| dist.name() != project_name)
    }

    fn apply_no_install_workspace(
        &self,
        resolution: Resolution,
        members: &BTreeSet<PackageName>,
    ) -> Resolution {
        if !self.no_install_workspace {
            return resolution;
        }

        resolution.filter(|dist| !members.contains(dist.name()))
    }

    fn apply_no_install_package(&self, resolution: Resolution) -> Resolution {
        if self.no_install_package.is_empty() {
            return resolution;
        }

        let no_install_packages = self.no_install_package.iter().collect::<FxHashSet<_>>();

        resolution.filter(|dist| !no_install_packages.contains(dist.name()))
    }

    /// Returns `true` if a package passes the install filters.
    pub fn include_package(
        &self,
        package: &PackageName,
        project_name: &PackageName,
        members: &BTreeSet<PackageName>,
    ) -> bool {
        // If `--no-install-project` is set, remove the project itself. The project is always
        // part of the workspace.
        if (self.no_install_project || self.no_install_workspace) && package == project_name {
            return false;
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
