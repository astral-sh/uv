//! Code for representing uv's release version number.
// See also <https://github.com/astral-sh/ruff/blob/8118d29419055b779719cc96cdf3dacb29ac47c9/crates/ruff/src/version.rs>
use std::fmt;

use serde::Serialize;
use uv_pep508::{PackageName, uv_pep440::Version};

/// Information about the git repository where uv was built from.
#[derive(Serialize)]
pub(crate) struct CommitInfo {
    short_commit_hash: String,
    commit_hash: String,
    commit_date: String,
    last_tag: Option<String>,
    commits_since_last_tag: u32,
}

/// uv's version.
#[derive(Serialize)]
pub struct VersionInfo {
    /// Name of the package (or "uv" if printing uv's own version)
    pub package_name: Option<String>,
    /// version, such as "0.5.1"
    version: String,
    /// Information about the git commit we may have been built from.
    ///
    /// `None` if not built from a git repo or if retrieval failed.
    commit_info: Option<CommitInfo>,
}

impl VersionInfo {
    pub fn new(package_name: Option<&PackageName>, version: &Version) -> Self {
        Self {
            package_name: package_name.map(ToString::to_string),
            version: version.to_string(),
            commit_info: None,
        }
    }
}

impl fmt::Display for VersionInfo {
    /// Formatted version information: "<version>[+<commits>] (<commit> <date>)"
    ///
    /// This is intended for consumption by `clap` to provide `uv --version`,
    /// and intentionally omits the name of the package
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)?;
        if let Some(ci) = &self.commit_info {
            write!(f, "{ci}")?;
        }
        Ok(())
    }
}

impl fmt::Display for CommitInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.commits_since_last_tag > 0 {
            write!(f, "+{}", self.commits_since_last_tag)?;
        }
        write!(f, " ({} {})", self.short_commit_hash, self.commit_date)?;
        Ok(())
    }
}

impl From<VersionInfo> for clap::builder::Str {
    fn from(val: VersionInfo) -> Self {
        val.to_string().into()
    }
}

/// Returns information about uv's version.
pub fn uv_self_version() -> VersionInfo {
    // Environment variables are only read at compile-time
    macro_rules! option_env_str {
        ($name:expr) => {
            option_env!($name).map(|s| s.to_string())
        };
    }

    // This version is pulled from Cargo.toml and set by Cargo
    let version = uv_version::version().to_string();

    // Commit info is pulled from git and set by `build.rs`
    let commit_info = option_env_str!("UV_COMMIT_HASH").map(|commit_hash| CommitInfo {
        short_commit_hash: option_env_str!("UV_COMMIT_SHORT_HASH").unwrap(),
        commit_hash,
        commit_date: option_env_str!("UV_COMMIT_DATE").unwrap(),
        last_tag: option_env_str!("UV_LAST_TAG"),
        commits_since_last_tag: option_env_str!("UV_LAST_TAG_DISTANCE")
            .as_deref()
            .map_or(0, |value| value.parse::<u32>().unwrap_or(0)),
    });

    VersionInfo {
        package_name: Some("uv".to_owned()),
        version,
        commit_info,
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_json_snapshot, assert_snapshot};

    use super::{CommitInfo, VersionInfo};

    #[test]
    fn version_formatting() {
        let version = VersionInfo {
            package_name: Some("uv".to_string()),
            version: "0.0.0".to_string(),
            commit_info: None,
        };
        assert_snapshot!(version, @"0.0.0");
    }

    #[test]
    fn version_formatting_with_commit_info() {
        let version = VersionInfo {
            package_name: Some("uv".to_string()),
            version: "0.0.0".to_string(),
            commit_info: Some(CommitInfo {
                short_commit_hash: "53b0f5d92".to_string(),
                commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
                last_tag: Some("v0.0.1".to_string()),
                commit_date: "2023-10-19".to_string(),
                commits_since_last_tag: 0,
            }),
        };
        assert_snapshot!(version, @"0.0.0 (53b0f5d92 2023-10-19)");
    }

    #[test]
    fn version_formatting_with_commits_since_last_tag() {
        let version = VersionInfo {
            package_name: Some("uv".to_string()),
            version: "0.0.0".to_string(),
            commit_info: Some(CommitInfo {
                short_commit_hash: "53b0f5d92".to_string(),
                commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
                last_tag: Some("v0.0.1".to_string()),
                commit_date: "2023-10-19".to_string(),
                commits_since_last_tag: 24,
            }),
        };
        assert_snapshot!(version, @"0.0.0+24 (53b0f5d92 2023-10-19)");
    }

    #[test]
    fn version_serializable() {
        let version = VersionInfo {
            package_name: Some("uv".to_string()),
            version: "0.0.0".to_string(),
            commit_info: Some(CommitInfo {
                short_commit_hash: "53b0f5d92".to_string(),
                commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
                last_tag: Some("v0.0.1".to_string()),
                commit_date: "2023-10-19".to_string(),
                commits_since_last_tag: 0,
            }),
        };
        assert_json_snapshot!(version, @r#"
    {
      "package_name": "uv",
      "version": "0.0.0",
      "commit_info": {
        "short_commit_hash": "53b0f5d92",
        "commit_hash": "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7",
        "commit_date": "2023-10-19",
        "last_tag": "v0.0.1",
        "commits_since_last_tag": 0
      }
    }
    "#);
    }
}
