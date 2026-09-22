use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use tracing::{debug, warn};
use uv_fs::Simplified;

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

        debug!("Reverting changes to {}", self.path.user_display());
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
            warn!("Failed to restore {}: {err}", file.path.user_display());
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
