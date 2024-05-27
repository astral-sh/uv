use std::{
    fs,
    io::{self},
    path::{Path, PathBuf},
    sync::Arc,
};

use directories::ProjectDirs;
use tempfile::{tempdir, TempDir};

/// The main state storage abstraction.
///
/// This is appropriate
#[derive(Debug, Clone)]
pub struct StateStore {
    /// The state storage.
    root: PathBuf,
    /// A temporary state storage.
    ///
    /// Included to ensure that the temporary store exists for the length of the operation, but
    /// is dropped at the end as appropriate.
    _temp_dir_drop: Option<Arc<TempDir>>,
}

impl StateStore {
    /// A persistent state store at `root`.
    pub fn from_path(root: impl Into<PathBuf>) -> Result<Self, io::Error> {
        Ok(Self {
            root: root.into(),
            _temp_dir_drop: None,
        })
    }

    /// Create a temporary state store.
    pub fn temp() -> Result<Self, io::Error> {
        let temp_dir = tempdir()?;
        Ok(Self {
            root: temp_dir.path().to_path_buf(),
            _temp_dir_drop: Some(Arc::new(temp_dir)),
        })
    }

    /// Return the root of the state store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Initialize the state store.
    pub fn init(self) -> Result<Self, io::Error> {
        let root = &self.root;

        // Create the state store directory, if it doesn't exist.
        fs::create_dir_all(root)?;

        Ok(Self {
            root: fs::canonicalize(root)?,
            ..self
        })
    }
}

impl StateStore {
    /// Prefer, in order:
    /// 1. The specific state directory specified by the user.
    /// 2. The system-appropriate user-level data directory.
    /// 3. A `.uv` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    pub fn from_settings(state_dir: Option<PathBuf>) -> Result<Self, io::Error> {
        if let Some(state_dir) = state_dir {
            StateStore::from_path(state_dir)
        } else if let Some(project_dirs) = ProjectDirs::from("", "", "uv") {
            StateStore::from_path(project_dirs.data_dir())
        } else {
            StateStore::from_path(".uv")
        }
    }
}
