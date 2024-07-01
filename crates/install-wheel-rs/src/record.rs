use serde::{Deserialize, Serialize};

/// Line in a RECORD file
/// <https://www.python.org/dev/peps/pep-0376/#record>
///
/// ```csv
/// tqdm/cli.py,sha256=x_c8nmc4Huc-lKEsAXj78ZiyqSJ9hJ71j7vltY67icw,10509
/// tqdm-4.62.3.dist-info/RECORD,,
/// ```
#[derive(Deserialize, Serialize, PartialOrd, PartialEq, Ord, Eq)]
pub struct RecordEntry {
    pub path: String,
    pub hash: Option<String>,
    #[allow(dead_code)]
    pub size: Option<u64>,
}
