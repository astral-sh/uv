#![cfg(feature = "clap")]

use std::io;
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;

use crate::Cache;

#[derive(Parser, Debug, Clone)]
pub struct CacheArgs {
    /// Avoid reading from or writing to the cache.
    #[arg(
        global = true,
        long,
        short,
        alias = "no-cache-dir",
        env = "UV_NO_CACHE"
    )]
    no_cache: bool,

    /// Path to the cache directory.
    #[arg(global = true, long, env = "UV_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

impl TryFrom<CacheArgs> for Cache {
    type Error = io::Error;

    /// Prefer, in order:
    /// 1. A temporary cache directory, if the user requested `--no-cache`.
    /// 2. The specific cache directory specified by the user via `--cache-dir` or `UV_CACHE_DIR`.
    /// 3. The system-appropriate cache directory.
    /// 4. A `.uv_cache` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    fn try_from(value: CacheArgs) -> Result<Self, Self::Error> {
        if value.no_cache {
            Cache::temp()
        } else if let Some(cache_dir) = value.cache_dir {
            Cache::from_path(cache_dir)
        } else if let Some(project_dirs) = ProjectDirs::from("", "", "uv") {
            Cache::from_path(project_dirs.cache_dir())
        } else {
            Cache::from_path(".uv_cache")
        }
    }
}
