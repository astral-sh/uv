#![cfg(feature = "clap")]

use fs_err as fs;
use std::io;
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;
use tempfile::{tempdir, TempDir};

#[derive(Parser, Debug, Clone)]
pub struct CacheArgs {
    /// Avoid reading from or writing to the cache.
    #[arg(global = true, long, short)]
    no_cache: bool,

    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct CacheDir {
    /// The cache directory.
    cache_dir: PathBuf,
    /// A temporary cache directory, if the user requested `--no-cache`. Included to ensure that
    /// the temporary directory exists for the length of the operation, but is dropped at the end
    /// as appropriate.
    #[allow(dead_code)]
    temp_dir: Option<TempDir>,
}

impl TryFrom<CacheArgs> for CacheDir {
    type Error = io::Error;

    /// Prefer, in order:
    /// 1. A temporary cache directory, if the user requested `--no-cache`.
    /// 2. The specific cache directory specified by the user via `--cache-dir` or `PUFFIN_CACHE_DIR`.
    /// 3. The system-appropriate cache directory.
    /// 4. A `.puffin_cache` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    fn try_from(value: CacheArgs) -> Result<Self, Self::Error> {
        let project_dirs = ProjectDirs::from("", "", "puffin");
        if value.no_cache {
            let temp_dir = tempdir()?;
            let cache_dir = temp_dir.path().to_path_buf();
            Ok(Self {
                cache_dir,
                temp_dir: Some(temp_dir),
            })
        } else if let Some(cache_dir) = value.cache_dir {
            fs::create_dir_all(&cache_dir)?;
            Ok(Self {
                cache_dir: fs::canonicalize(cache_dir)?,
                temp_dir: None,
            })
        } else if let Some(project_dirs) = project_dirs {
            Ok(Self {
                cache_dir: project_dirs.cache_dir().to_path_buf(),
                temp_dir: None,
            })
        } else {
            let cache_dir = ".puffin_cache";
            fs::create_dir_all(cache_dir)?;
            Ok(Self {
                cache_dir: fs::canonicalize(cache_dir)?,
                temp_dir: None,
            })
        }
    }
}

impl CacheDir {
    pub fn path(&self) -> &PathBuf {
        &self.cache_dir
    }
}
