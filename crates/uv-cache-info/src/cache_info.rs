use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use tracing::{debug, info_span, warn};

use uv_fs::{PortablePath, created_time};

use crate::git_info::{Commit, Tags};
use crate::glob::cluster_globs;
use crate::timestamp::Timestamp;

#[derive(Debug, thiserror::Error)]
pub enum CacheInfoError {
    #[error("Failed to parse glob patterns for `cache-keys`: {0}")]
    Glob(#[from] globwalk::GlobError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The information used to determine whether a built distribution is up-to-date, based on the
/// timestamps of relevant files, the current commit of a repository, etc.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CacheInfo {
    /// The timestamp of a single relevant file, at the time of the build.
    ///
    /// This is only populated when the [`CacheInfo`] is derived from a single file (e.g., a
    /// source distribution archive) via [`CacheInfo::from_file`]; freshness checks for a source
    /// tree are instead tracked per-file in [`CacheInfo::files`], so that the disappearance of a
    /// single cache-key file doesn't go unnoticed.
    timestamp: Option<Timestamp>,
    /// The timestamp of each individual `cache-keys` file that was considered when building the
    /// distribution, at the time of the build, keyed by the path of the file relative to the
    /// source tree.
    ///
    /// A `None` value records that the file did _not_ exist at the time of the build. This is
    /// itself meaningful: if a cache-key file is later deleted (e.g., a build backend that
    /// generates a file into the source tree, like a compiled extension module, is deleted along
    /// with the virtual environment), the absence must be distinguishable from the file having
    /// never been tracked at all, so that the deletion is treated as a cache-invalidating change
    /// rather than being silently ignored.
    #[serde(default)]
    files: BTreeMap<Cow<'static, str>, Option<Timestamp>>,
    /// The commit at which the distribution was built.
    commit: Option<Commit>,
    /// The Git tags present at the time of the build.
    tags: Option<Tags>,
    /// Environment variables to include in the cache key.
    #[serde(default)]
    env: BTreeMap<String, Option<String>>,
    /// The timestamp or inode of any directories that should be considered in the cache key.
    #[serde(default)]
    directories: BTreeMap<Cow<'static, str>, Option<DirectoryTimestamp>>,
}

impl CacheInfo {
    /// Return the [`CacheInfo`] for a given timestamp.
    pub fn from_timestamp(timestamp: Timestamp) -> Self {
        Self {
            timestamp: Some(timestamp),
            ..Self::default()
        }
    }

    /// Compute the cache info for a given path, which may be a file or a directory.
    pub fn from_path(path: &Path) -> Result<Self, CacheInfoError> {
        let metadata = fs_err::metadata(path)?;
        if metadata.is_file() {
            Ok(Self::from_file(path)?)
        } else {
            Self::from_directory(path)
        }
    }

