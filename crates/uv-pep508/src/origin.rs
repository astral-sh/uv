use std::path::{Path, PathBuf};

use uv_normalize::PackageName;

/// The origin of a dependency, e.g., a `-r requirements.txt` file.
#[derive(
    Hash, Debug, Clone, Eq, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "rkyv", rkyv(derive(Debug)))]
#[serde(rename_all = "kebab-case")]
pub enum RequirementOrigin {
    /// The requirement was provided via a standalone file (e.g., a `requirements.txt` file).
    File(#[cfg_attr(feature = "rkyv", rkyv(with = rkyv::with::AsString))] PathBuf),
    /// The requirement was provided via a local project (e.g., a `pyproject.toml` file).
    Project(
        #[cfg_attr(feature = "rkyv", rkyv(with = rkyv::with::AsString))] PathBuf,
        PackageName,
    ),
    /// The requirement was provided via a workspace.
    Workspace,
}

impl RequirementOrigin {
    /// Returns the path of the requirement origin.
    pub fn path(&self) -> &Path {
        match self {
            RequirementOrigin::File(path) => path.as_path(),
            RequirementOrigin::Project(path, _) => path.as_path(),
            // Multiple `toml` files are merged and difficult to track.
            RequirementOrigin::Workspace => Path::new("(workspace)"),
        }
    }
}
