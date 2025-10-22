use std::fmt::Write;

use anyhow::Result;

use uv_cache::Cache;

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

    // Walk the entire cache root
    let total_bytes: u64 = walkdir::WalkDir::new(cache.root())
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| match entry.metadata() {
            Ok(metadata) if metadata.is_file() => Some(metadata.len()),
            _ => None,
        })
        .sum();

    // Output in requested format
    if human_readable {
        let (size, unit) = human_readable_bytes(total_bytes);
        writeln!(printer.stdout(), "{size:.1} {unit}")?;
    } else {
        // Raw bytes (script-friendly)
        writeln!(printer.stdout(), "{total_bytes}")?;
    }

    Ok(ExitStatus::Success)
}