    /// Compute the cache info for a given directory.
    pub fn from_directory(directory: &Path) -> Result<Self, CacheInfoError> {
        let mut commit = None;
        let mut tags = None;
        let mut files: BTreeMap<Cow<'static, str>, Option<Timestamp>> = BTreeMap::new();
        let mut directories = BTreeMap::new();
        let mut env = BTreeMap::new();

        // Read the cache keys.
        let pyproject_path = directory.join("pyproject.toml");
        let cache_keys = if let Ok(contents) = fs_err::read_to_string(&pyproject_path) {
            let result = info_span!("toml::from_str cache keys", path = %pyproject_path.display())
                .in_scope(|| toml::from_str::<PyProjectToml>(&contents));
            if let Ok(pyproject_toml) = result {
                pyproject_toml
                    .tool
                    .and_then(|tool| tool.uv)
                    .and_then(|tool_uv| tool_uv.cache_keys)
            } else {
                None
            }
        } else {
            None
        };

        // If no cache keys were defined, use the defaults.
        let cache_keys = cache_keys.unwrap_or_else(|| {
            vec![
                CacheKey::Path(Cow::Borrowed("pyproject.toml")),
                CacheKey::Path(Cow::Borrowed("setup.py")),
                CacheKey::Path(Cow::Borrowed("setup.cfg")),
                CacheKey::Directory {
                    dir: Cow::Borrowed("src"),
                },
            ]
        });

        // Incorporate timestamps from any direct filepaths.
        let mut globs = vec![];
        for cache_key in cache_keys {
            match cache_key {
                CacheKey::Path(file) | CacheKey::File { file } => {
                    if file
                        .as_ref()
                        .chars()
                        .any(|c| matches!(c, '*' | '?' | '[' | '{'))
                    {
                        // Defer globs to a separate pass.
                        globs.push(file);
                        continue;
                    }

                    // Treat the path as a file.
                    let path = directory.join(file.as_ref());
                    let metadata = match path.metadata() {
                        Ok(metadata) => metadata,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            // Record the absence explicitly: if the file previously existed (e.g.,
                            // it's a file that a build backend generates into the source tree),
                            // its disappearance must be distinguishable from it never having been
                            // tracked, or a deleted file would silently fail to invalidate the
                            // cache.
                            files.insert(file, None);
                            continue;
                        }
                        Err(err) => {
                            warn!("Failed to read metadata for file: {err}");
                            continue;
                        }
                    };
                    if !metadata.is_file() {
                        warn!(
                            "Expected file for cache key, but found directory: `{}`",
                            path.display()
                        );
                        continue;
                    }
                    files.insert(file, Some(Timestamp::from_metadata(&metadata)));
                }
                CacheKey::Directory { dir } => {
                    // Treat the path as a directory.
                    let path = directory.join(dir.as_ref());
                    let metadata = match path.metadata() {
                        Ok(metadata) => metadata,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            directories.insert(dir, None);
                            continue;
                        }
                        Err(err) => {
                            warn!("Failed to read metadata for directory: {err}");
                            continue;
                        }
                    };
                    if !metadata.is_dir() {
                        warn!(
                            "Expected directory for cache key, but found file: `{}`",
                            path.display()
                        );
                        continue;
                    }

                    if let Ok(created) = created_time(&path, &metadata) {
                        // Prefer the creation time.
                        directories.insert(
                            dir,
                            Some(DirectoryTimestamp::Timestamp(Timestamp::from(created))),
                        );
                    } else {
                        // Fall back to the inode.
                        cfg_select! {
                            unix => {
                                use std::os::unix::fs::MetadataExt;
                                directories.insert(
                                    dir,
                                    Some(DirectoryTimestamp::Inode(metadata.ino())),
                                );
                            },
                            _ => {
                                warn!(
                                    "Failed to read creation time for directory: `{}`",
                                    path.display()
                                );
                            },
                        }
                    }
                }
                CacheKey::Git {
                    git: GitPattern::Bool(true),
                } => match Commit::from_repository(directory) {
                    Ok(commit_info) => commit = Some(commit_info),
                    Err(err) => {
                        debug!("Failed to read the current commit: {err}");
                    }
                },
                CacheKey::Git {
                    git: GitPattern::Set(set),
                } => {
                    if set.commit.unwrap_or(false) {
                        match Commit::from_repository(directory) {
                            Ok(commit_info) => commit = Some(commit_info),
                            Err(err) => {
                                debug!("Failed to read the current commit: {err}");
                            }
                        }
                    }
                    if set.tags.unwrap_or(false) {
                        match Tags::from_repository(directory) {
                            Ok(tags_info) => tags = Some(tags_info),
                            Err(err) => {
                                debug!("Failed to read the current tags: {err}");
                            }
                        }
                    }
                }
                CacheKey::Git {
                    git: GitPattern::Bool(false),
                } => {}
                CacheKey::Environment { env: var } => {
                    let value = std::env::var(&var).ok();
                    env.insert(var, value);
                }
            }
        }

