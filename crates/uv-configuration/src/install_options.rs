use std::collections::BTreeSet;

use tracing::debug;

use uv_normalize::PackageName;

/// Minimal view of a package used to apply install filters.
#[derive(Debug, Clone, Copy)]
pub struct InstallTarget<'a> {
    /// The package name.
    pub name: &'a PackageName,
    /// Whether the package refers to a local source (path, directory, editable, etc.).
    pub is_local: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Omit the project itself from the resolution.
    pub no_install_project: bool,
    /// Omit all workspace members (including the project itself) from the resolution.
    pub no_install_workspace: bool,
    /// Omit all local packages from the resolution.
    pub no_install_local: bool,
    /// Omit the specified packages from the resolution.
    pub no_install_package: Vec<PackageName>,
}

impl InstallOptions {
    pub fn new(
        no_install_project: bool,
        no_install_workspace: bool,
        no_install_local: bool,
        no_install_package: Vec<PackageName>,
    ) -> Self {
        Self {
            no_install_project,
            no_install_workspace,
            no_install_local,
            no_install_package,
        }
    }

    /// Returns `true` if a package passes the install filters.
    pub fn include_package(
        &self,
        target: InstallTarget<'_>,
        project_name: Option<&PackageName>,
        members: &BTreeSet<PackageName>,
    ) -> bool {
        let package_name = target.name;
        // If `--no-install-project` is set, remove the project itself.
        if self.no_install_project {
            if let Some(project_name) = project_name {
                if package_name == project_name {
                    debug!(
                        "Omitting `{package_name}` from resolution due to `--no-install-project`"
                    );
                    return false;
                }
            }
        }

        // If `--no-install-workspace` is set, remove the project and any workspace members.
        if self.no_install_workspace {
            // In some cases, the project root might be omitted from the list of workspace members
            // encoded in the lockfile. (But we already checked this above if `--no-install-project`
            // is set.)
            if !self.no_install_project {
                if let Some(project_name) = project_name {
                    if package_name == project_name {
                        debug!(
                            "Omitting `{package_name}` from resolution due to `--no-install-workspace`"
                        );
                        return false;
                    }
                }
            }

            if members.contains(package_name) {
                debug!("Omitting `{package_name}` from resolution due to `--no-install-workspace`");
                return false;
            }
        }

        // If `--no-install-local` is set, remove local packages.
        if self.no_install_local {
            if target.is_local {
                debug!("Omitting `{package_name}` from resolution due to `--no-install-local`");
                return false;
            }
        }

        // If `--no-install-package` is provided, remove the requested packages.
        if self.no_install_package.contains(package_name) {
            debug!("Omitting `{package_name}` from resolution due to `--no-install-package`");
            return false;
        }

        true
    }
}
