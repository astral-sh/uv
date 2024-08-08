use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;

/// Metadata for a distribution that was installed via a direct URL.
///
/// See: <https://packaging.python.org/en/latest/specifications/direct-url-data-structure/>
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum DirectUrl {
    /// The direct URL is a local directory. For example:
    /// ```json
    /// {"url": "file:///home/user/project", "dir_info": {}}
    /// ```
    LocalDirectory { url: String, dir_info: DirInfo },
    /// The direct URL is a path to an archive. For example:
    /// ```json
    /// {"archive_info": {"hash": "sha256=75909db2664838d015e3d9139004ee16711748a52c8f336b52882266540215d8", "hashes": {"sha256": "75909db2664838d015e3d9139004ee16711748a52c8f336b52882266540215d8"}}, "url": "https://files.pythonhosted.org/packages/b8/8b/31273bf66016be6ad22bb7345c37ff350276cfd46e389a0c2ac5da9d9073/wheel-0.41.2-py3-none-any.whl"}
    /// ```
    ArchiveUrl {
        /// The URL without parsed information (such as the Git revision or subdirectory).
        ///
        /// For example, for `pip install git+https://github.com/tqdm/tqdm@cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44#subdirectory=.`,
        /// the URL is `https://github.com/tqdm/tqdm`.
        url: String,
        archive_info: ArchiveInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdirectory: Option<PathBuf>,
    },
    /// The direct URL is path to a VCS repository. For example:
    /// ```json
    /// {"url": "https://github.com/pallets/flask.git", "vcs_info": {"commit_id": "8d9519df093864ff90ca446d4af2dc8facd3c542", "vcs": "git"}}
    /// ```
    VcsUrl {
        url: String,
        vcs_info: VcsInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdirectory: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DirInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ArchiveInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VcsInfo {
    pub vcs: VcsKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_revision: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VcsKind {
    Git,
    Hg,
    Bzr,
    Svn,
}

impl std::fmt::Display for VcsKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git => write!(f, "git"),
            Self::Hg => write!(f, "hg"),
            Self::Bzr => write!(f, "bzr"),
            Self::Svn => write!(f, "svn"),
        }
    }
}

impl TryFrom<&DirectUrl> for Url {
    type Error = url::ParseError;

    fn try_from(value: &DirectUrl) -> Result<Self, Self::Error> {
        match value {
            DirectUrl::LocalDirectory { url, .. } => Self::parse(url),
            DirectUrl::ArchiveUrl {
                url,
                subdirectory,
                archive_info: _,
            } => {
                let mut url = Self::parse(url)?;
                if let Some(subdirectory) = subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
                }
                Ok(url)
            }
            DirectUrl::VcsUrl {
                url,
                vcs_info,
                subdirectory,
            } => {
                let mut url = Self::parse(&format!("{}+{}", vcs_info.vcs, url))?;
                if let Some(commit_id) = &vcs_info.commit_id {
                    url.set_path(&format!("{}@{commit_id}", url.path()));
                } else if let Some(requested_revision) = &vcs_info.requested_revision {
                    url.set_path(&format!("{}@{requested_revision}", url.path()));
                }
                if let Some(subdirectory) = subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
                }
                Ok(url)
            }
        }
    }
}
