use rustc_hash::FxHashSet;
use tracing::debug;

use distribution_types::{Name, Resolution};
use pep508_rs::PackageName;
use uv_workspace::VirtualProject;

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
        project: &VirtualProject,
    ) -> Resolution {
        // If `--no-install-project` is set, remove the project itself.
        let resolution = self.apply_no_install_project(resolution, project);

        // If `--no-install-workspace` is set, remove the project and any workspace members.
        let resolution = self.apply_no_install_workspace(resolution, project);

        // If `--no-install-package` is provided, remove the requested packages.
        self.apply_no_install_package(resolution)
    }

    fn apply_no_install_project(
        &self,
        resolution: Resolution,
        project: &VirtualProject,
    ) -> Resolution {
        if !self.no_install_project {
            return resolution;
        }

        let Some(project_name) = project.project_name() else {
            debug!("Ignoring `--no-install-project` for virtual workspace");
            return resolution;
        };

        resolution.filter(|dist| dist.name() != project_name)
    }

    fn apply_no_install_workspace(
        &self,
        resolution: Resolution,
        project: &VirtualProject,
    ) -> Resolution {
        if !self.no_install_workspace {
            return resolution;
        }

        let workspace_packages = project.workspace().packages();
        resolution.filter(|dist| {
            !workspace_packages.contains_key(dist.name())
                && Some(dist.name()) != project.project_name()
        })
    }

    fn apply_no_install_package(&self, resolution: Resolution) -> Resolution {
        if self.no_install_package.is_empty() {
            return resolution;
        }

        let no_install_packages = self.no_install_package.iter().collect::<FxHashSet<_>>();

        resolution.filter(|dist| !no_install_packages.contains(dist.name()))
    }
}
