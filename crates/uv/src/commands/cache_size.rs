use std::fmt::Write;
use std::num::NonZero;

use anyhow::Result;
use diskus::{FilesizeType, Walk};

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;
use uv_cache::Cache;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;

/// Display the total size of the cache.
pub(crate) fn cache_size(
    cache: &Cache,
    human_readable: bool,
    threads: Option<usize>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeatures::CACHE_SIZE) {
        warn_user!(
            "`uv cache size` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::CACHE_SIZE
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

    let num_threads = threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(NonZero::get)
            .unwrap_or(1)
    });

    tracing::info!("Using {} threads to calculate cache size", num_threads);

    let root_path_buf = &[cache.root().to_path_buf()];

    let walk = Walk::new(root_path_buf, num_threads, FilesizeType::ApparentSize);
    let (total_bytes, _) = walk.run();

    if human_readable {
        let (bytes, unit) = human_readable_bytes(total_bytes);
        writeln!(printer.stdout_important(), "{bytes:.1}{unit}")?;
    } else {
        writeln!(printer.stdout_important(), "{total_bytes}")?;
    }

    Ok(ExitStatus::Success)
}
