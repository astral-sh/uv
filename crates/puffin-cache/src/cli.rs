#![cfg(feature = "clap")]

use std::io;
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;
use fs_err as fs;

use crate::Cache;

#[derive(Parser, Debug, Clone)]
pub struct CacheArgs {
    /// Avoid reading from or writing to the cache.
    #[arg(global = true, long, short)]
    no_cache: bool,

    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

impl TryFrom<CacheArgs> for Cache {
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
            Ok(Cache::temp()?)
        } else if let Some(cache_dir) = value.cache_dir {
            fs::create_dir_all(&cache_dir)?;
            Ok(Cache::from_path(fs::canonicalize(cache_dir)?))
        } else if let Some(project_dirs) = project_dirs {
            Ok(Cache::from_path(project_dirs.cache_dir().to_path_buf()))
        } else {
            let cache_dir = ".puffin_cache";
            fs::create_dir_all(cache_dir)?;
            Ok(Cache::from_path(fs::canonicalize(cache_dir)?))
        }
    }
}
