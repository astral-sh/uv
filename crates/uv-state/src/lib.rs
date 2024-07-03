use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use directories::ProjectDirs;
use fs_err as fs;
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

        // Add a .gitignore.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        Ok(Self {
            root: fs::canonicalize(root)?,
            ..self
        })
    }

    /// The folder for a specific cache bucket
    pub fn bucket(&self, state_bucket: StateBucket) -> PathBuf {
        self.root.join(state_bucket.to_str())
    }

    /// Prefer, in order:
    /// 1. The specific state directory specified by the user.
    /// 2. The system-appropriate user-level data directory.
    /// 3. A `.uv` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    pub fn from_settings(state_dir: Option<PathBuf>) -> Result<Self, io::Error> {
        if let Some(state_dir) = state_dir {
            StateStore::from_path(state_dir)
        } else if let Some(data_dir) = data_dir() {
            StateStore::from_path(data_dir.join("uv"))
        } else {
            StateStore::from_path(".uv")
        }
    }
}

/// The different kinds of data in the state store are stored in different bucket, which in our case
/// are subdirectories of the state store root.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum StateBucket {
    // Managed Python installations
    ManagedPython,
    // Installed tools
    Tools,
}

impl StateBucket {
    fn to_str(self) -> &'static str {
        match self {
            Self::ManagedPython => "python",
            Self::Tools => "tools",
        }
    }
}

/// Returns the path to the user data directory.
///
/// This is similar to the `data_dir()` returned by the `dirs` crate, but it respects the
/// `XDG_DATA_HOME` environment variable on both Linux _and_ macOS. If `XDG_DATA_HOME` is not
/// set, it defaults to `$HOME/.local/share` on Linux and `Application Support` on macOS.
///
/// Note we do not use `XDG_STATE_HOME` because this data is portable:
///
/// > The $XDG_STATE_HOME contains state data that should persist between (application) restarts,
/// > but that is not important or portable enough to the user that it should be stored in $XDG_DATA_HOME.
fn data_dir() -> Option<PathBuf> {
    let default = ProjectDirs::from("", "", "uv").map(|dirs| dirs.data_dir().to_path_buf());

    // On macOS, respect `XDG_DATA_HOME` if present, then fallback to the default
    if cfg!(target_os = "macos") {
        std::env::var_os("XDG_DATA_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or(default)
    // On Windows and Linux, use the `ProjectDirs` default behavior.
    } else {
        default
    }
}
