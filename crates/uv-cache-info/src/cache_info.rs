use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, info_span, warn};

use uv_fs::Simplified;
use uv_normalize::GroupName;

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
    /// The timestamp of the most recent `ctime` of any relevant files, at the time of the build.
    /// The timestamp will typically be the maximum of the `ctime` values of the `pyproject.toml`,
    /// `setup.py`, and `setup.cfg` files, if they exist; however, users can provide additional
    /// files to timestamp via the `cache-keys` field.
    timestamp: Option<Timestamp>,
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
        Self::from_directory_impl(directory, None, &|_| false)
    }

    /// Like [`CacheInfo::from_directory`], but filters group-scoped cache keys by the given
    /// predicate, and optionally applies a consumer-provided override.
    ///
    /// A cache key tagged with `group = "..."` is only incorporated when `is_group_active` returns
    /// `true` for that group; untagged keys always apply. Callers that know the active dependency
    /// groups (e.g. the project commands) pass `|group| dependency_groups.contains(group)`. Callers
    /// with no group context use [`CacheInfo::from_directory`], which excludes all group-scoped keys.
    pub fn from_directory_filtered(
        directory: &Path,
        overrides: Option<&[CacheKey]>,
        is_group_active: &dyn Fn(&GroupName) -> bool,
    ) -> Result<Self, CacheInfoError> {
        Self::from_directory_impl(directory, overrides, is_group_active)
    }

    /// Like [`CacheInfo::from_directory`], but uses the provided `cache_keys` rather than reading
    /// them from the package's own `pyproject.toml`.
    ///
    /// This allows a consuming project to override a dependency's cache keys for its own build
    /// (via `tool.uv.cache-keys-package`), without modifying the dependency itself — mirroring
    /// `tool.uv.config-settings-package`. The dependency's own behavior (e.g., for developers
    /// working within its directory) is unaffected, since the override lives only in the
    /// consumer's settings.
    pub fn from_directory_with_keys(
        directory: &Path,
        cache_keys: &[CacheKey],
    ) -> Result<Self, CacheInfoError> {
        Self::from_directory_impl(directory, Some(cache_keys), &|_| false)
    }

    fn from_directory_impl(
        directory: &Path,
        overrides: Option<&[CacheKey]>,
        is_group_active: &dyn Fn(&GroupName) -> bool,
    ) -> Result<Self, CacheInfoError> {
        let mut commit = None;
        let mut tags = None;
        let mut last_changed: Option<(PathBuf, Timestamp)> = None;
        let mut directories = BTreeMap::new();
        let mut env = BTreeMap::new();

        // Resolve the cache keys: a consumer-provided override takes precedence over the
        // package's own `pyproject.toml`, which in turn falls back to the defaults.
        let cache_keys = if let Some(overrides) = overrides {
            overrides.to_vec()
        } else {
            // Read the cache keys.
            let pyproject_path = directory.join("pyproject.toml");
            let cache_keys = if let Ok(contents) = fs_err::read_to_string(&pyproject_path) {
                let result =
                    info_span!("toml::from_str cache keys", path = %pyproject_path.display())
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
            cache_keys.unwrap_or_else(|| {
                vec![
                    CacheKey::Path(Cow::Borrowed("pyproject.toml")),
                    CacheKey::Path(Cow::Borrowed("setup.py")),
                    CacheKey::Path(Cow::Borrowed("setup.cfg")),
                    CacheKey::Directory {
                        dir: Cow::Borrowed("src"),
                        group: None,
                    },
                ]
            })
        };

        // Incorporate timestamps from any direct filepaths.
        let mut globs = vec![];
        for cache_key in cache_keys {
            // Skip keys scoped to a dependency group that isn't active.
            if let Some(group) = cache_key.group() {
                if !is_group_active(group) {
                    continue;
                }
            }
            match cache_key {
                CacheKey::Path(file) | CacheKey::File { file, .. } => {
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
                    let timestamp = Timestamp::from_metadata(&metadata);
                    if last_changed.as_ref().is_none_or(|(_, prev_timestamp)| {
                        *prev_timestamp < Timestamp::from_metadata(&metadata)
                    }) {
                        last_changed = Some((path, timestamp));
                    }
                }
                CacheKey::Directory { dir, .. } => {
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

                    if let Ok(created) = metadata.created() {
                        // Prefer the creation time.
                        directories.insert(
                            dir,
                            Some(DirectoryTimestamp::Timestamp(Timestamp::from(created))),
                        );
                    } else {
                        // Fall back to the inode.
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::MetadataExt;
                            directories
                                .insert(dir, Some(DirectoryTimestamp::Inode(metadata.ino())));
                        }
                        #[cfg(not(unix))]
                        {
                            warn!(
                                "Failed to read creation time for directory: `{}`",
                                path.display()
                            );
                        }
                    }
                }
                CacheKey::Git {
                    git: GitPattern::Bool(true),
                    ..
                } => match Commit::from_repository(directory) {
                    Ok(commit_info) => commit = Some(commit_info),
                    Err(err) => {
                        debug!("Failed to read the current commit: {err}");
                    }
                },
                CacheKey::Git {
                    git: GitPattern::Set(set),
                    ..
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
                    ..
                } => {}
                CacheKey::Environment { env: var, .. } => {
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
                    let timestamp = Timestamp::from_metadata(&metadata);
                    if last_changed.as_ref().is_none_or(|(_, prev_timestamp)| {
                        *prev_timestamp < Timestamp::from_metadata(&metadata)
                    }) {
                        last_changed = Some((entry.into_path(), timestamp));
                    }
                }
            }
        }

        let timestamp = if let Some((path, timestamp)) = last_changed {
            debug!(
                "Computed cache info: {timestamp:?}, {commit:?}, {tags:?}, {env:?}, {directories:?}. Most recently modified: {}",
                path.user_display()
            );
            Some(timestamp)
        } else {
            None
        };

        Ok(Self {
            timestamp,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, rename_all = "kebab-case", deny_unknown_fields)]
pub enum CacheKey {
    /// Ex) `"Cargo.lock"` or `"**/*.toml"`
    Path(Cow<'static, str>),
    /// Ex) `{ file = "Cargo.lock" }` or `{ file = "**/*.toml", group = "k8s" }`
    File {
        file: Cow<'static, str>,
        /// If set, this key only applies when the named dependency group is enabled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupName>,
    },
    /// Ex) `{ dir = "src" }`
    Directory {
        dir: Cow<'static, str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupName>,
    },
    /// Ex) `{ git = true }` or `{ git = { commit = true, tags = false } }`
    Git {
        git: GitPattern,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupName>,
    },
    /// Ex) `{ env = "UV_CACHE_INFO" }`
    Environment {
        env: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupName>,
    },
}

impl CacheKey {
    /// The dependency group this cache key is scoped to, if any.
    ///
    /// A key with a group only applies when that group is enabled for the current project; a key
    /// without a group always applies.
    fn group(&self) -> Option<&GroupName> {
        match self {
            Self::Path(_) => None,
            Self::File { group, .. }
            | Self::Directory { group, .. }
            | Self::Git { group, .. }
            | Self::Environment { group, .. } => group.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, rename_all = "kebab-case", deny_unknown_fields)]
pub enum GitPattern {
    Bool(bool),
    Set(GitSet),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

        let cache_timestamp = || -> Result<_> { Ok(CacheInfo::from_directory(&dir)?.timestamp) };

        write_manifest("x/**")?;
        assert_eq!(cache_timestamp()?, None);
        let y = touch("x/y")?;
        assert_eq!(cache_timestamp()?, Some(y));
        let z = touch("x/z")?;
        assert_eq!(cache_timestamp()?, Some(z));

        // leaf entry symlink should be resolved
        let a = touch("../a")?;
        fs_err::os::unix::fs::symlink(dir.join("../a"), dir.join("x/a"))?;
        assert_eq!(cache_timestamp()?, Some(a));

        // symlink directories should not be followed while globbing
        let c = touch("../b/c")?;
        fs_err::os::unix::fs::symlink(dir.join("../b"), dir.join("x/b"))?;
        assert_eq!(cache_timestamp()?, Some(a));

        // no globs, should work as expected
        write_manifest("x/y")?;
        assert_eq!(cache_timestamp()?, Some(y));
        write_manifest("x/a")?;
        assert_eq!(cache_timestamp()?, Some(a));
        write_manifest("x/b/c")?;
        assert_eq!(cache_timestamp()?, Some(c));

        // symlink pointing to a directory
        write_manifest("x/*b*")?;
        assert_eq!(cache_timestamp()?, None);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use anyhow::Result;

    use uv_normalize::GroupName;

    use super::{CacheInfo, CacheKey};

    /// A consumer-provided cache-key override (as supplied via `tool.uv.cache-keys-package`)
    /// replaces the package's own `cache-keys`, without the package's `pyproject.toml` being
    /// consulted for keys. This is what lets a consumer change a dependency's invalidation
    /// behavior for its own build without affecting the dependency itself.
    #[test]
    fn override_replaces_package_cache_keys() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let dir = dir.path();

        // The package declares a file-based cache key and the file exists, so the default
        // (package-driven) computation produces a timestamp.
        fs_err::write(
            dir.join("pyproject.toml"),
            "[tool.uv]\ncache-keys = [\"data.txt\"]\n",
        )?;
        fs_err::write(dir.join("data.txt"), "")?;

        assert!(
            CacheInfo::from_directory(dir)?.timestamp.is_some(),
            "the package's own file cache key should fire"
        );

        // An empty override replaces the package's keys entirely: no keys are evaluated, so the
        // result is empty even though `pyproject.toml` declares a file key for an existing file.
        assert!(
            CacheInfo::from_directory_with_keys(dir, &[])?.is_empty(),
            "an empty override should replace the package's own cache keys"
        );

        // A non-empty override is honored in place of the package's keys.
        let overridden =
            CacheInfo::from_directory_with_keys(dir, &[CacheKey::Path(Cow::Borrowed("data.txt"))])?;
        assert!(
            overridden.timestamp.is_some(),
            "the overriding file cache key should fire"
        );

        Ok(())
    }

    /// A cache key scoped to a dependency group (`{ file = "...", group = "k8s" }`) only applies
    /// when that group is active for the current invocation. This is what lets a project set
    /// group-conditional `cache-keys` for its own build (e.g. `uv sync --group k8s`).
    #[test]
    fn group_scoped_cache_keys_filtered_by_active_groups() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let dir = dir.path();
        fs_err::write(dir.join("data.txt"), "")?;

        let k8s = "k8s".parse::<GroupName>()?;
        let keys = [CacheKey::File {
            file: Cow::Borrowed("data.txt"),
            group: Some(k8s.clone()),
        }];

        // Group inactive: the group-scoped key is excluded, so the result is empty.
        assert!(
            CacheInfo::from_directory_filtered(dir, Some(&keys), &|_| false)?.is_empty(),
            "a group-scoped key should be excluded when its group is inactive"
        );

        // Group active: the key applies and produces a timestamp.
        assert!(
            CacheInfo::from_directory_filtered(dir, Some(&keys), &|group| *group == k8s)?
                .timestamp
                .is_some(),
            "a group-scoped key should apply when its group is active"
        );

        Ok(())
    }
}
