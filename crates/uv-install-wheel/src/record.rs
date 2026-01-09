use serde::{Deserialize, Serialize};

/// Line in a RECORD file.
///
/// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
///
/// ```csv
/// tqdm/cli.py,sha256=x_c8nmc4Huc-lKEsAXj78ZiyqSJ9hJ71j7vltY67icw,10509
/// tqdm-4.62.3.dist-info/RECORD,,
/// ```
#[derive(Deserialize, Serialize, PartialOrd, PartialEq, Ord, Eq)]
pub struct RecordEntry {
    pub path: String,
    pub hash: Option<String>,
    pub size: Option<u64>,
}

impl RecordEntry {
    /// Create a new record entry with a hash and size.
    pub fn new(path: String, hash: String, size: u64) -> Self {
        Self {
            path,
            hash: Some(hash),
            size: Some(size),
        }
    }

    /// Create a record entry for a file that should not be hashed (e.g., the RECORD file itself).
    pub fn unhashed(path: String) -> Self {
        Self {
            path,
            hash: None,
            size: None,
        }
    }
}