        // If we have any globs, first cluster them using LCP and then do a single pass on each group.
        if !globs.is_empty() {
            for (glob_base, glob_patterns) in cluster_globs(&globs) {
                let walker = globwalk::GlobWalkerBuilder::from_patterns(
                    directory.join(glob_base),
                    &glob_patterns,
                )
                .file_type(globwalk::FileType::FILE | globwalk::FileType::SYMLINK)
                .build()?;
                for entry in walker {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(err) => {
                            warn!("Failed to read glob entry: {err}");
                            continue;
                        }
                    };
                    let metadata = if entry.path_is_symlink() {
                        // resolve symlinks for leaf entries without following symlinks while globbing
                        match fs_err::metadata(entry.path()) {
                            Ok(metadata) => metadata,
                            Err(err) => {
                                warn!("Failed to resolve symlink for glob entry: {err}");
                                continue;
                            }
                        }
                    } else {
                        match entry.metadata() {
                            Ok(metadata) => metadata,
                            Err(err) => {
                                warn!("Failed to read metadata for glob entry: {err}");
                                continue;
                            }
                        }
                    };
                    if !metadata.is_file() {
                        if !entry.path_is_symlink() {
                            // don't warn if it was a symlink - it may legitimately resolve to a directory
                            warn!(
                                "Expected file for cache key, but found directory: `{}`",
                                entry.path().display()
                            );
                        }
                        continue;
                    }

                    // Key each match by its path relative to the source directory, using a
                    // portable (forward-slash) representation, so that keys are stable across
                    // platforms and comparable between separate calls to `from_directory`.
                    //
                    // Unlike explicitly-named `CacheKey::Path`/`CacheKey::File` entries, we don't
                    // record an explicit `None` for glob matches that disappear: since a glob
                    // pattern doesn't enumerate the files that *could* match, there's no fixed set
                    // of keys to mark as absent. Instead, a file that no longer matches is simply
                    // missing from `files` on the next computation, which is already sufficient
                    // for the map comparison in `PartialEq` to detect the change.
                    let path = entry.into_path();
                    let relative = path.strip_prefix(directory).unwrap_or(&path);
                    let key = Cow::Owned(PortablePath::from(relative).to_string());
                    files.insert(key, Some(Timestamp::from_metadata(&metadata)));
                }
            }
        }

        if !(files.is_empty() && directories.is_empty() && env.is_empty()) {
            debug!(
                "Computed cache info: {files:?}, {commit:?}, {tags:?}, {env:?}, {directories:?}"
            );
        }

        Ok(Self {
            timestamp: None,
            files,
            commit,
            tags,
            env,
            directories,
        })
    }

    /// Compute the cache info for a given file, assumed to be a binary or source distribution
    /// represented as (e.g.) a `.whl` or `.tar.gz` archive.
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let metadata = fs_err::metadata(path.as_ref())?;
        let timestamp = Timestamp::from_metadata(&metadata);
        Ok(Self {
            timestamp: Some(timestamp),
            ..Self::default()
        })
    }

    /// Returns `true` if the cache info is empty.
    pub fn is_empty(&self) -> bool {
        self.timestamp.is_none()
            && self.files.is_empty()
            && self.commit.is_none()
            && self.tags.is_none()
            && self.env.is_empty()
            && self.directories.is_empty()
    }
}

/// A `pyproject.toml` with an (optional) `[tool.uv]` section.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    tool: Option<Tool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Tool {
    uv: Option<ToolUv>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ToolUv {
    cache_keys: Option<Vec<CacheKey>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, rename_all = "kebab-case", deny_unknown_fields)]
