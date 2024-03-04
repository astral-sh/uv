use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The paths associated with an installation scheme, typically returned by `sysconfig.get_paths()`.
///
/// See: <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/models/scheme.py#L12>
///
/// See: <https://docs.python.org/3.12/library/sysconfig.html#installation-paths>
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Scheme {
    pub purelib: PathBuf,
    pub platlib: PathBuf,
    pub scripts: PathBuf,
    pub data: PathBuf,
    pub include: PathBuf,
}
