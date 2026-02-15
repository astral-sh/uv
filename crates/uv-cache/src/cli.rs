use std::io;
use std::path::{Path, PathBuf};
use uv_preview::{Preview, PreviewFeature};
use uv_static::EnvVars;

use crate::Cache;
use clap::{Parser, ValueHint};
use tracing::{debug, warn};

#[derive(Parser, Debug, Clone)]
#[command(next_help_heading = "Cache options")]
pub struct CacheArgs {
    /// Avoid reading from or writing to the cache, instead using a temporary directory for the
    /// duration of the operation.
    #[arg(
        global = true,
        long,
        short,
        alias = "no-cache-dir",
        env = EnvVars::UV_NO_CACHE,
        value_parser = clap::builder::BoolishValueParser::new(),
    )]
    pub no_cache: bool,

    /// Path to the cache directory.
    ///
    /// Defaults to `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on macOS and Linux, and
    /// `%LOCALAPPDATA%\uv\cache` on Windows.
    ///
    /// To view the location of the cache directory, run `uv cache dir`.
    #[arg(global = true, long, env = EnvVars::UV_CACHE_DIR, value_hint = ValueHint::DirPath)]
    pub cache_dir: Option<PathBuf>,
}

impl Cache {
    /// Prefer, in order:
    ///
    /// 1. A temporary cache directory, if the user requested `--no-cache`.
    /// 2. The specific cache directory specified by the user via `--cache-dir` or `UV_CACHE_DIR`.
    /// 3. The system-appropriate cache directory.
    /// 4. A `.uv_cache` directory in the current working directory.
    ///
    /// Returns an absolute cache dir.
    pub fn from_settings(
        no_cache: bool,
        cache_dir: Option<PathBuf>,
        preview: Preview,
    ) -> Result<Self, io::Error> {
        let lock_file_override_umask = !preview.is_enabled(PreviewFeature::RespectUmask);
        if no_cache {
            Self::temp(lock_file_override_umask)
        } else if let Some(cache_dir) = cache_dir {
            Ok(Self::from_path(cache_dir, lock_file_override_umask))
        } else if let Some(cache_dir) = uv_dirs::legacy_user_cache_dir().filter(|dir| dir.exists())
        {
            // If the user has an existing directory at (e.g.) `/Users/user/Library/Caches/uv`,
            // respect it for backwards compatibility. Otherwise, prefer the XDG strategy, even on
            // macOS.
            Ok(Self::from_path(cache_dir, lock_file_override_umask))
        } else if let Some(cache_dir) = uv_dirs::user_cache_dir() {
            if cfg!(windows) {
                // On Windows, we append `cache` to the LocalAppData directory, i.e., prefer
                // `C:\Users\User\AppData\Local\uv\cache` over `C:\Users\User\AppData\Local\uv`.
                //
                // Unfortunately, v0.3.0 and v0.3.1 used the latter, so we need to migrate the cache
                // for those users.
                let destination = cache_dir.join("cache");
                let source = cache_dir;
                if let Err(err) = migrate_windows_cache(&source, &destination) {
                    warn!(
                        "Failed to migrate cache from `{}` to `{}`: {err}",
                        source.display(),
                        destination.display()
                    );
                }

                Ok(Self::from_path(destination, lock_file_override_umask))
            } else {
                Ok(Self::from_path(cache_dir, lock_file_override_umask))
            }
        } else {
            Ok(Self::from_path(".uv_cache", lock_file_override_umask))
        }
    }

    pub fn from_args(args: CacheArgs, preview: Preview) -> Result<Self, io::Error> {
        Self::from_settings(args.no_cache, args.cache_dir, preview)
    }
}

/// Migrate the Windows cache from `C:\Users\User\AppData\Local\uv` to `C:\Users\User\AppData\Local\uv\cache`.
fn migrate_windows_cache(source: &Path, destination: &Path) -> Result<(), io::Error> {
    // The list of expected cache buckets in v0.3.0.
    for directory in [
        "built-wheels-v3",
        "flat-index-v0",
        "git-v0",
        "interpreter-v2",
        "simple-v12",
        "wheels-v1",
        "archive-v0",
        "builds-v0",
        "environments-v1",
    ] {
        let source = source.join(directory);
        let destination = destination.join(directory);

        // Migrate the cache bucket.
        if source.exists() {
            debug!(
                "Migrating cache bucket from {} to {}",
                source.display(),
                destination.display()
            );
            if let Some(parent) = destination.parent() {
                fs_err::create_dir_all(parent)?;
            }
            fs_err::rename(&source, &destination)?;
        }
    }

    // The list of expected cache files in v0.3.0.
    for file in [".gitignore", "CACHEDIR.TAG"] {
        let source = source.join(file);
        let destination = destination.join(file);

        // Migrate the cache file.
        if source.exists() {
            debug!(
                "Migrating cache file from {} to {}",
                source.display(),
                destination.display()
            );
            if let Some(parent) = destination.parent() {
                fs_err::create_dir_all(parent)?;
            }
            fs_err::rename(&source, &destination)?;
        }
    }

    Ok(())
}
