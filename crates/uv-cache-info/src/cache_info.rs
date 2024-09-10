use crate::commit_info::CacheCommit;
use crate::timestamp::Timestamp;

use glob::MatchOptions;
use serde::Deserialize;
use std::cmp::max;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// The information used to determine whether a built distribution is up-to-date, based on the
/// timestamps of relevant files, the current commit of a repository, etc.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(try_from = "CacheInfoWire")]
pub struct CacheInfo {
    /// The timestamp of the most recent `ctime` of any relevant files, at the time of the build.
    /// The timestamp will typically be the maximum of the `ctime` values of the `pyproject.toml`,
    /// `setup.py`, and `setup.cfg` files, if they exist; however, users can provide additional
    /// files to timestamp via the `cache-keys` field.
    timestamp: Option<Timestamp>,
    /// The commit at which the distribution was built.
    commit: Option<CacheCommit>,
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
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = fs_err::metadata(path)?;
        if metadata.is_file() {
            Self::from_file(path)
        } else {
            Self::from_directory(path)
        }
    }

    /// Compute the cache info for a given directory.
    pub fn from_directory(directory: &Path) -> io::Result<Self> {
        let mut commit = None;
        let mut timestamp = None;

        // Read the cache keys.
        let cache_keys =
            if let Ok(contents) = fs_err::read_to_string(directory.join("pyproject.toml")) {
                if let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) {
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
                CacheKey::Path("pyproject.toml".to_string()),
                CacheKey::Path("setup.py".to_string()),
                CacheKey::Path("setup.cfg".to_string()),
            ]
        });

        // Incorporate any additional timestamps or VCS information.
        for cache_key in &cache_keys {
            match cache_key {
                CacheKey::Path(file) | CacheKey::File { file } => {
                    if file.chars().any(|c| matches!(c, '*' | '?' | '[')) {
                        // Treat the path as a glob.
                        let path = directory.join(file);
                        let Some(pattern) = path.to_str() else {
                            warn!("Failed to convert pattern to string: {}", path.display());
                            continue;
                        };
                        let paths = match glob::glob_with(
                            pattern,
                            MatchOptions {
                                case_sensitive: true,
                                require_literal_separator: true,
                                require_literal_leading_dot: false,
                            },
                        ) {
                            Ok(paths) => paths,
                            Err(err) => {
                                warn!("Failed to parse glob pattern: {err}");
                                continue;
                            }
                        };
                        for entry in paths {
                            let entry = match entry {
                                Ok(entry) => entry,
                                Err(err) => {
                                    warn!("Failed to read glob entry: {err}");
                                    continue;
                                }
                            };
                            let metadata = match entry.metadata() {
                                Ok(metadata) => metadata,
                                Err(err) => {
                                    warn!("Failed to read metadata for glob entry: {err}");
                                    continue;
                                }
                            };
                            if metadata.is_file() {
                                timestamp =
                                    max(timestamp, Some(Timestamp::from_metadata(&metadata)));
                            } else {
                                warn!(
                                    "Expected file for cache key, but found directory: `{}`",
                                    entry.display()
                                );
                            }
                        }
                    } else {
                        // Treat the path as a file.
                        let path = directory.join(file);
                        let metadata = match path.metadata() {
                            Ok(metadata) => metadata,
                            Err(err) => {
                                warn!("Failed to read metadata for file: {err}");
                                continue;
                            }
                        };
                        if metadata.is_file() {
                            timestamp = max(timestamp, Some(Timestamp::from_metadata(&metadata)));
                        } else {
                            warn!(
                                "Expected file for cache key, but found directory: `{}`",
                                path.display()
                            );
                        }
                    }
                }
                CacheKey::Git { git: true } => match CacheCommit::from_repository(directory) {
                    Ok(commit_info) => commit = Some(commit_info),
                    Err(err) => {
                        debug!("Failed to read the current commit: {err}");
                    }
                },
                CacheKey::Git { git: false } => {}
            }
        }

        Ok(Self { timestamp, commit })
    }

    /// Compute the cache info for a given file, assumed to be a binary or source distribution
    /// represented as (e.g.) a `.whl` or `.tar.gz` archive.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let metadata = fs_err::metadata(path.as_ref())?;
        let timestamp = Timestamp::from_metadata(&metadata);
        Ok(Self {
            timestamp: Some(timestamp),
            ..Self::default()
        })
    }

    pub fn is_empty(&self) -> bool {
        self.timestamp.is_none() && self.commit.is_none()
    }
}

#[derive(Debug, serde::Deserialize)]
struct TimestampCommit {
    timestamp: Option<Timestamp>,
    commit: Option<CacheCommit>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum CacheInfoWire {
    /// For backwards-compatibility, enable deserializing [`CacheInfo`] structs that are solely
    /// represented by a timestamp.
    Timestamp(Timestamp),
    /// A [`CacheInfo`] struct that includes both a timestamp and a commit.
    TimestampCommit(TimestampCommit),
}

impl From<CacheInfoWire> for CacheInfo {
    fn from(wire: CacheInfoWire) -> Self {
        match wire {
            CacheInfoWire::Timestamp(timestamp) => Self {
                timestamp: Some(timestamp),
                ..Self::default()
            },
            CacheInfoWire::TimestampCommit(TimestampCommit { timestamp, commit }) => {
                Self { timestamp, commit }
            }
        }
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
    Path(String),
    /// Ex) `{ file = "Cargo.lock" }` or `{ file = "**/*.toml" }`
    File { file: String },
    /// Ex) `{ git = true }`
    Git { git: bool },
}

pub enum FilePattern {
    Glob(String),
    Path(PathBuf),
}
