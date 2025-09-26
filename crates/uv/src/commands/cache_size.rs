use std::fmt::Write;

use anyhow::Result;

use uv_cache::{Cache, CacheBucket};

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Display the total size of the cache.
pub(crate) fn cache_size(
    cache: &Cache,
    human_readable: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    if !cache.root().exists() {
        // Return 0 bytes for non-existent cache
        if human_readable {
            writeln!(printer.stdout(), "0 B")?;
        } else {
            writeln!(printer.stdout(), "0")?;
        }
        return Ok(ExitStatus::Success);
    }

    let mut total_bytes = 0u64;

    // Traverse all cache buckets and sum their sizes
    for bucket in CacheBucket::iter() {
        let bucket_path = cache.bucket(bucket);
        if !bucket_path.exists() {
            continue;
        }

        // Walk directory tree and sum file sizes
        for entry in walkdir::WalkDir::new(&bucket_path) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue, // Skip entries we can't read
            };

            if entry.file_type().is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total_bytes += metadata.len();
                }
            }
        }
    }

    // Output in requested format
    if human_readable {
        let (size, unit) = human_readable_bytes(total_bytes);
        writeln!(printer.stdout(), "{:.1} {}", size, unit)?;
    } else {
        // Raw bytes (script-friendly)
        writeln!(printer.stdout(), "{}", total_bytes)?;
    }

    Ok(ExitStatus::Success)
}
