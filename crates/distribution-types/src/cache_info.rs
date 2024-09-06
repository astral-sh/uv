use crate::timestamp::Timestamp;

use serde::Deserialize;
use std::cmp::max;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CacheInfo {
    timestamp: Option<Timestamp>,
    commit: Option<CommitInfo>,
}

impl CacheInfo {
    pub fn from_timestamp(timestamp: Timestamp) -> Self {
        Self {
            timestamp: Some(timestamp),
            ..Self::default()
        }
    }

    pub fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = fs_err::metadata(path)?;
        if metadata.is_file() {
            Self::from_file(path)
        } else {
            Self::from_directory(path)
        }
    }

    pub fn from_directory(directory: &Path) -> io::Result<Self> {
        let timestamp = {
            // Compute the modification timestamp for the `pyproject.toml`, `setup.py`, and
            // `setup.cfg` files, if they exist.
            let pyproject_toml = directory
                .join("pyproject.toml")
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
                .as_ref()
                .map(Timestamp::from_metadata);

            let setup_py = directory
                .join("setup.py")
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
                .as_ref()
                .map(Timestamp::from_metadata);

            let setup_cfg = directory
                .join("setup.cfg")
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
                .as_ref()
                .map(Timestamp::from_metadata);

            let mut timestamp = max(pyproject_toml, max(setup_py, setup_cfg));

            // Compute the modification timestamps of any additional cache keys.
            if let Ok(contents) = fs_err::read_to_string(directory.join("pyproject.toml")) {
                if let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) {
                    if let Some(tool) = pyproject_toml.tool {
                        if let Some(tool_uv) = tool.uv {
                            for key in tool_uv.cache_keys.unwrap_or_default() {
                                let key = directory.join(key);
                                let key_timestamp = key
                                    .metadata()
                                    .ok()
                                    .filter(std::fs::Metadata::is_file)
                                    .as_ref()
                                    .map(Timestamp::from_metadata);
                                timestamp = max(timestamp, key_timestamp);
                            }
                        }
                    }
                }
            }

            timestamp
        };

        Ok(Self {
            timestamp,
            ..Self::default()
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let metadata = fs_err::metadata(path.as_ref())?;
        let timestamp = Timestamp::from_metadata(&metadata);
        Ok(Self {
            timestamp: Some(timestamp),
            ..Self::default()
        })
    }

    #[must_use]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    #[must_use]
    pub fn with_commit(mut self, commit: CommitInfo) -> Self {
        self.commit = Some(commit);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.timestamp.is_none() && self.commit.is_none()
    }
}

/// Information about the git repository where uv was built from.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CommitInfo {
    short_commit_hash: String,
    commit_hash: String,
    commit_date: String,
    last_tag: Option<String>,
    commits_since_last_tag: u32,
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
    cache_keys: Option<Vec<PathBuf>>,
}
