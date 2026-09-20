use std::io;

use tracing::debug;
use uv_scripts::Pep723Script;
use uv_workspace::VirtualProject;

use crate::commands::project::lock_target::LockTarget;

/// Restore a project edit on errors or Ctrl-C unless it succeeds.
pub(super) struct ProjectEdit {
    snapshot: Option<ProjectSnapshot>,
}

impl ProjectEdit {
    /// Start guarding an edit once its files have been modified.
    pub(super) fn new(snapshot: ProjectSnapshot, modified: bool) -> Self {
        let snapshot = modified.then_some(snapshot);
        let _ = ctrlc::set_handler({
            let snapshot = snapshot.clone();
            move || {
                if let Some(snapshot) = &snapshot {
                    let _ = snapshot.revert();
                }

                #[expect(clippy::cast_possible_wrap)]
                std::process::exit(if cfg!(windows) {
                    0xC000_013A_u32 as i32
                } else {
                    130
                });
            }
        });
        Self { snapshot }
    }

    /// Keep the edited files when the operation succeeds.
    pub(super) fn commit(mut self) {
        self.snapshot = None;
    }
}

impl Drop for ProjectEdit {
    fn drop(&mut self) {
        if let Some(snapshot) = &self.snapshot {
            let _ = snapshot.revert();
        }
    }
}

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub(super) enum ProjectSnapshot {
    Script(Pep723Script, Option<Vec<u8>>),
    Project(VirtualProject, Option<Vec<u8>>),
}

impl ProjectSnapshot {
    /// Write the snapshot back to disk (e.g., to a `pyproject.toml` and `uv.lock`).
    fn revert(&self) -> Result<(), io::Error> {
        match self {
            Self::Script(script, lock) => {
                // Write the PEP 723 script back to disk.
                debug!("Reverting changes to PEP 723 script block");
                script.write(&script.metadata.raw)?;

                // Write the lockfile back to disk.
                let target = LockTarget::from(script);
                if let Some(lock) = lock {
                    debug!("Reverting changes to `uv.lock`");
                    fs_err::write(target.lock_path(), lock)?;
                } else {
                    debug!("Removing `uv.lock`");
                    fs_err::remove_file(target.lock_path())?;
                }
                Ok(())
            }
            Self::Project(project, lock) => {
                // Write the workspace `pyproject.toml` back to disk.
                let workspace = project.workspace();
                if workspace.install_path() != project.root() {
                    debug!("Reverting changes to workspace `pyproject.toml`");
                    fs_err::write(
                        workspace.install_path().join("pyproject.toml"),
                        workspace.pyproject_toml().as_ref(),
                    )?;
                }

                // Write the `pyproject.toml` back to disk.
                debug!("Reverting changes to `pyproject.toml`");
                fs_err::write(
                    project.root().join("pyproject.toml"),
                    project.pyproject_toml().as_ref(),
                )?;

                // Write the lockfile back to disk.
                let target = LockTarget::from(project.workspace());
                if let Some(lock) = lock {
                    debug!("Reverting changes to `uv.lock`");
                    fs_err::write(target.lock_path(), lock)?;
                } else {
                    debug!("Removing `uv.lock`");
                    fs_err::remove_file(target.lock_path())?;
                }
                Ok(())
            }
        }
    }
}
