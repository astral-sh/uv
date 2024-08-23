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
}
