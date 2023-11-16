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

impl CacheArgs {
    /// Prefer, in order:
    /// 1. A temporary cache directory, if the user requested `--no-cache`.
    /// 2. The specific cache directory specified by the user via `--cache-dir` or `PUFFIN_CACHE_DIR`.
    /// 3. The system-appropriate cache directory.
    /// 4. A `.puffin_cache` directory in the current working directory.
    pub fn get_cache_dir(self) -> io::Result<(Option<TempDir>, PathBuf)> {
        let project_dirs = ProjectDirs::from("", "", "puffin");
        if self.no_cache {
            let tempdir = tempdir()?;
            let cache_dir = tempdir.path().to_path_buf();
            Ok((Some(tempdir), cache_dir))
        } else if let Some(cache_dir) = self.cache_dir {
            Ok((None, cache_dir))
        } else if let Some(project_dirs) = project_dirs {
            Ok((None, project_dirs.cache_dir().to_path_buf()))
        } else {
            Ok((None, PathBuf::from(".puffin_cache")))
        }
    }
}