pub enum CacheKey {
    /// Ex) `"Cargo.lock"` or `"**/*.toml"`
    Path(Cow<'static, str>),
    /// Ex) `{ file = "Cargo.lock" }` or `{ file = "**/*.toml" }`
    File { file: Cow<'static, str> },
    /// Ex) `{ dir = "src" }`
    Directory { dir: Cow<'static, str> },
    /// Ex) `{ git = true }` or `{ git = { commit = true, tags = false } }`
    Git { git: GitPattern },
    /// Ex) `{ env = "UV_CACHE_INFO" }`
    Environment { env: String },
}

#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, rename_all = "kebab-case", deny_unknown_fields)]
pub enum GitPattern {
    Bool(bool),
    Set(GitSet),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GitSet {
    commit: Option<bool>,
    tags: Option<bool>,
}

/// A timestamp used to measure changes to a directory.
#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(untagged, rename_all = "kebab-case", deny_unknown_fields)]
enum DirectoryTimestamp {
    Timestamp(Timestamp),
    Inode(u64),
}

#[cfg(all(test, unix))]
mod tests_unix {
    use std::collections::BTreeMap;

    use anyhow::Result;

    use super::{CacheInfo, Timestamp};

    #[test]
    fn test_cache_info_symlink_resolve() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let dir = dir.path().join("dir");
        fs_err::create_dir_all(&dir)?;

        let write_manifest = |cache_key: &str| {
            fs_err::write(
                dir.join("pyproject.toml"),
                format!(
                    r#"
                [tool.uv]
                cache-keys = [
                    "{cache_key}"
                ]
                "#
                ),
            )
        };

        let touch = |path: &str| -> Result<_> {
            let path = dir.join(path);
            fs_err::create_dir_all(path.parent().unwrap())?;
            fs_err::write(&path, "")?;
            Ok(Timestamp::from_metadata(&path.metadata()?))
        };

        // Each tracked cache-key file is now recorded individually (as opposed to collapsing to a
        // single "most recently modified" timestamp), so that a file disappearing from the set is
        // itself a detectable change rather than being silently absorbed into an unrelated file's
        // timestamp.
        let cache_files = || -> Result<BTreeMap<String, Option<Timestamp>>> {
            Ok(CacheInfo::from_directory(&dir)?
                .files
                .into_iter()
                .map(|(path, timestamp)| (path.into_owned(), timestamp))
                .collect())
        };

        write_manifest("x/**")?;
        assert_eq!(cache_files()?, BTreeMap::new());
        let y = touch("x/y")?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/y".to_string(), Some(y))])
        );
        let z = touch("x/z")?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/y".to_string(), Some(y)), ("x/z".to_string(), Some(z))])
        );

        // leaf entry symlink should be resolved
        let a = touch("../a")?;
        fs_err::os::unix::fs::symlink(dir.join("../a"), dir.join("x/a"))?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([
                ("x/a".to_string(), Some(a)),
                ("x/y".to_string(), Some(y)),
                ("x/z".to_string(), Some(z)),
            ])
        );

        // symlink directories should not be followed while globbing
        let _c = touch("../b/c")?;
        fs_err::os::unix::fs::symlink(dir.join("../b"), dir.join("x/b"))?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([
                ("x/a".to_string(), Some(a)),
                ("x/y".to_string(), Some(y)),
                ("x/z".to_string(), Some(z)),
            ])
        );

        // no globs, should work as expected
        write_manifest("x/y")?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/y".to_string(), Some(y))])
        );
        write_manifest("x/a")?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/a".to_string(), Some(a))])
        );
        write_manifest("x/b/c")?;
        let c = Timestamp::from_metadata(&dir.join("x/b/c").metadata()?);
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/b/c".to_string(), Some(c))])
        );

        // symlink pointing to a directory
        write_manifest("x/*b*")?;
        assert_eq!(cache_files()?, BTreeMap::new());

        // A cache-key file that is deleted after having existed must be distinguishable from one
        // that never existed: the deletion itself is a cache-invalidating change.
        write_manifest("x/y")?;
        assert_eq!(
            cache_files()?,
            BTreeMap::from([("x/y".to_string(), Some(y))])
        );
        fs_err::remove_file(dir.join("x/y"))?;
        assert_eq!(cache_files()?, BTreeMap::from([("x/y".to_string(), None)]));

        Ok(())
    }
}
