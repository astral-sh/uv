use std::{io, path::PathBuf, sync::Arc};

use tempfile::{TempDir, tempdir};

/// The main state storage abstraction.
///
/// This is appropriate for storing persistent data that is not user-facing, such as managed Python
/// installations or tool environments.
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
    fn from_path(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            _temp_dir_drop: None,
        }
    }

    /// Create a temporary state store.
    pub fn temp() -> Result<Self, io::Error> {
        let temp_dir = tempdir()?;
        Ok(Self {
            root: temp_dir.path().to_path_buf(),
            _temp_dir_drop: Some(Arc::new(temp_dir)),
        })
    }

    /// The folder for a specific cache bucket
    pub fn bucket(&self, state_bucket: StateBucket) -> PathBuf {
        self.root.join(state_bucket.to_str())
    }

    /// Prefer, in order:
    ///
    /// 1. The specific state directory specified by the user.
    /// 2. The system-appropriate user-level data directory.
    /// 3. A `.uv` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    pub fn from_settings(state_dir: Option<PathBuf>) -> Result<Self, io::Error> {
        if let Some(state_dir) = state_dir {
            Ok(Self::from_path(state_dir))
        } else if let Some(data_dir) = uv_dirs::legacy_user_state_dir().filter(|dir| dir.exists()) {
            // If the user has an existing directory at (e.g.) `/Users/user/Library/Application Support/uv`,
            // respect it for backwards compatibility. Otherwise, prefer the XDG strategy, even on
            // macOS.
            Ok(Self::from_path(data_dir))
        } else if let Some(data_dir) = uv_dirs::user_state_dir() {
            Ok(Self::from_path(data_dir))
        } else {
            Ok(Self::from_path(".uv"))
        }
    }
}

/// The different kinds of data in the state store are stored in different bucket, which in our case
/// are subdirectories of the state store root.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum StateBucket {
    /// Managed Python installations
    ManagedPython,
    /// Installed tools.
    Tools,
    /// Credentials.
    Credentials,
}

impl StateBucket {
    fn to_str(self) -> &'static str {
        match self {
            Self::ManagedPython => "python",
            Self::Tools => "tools",
            Self::Credentials => "credentials",
        }
    }
}
