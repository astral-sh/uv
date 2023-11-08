use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Metadata for a distribution that was installed via a direct URL.
///
/// See: <https://packaging.python.org/en/latest/specifications/direct-url-data-structure/>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectUrl {
    /// The direct URL is a path to an archive. For example:
    /// ```json
    /// {"archive_info": {"hash": "sha256=75909db2664838d015e3d9139004ee16711748a52c8f336b52882266540215d8", "hashes": {"sha256": "75909db2664838d015e3d9139004ee16711748a52c8f336b52882266540215d8"}}, "url": "https://files.pythonhosted.org/packages/b8/8b/31273bf66016be6ad22bb7345c37ff350276cfd46e389a0c2ac5da9d9073/wheel-0.41.2-py3-none-any.whl"}
    /// ```
    ArchiveUrl {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ArchiveInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VcsInfo {
    pub vcs: VcsKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_revision: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VcsKind {
    Git,
    Hg,
    Bzr,
    Svn,
}

