use std::path::Path;

use itertools::Either;

use uv_normalize::PackageName;
use uv_resolver::{Installable, Lock, Package};
use uv_workspace::Workspace;

/// A target that can be installed from a lockfile.
#[derive(Debug, Copy, Clone)]
pub(crate) enum InstallTarget<'lock> {
    /// A project (which could be a workspace root or member).
    Project {
        workspace: &'lock Workspace,
        name: &'lock PackageName,
        lock: &'lock Lock,
    },
    /// An entire workspace.
    Workspace {
        workspace: &'lock Workspace,
        lock: &'lock Lock,
    },
    /// An entire workspace with a (legacy) non-project root.
    NonProjectWorkspace {
        workspace: &'lock Workspace,
        lock: &'lock Lock,
    },
}

impl<'lock> Installable<'lock> for InstallTarget<'lock> {
    fn install_path(&self) -> &'lock Path {
        match self {
            Self::Project { workspace, .. } => workspace.install_path(),
            Self::Workspace { workspace, .. } => workspace.install_path(),
            Self::NonProjectWorkspace { workspace, .. } => workspace.install_path(),
        }
    }

    fn lock(&self) -> &'lock Lock {
        match self {
            Self::Project { lock, .. } => lock,
            Self::Workspace { lock, .. } => lock,
            Self::NonProjectWorkspace { lock, .. } => lock,
        }
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project { name, .. } => Either::Right(Either::Left(std::iter::once(*name))),
            Self::NonProjectWorkspace { lock, .. } => Either::Left(lock.members().iter()),
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Either::Right(Either::Right(lock.root().into_iter().map(Package::name)))
                } else {
                    Either::Left(lock.members().iter())
                }
            }
        }
    }

    fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project { name, .. } => Some(name),
            Self::Workspace { .. } => None,
            Self::NonProjectWorkspace { .. } => None,
        }
    }
}

impl<'lock> InstallTarget<'lock> {
    /// Return the [`Workspace`] of the target.
    pub(crate) fn workspace(&self) -> &'lock Workspace {
        match self {
            Self::Project { workspace, .. } => workspace,
            Self::Workspace { workspace, .. } => workspace,
            Self::NonProjectWorkspace { workspace, .. } => workspace,
        }
    }
}
