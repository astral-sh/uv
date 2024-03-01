use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The installation paths returned by `sysconfig.get_paths()`.
///
/// See: <https://docs.python.org/3.12/library/sysconfig.html#installation-paths>
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SysconfigPaths {
    pub stdlib: PathBuf,
    pub platstdlib: PathBuf,
    pub purelib: PathBuf,
    pub platlib: PathBuf,
    pub include: PathBuf,
    pub platinclude: PathBuf,
    pub scripts: PathBuf,
    pub data: PathBuf,
}
