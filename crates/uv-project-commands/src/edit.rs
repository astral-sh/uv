use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use tracing::{debug, warn};

use uv_fs::Simplified;
use uv_lock_operations::LockTarget;
use uv_python_interpreter::{Interpreter, PythonEnvironment};
use uv_scripts::{Pep723Metadata, Pep723Script};
use uv_workspace::pyproject::PyProjectToml;
use uv_workspace::{VirtualProject, WorkspaceCache};

use crate::ProjectError;

/// A project manifest or script metadata to edit.
#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub(super) enum EditTarget {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script),
    /// A project with a `pyproject.toml`.
    Project(VirtualProject),
}

impl<'lock> From<&'lock EditTarget> for LockTarget<'lock> {
    fn from(value: &'lock EditTarget) -> Self {
        match value {
            EditTarget::Script(script) => Self::Script(script),
            EditTarget::Project(project) => Self::Workspace(project.workspace()),
        }
    }
}

impl EditTarget {
    /// Write the updated metadata, returning whether the content changed.
    pub(super) fn write(&self, content: &str) -> Result<bool, io::Error> {
        match self {
            Self::Script(script) => {
                if content == script.metadata.raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    script.write(content)?;
                    Ok(true)
                }
            }
            Self::Project(project) => {
                if content == project.pyproject_toml().raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    let pyproject_path = project.root().join("pyproject.toml");
                    fs_err::write(pyproject_path, content)?;
                    Ok(true)
                }
            }
        }
    }

    /// Update parsed metadata and the workspace cache after writing the target.
    pub(super) fn update(
        self,
        content: &str,
        workspace_cache: &WorkspaceCache,
    ) -> Result<Self, ProjectError> {
        match self {
            Self::Script(mut script) => {
                script.metadata = Pep723Metadata::from_str(content)
                    .map_err(ProjectError::Pep723ScriptTomlParse)?;
                Ok(Self::Script(script))
            }
            Self::Project(project) => {
                let pyproject_path = project.root().join("pyproject.toml");
                let project = project
                    .update_member(
                        PyProjectToml::from_string(content.to_string(), &pyproject_path)
                            .map_err(ProjectError::PyprojectTomlParse)?,
                        workspace_cache,
                    )?
                    .ok_or(ProjectError::PyprojectTomlUpdate)?;
                Ok(Self::Project(project))
            }
        }
    }
}

/// The interpreter used for resolution, or an environment that can also be synchronized.
#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
pub(super) enum PythonTarget {
    Interpreter(Interpreter),
    Environment(PythonEnvironment),
}

impl PythonTarget {
    /// Return the interpreter from either form of Python discovery.
    pub(super) fn interpreter(&self) -> &Interpreter {
        match self {
            Self::Interpreter(interpreter) => interpreter,
            Self::Environment(venv) => venv.interpreter(),
        }
    }
}

/// Restore project or script files on errors and Ctrl-C, unless the edit is committed.
///
/// Only changed files are restored. Callers must exclude files they cannot modify, such as
/// lockfiles when editing with `--frozen`.
pub(super) struct ProjectEdit {
    files: Arc<Mutex<Vec<FileSnapshot>>>,
}

impl ProjectEdit {
    /// Snapshot the files an operation can modify and install its Ctrl-C handler.
    pub(super) fn new(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self> {
        let files = paths
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|path| {
                let contents = read_file(&path)?;
                Ok(FileSnapshot { path, contents })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let files = Arc::new(Mutex::new(files));

        let _ = ctrlc::set_handler({
            let files = Arc::clone(&files);
            move || {
                revert(&mut files.lock().unwrap_or_else(PoisonError::into_inner));

                #[expect(clippy::cast_possible_wrap)]
                std::process::exit(if cfg!(windows) {
                    0xC000_013A_u32 as i32
                } else {
                    130
                });
            }
        });

        Ok(Self { files })
    }

    /// Keep the edited files when the operation succeeds.
    pub(super) fn commit(self) {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

impl Drop for ProjectEdit {
    fn drop(&mut self) {
        revert(&mut self.files.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

impl FileSnapshot {
    /// Restore the original contents, or remove a file created by the operation.
    fn revert(&self) -> io::Result<()> {
        // An unchanged file may be read-only, even when another file in the edit is writable.
        if let Ok(contents) = read_file(&self.path)
            && contents == self.contents
        {
            return Ok(());
        }

        debug!("Reverting changes to `{}`", self.path.user_display());
        if let Some(contents) = &self.contents {
            fs_err::write(&self.path, contents)
        } else {
            match fs_err::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err),
            }
        }
    }
}

/// Attempt every restoration even if an earlier file cannot be restored.
fn revert(files: &mut Vec<FileSnapshot>) {
    for file in files.drain(..) {
        if let Err(err) = file.revert() {
            warn!("Failed to restore `{}`: {err}", file.path.user_display());
        }
    }
}

fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs_err::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
