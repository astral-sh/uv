use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pub size: Option<u64>,
}

impl RecordEntry {
    /// Returns `Ok(false)` if the file on disk no longer matches this RECORD entry.
    pub fn matches_file(&self, site_packages: &Path) -> io::Result<bool> {
        let Some(expected) = self.hash.as_deref().and_then(|h| h.strip_prefix("sha256=")) else {
            return Ok(true);
        };
        let mut file = match File::open(site_packages.join(&self.path)) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err),
        };
        if let Some(size) = self.size {
            if file.metadata()?.len() != size {
                return Ok(false);
            }
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(BASE64URL_NOPAD.encode(&hasher.finalize()) == expected)
    }
}
