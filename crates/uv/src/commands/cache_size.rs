use std::fmt::Write;

use anyhow::Result;
use diskus::DiskUsage;

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;
use uv_cache::Cache;
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;

/// Display the total size of the cache.
pub(crate) fn cache_size(
    cache: &Cache,
    human_readable: bool,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::CacheSize) {
        warn_user!(
            "`uv cache size` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::CacheSize
        );
    }

    if !cache.root().exists() {
        if human_readable {
            writeln!(printer.stdout_important(), "0B")?;
        } else {
            writeln!(printer.stdout_important(), "0")?;
        }
        return Ok(ExitStatus::Success);
    }

    let disk_usage = DiskUsage::new(vec![cache.root().to_path_buf()]);

    let total_bytes = disk_usage.count_ignoring_errors();

    if human_readable {
        let (bytes, unit) = human_readable_bytes(total_bytes);
        writeln!(printer.stdout_important(), "{bytes:.1}{unit}")?;
    } else {
        writeln!(printer.stdout_important(), "{total_bytes}")?;
    }

    Ok(ExitStatus::Success)
}
