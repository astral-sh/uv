use crate::commit_info::CommitInfo;
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
        let mut commit = None;
        let mut timestamp = None;

        // Always compute the modification timestamp for the `pyproject.toml`, `setup.py`, and
        // `setup.cfg` files, if they exist.
        timestamp = {
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

            max(pyproject_toml, max(setup_py, setup_cfg))
        };

        // Read the cache keys.
        let cache_keys =
            if let Ok(contents) = fs_err::read_to_string(directory.join("pyproject.toml")) {
                if let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) {
                    pyproject_toml
                        .tool
                        .and_then(|tool| tool.uv)
                        .and_then(|tool_uv| tool_uv.cache_keys)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

        // Incorporate any additional timestamps or VCS information.
        for cache_key in &cache_keys {
            match cache_key {
                CacheKey::File(path) => {
                    let key_timestamp = path
                        .metadata()
                        .ok()
                        .filter(std::fs::Metadata::is_file)
                        .as_ref()
                        .map(Timestamp::from_metadata);
                    timestamp = max(timestamp, key_timestamp);
                }
                CacheKey::Git => {
                    commit = CommitInfo::from_repository(directory);
                }
            }
        }

        Ok(Self { timestamp, commit })
    }

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

#[derive(Debug)]
enum CacheKey {
    /// Ex) `{ file = "Cargo.lock" }` or `"Cargo.lock"`
    File(PathBuf),
    /// Ex) `{ git = true }`
    Git,
}

impl<'de> serde::de::Deserialize<'de> for CacheKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CacheKeyHelper {
            FileMap { file: PathBuf },
            GitMap { git: bool },
            SimpleFile(PathBuf),
        }

        let helper = CacheKeyHelper::deserialize(deserializer)?;
        match helper {
            CacheKeyHelper::FileMap { file } => Ok(CacheKey::File(file)),
            CacheKeyHelper::GitMap { git } => {
                if git {
                    Ok(CacheKey::Git)
                } else {
                    Err(serde::de::Error::custom(
                        "Invalid value for git key, expected true",
                    ))
                }
            }
            CacheKeyHelper::SimpleFile(file) => Ok(CacheKey::File(file)),
        }
    }
}
