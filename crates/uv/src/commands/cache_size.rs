use std::fmt::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anstream::stream::IsTerminal;
use anyhow::Result;
use diskus::DiskUsage;

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;
use uv_cache::Cache;
use uv_cli::CacheSizeOutputFormat;
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;

/// Display the total size of the cache.
pub(crate) fn cache_size(
    cache: &Cache,
    output_format: CacheSizeOutputFormat,
    inodes: bool,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::CacheSize) {
        warn_user!(
            "`uv cache size` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::CacheSize
        );
    }

    let human_readable = match output_format {
        CacheSizeOutputFormat::Auto => std::io::stdout().is_terminal(),
        CacheSizeOutputFormat::Human => true,
        CacheSizeOutputFormat::Machine => false,
    };

    if !cache.root().exists() {
        if inodes || !human_readable {
            writeln!(printer.stdout_important(), "0")?;
        } else {
            writeln!(printer.stdout_important(), "0B")?;
        }
        return Ok(ExitStatus::Success);
    }

    if inodes {
        // Count the number of inodes (filesystem entries) used by the cache.
        let count = count_inodes(cache.root());
        if human_readable {
            // Format with thousands separator for readability.
            writeln!(printer.stdout_important(), "{}", format_inode_count(count))?;
        } else {
            writeln!(printer.stdout_important(), "{count}")?;
        }
        return Ok(ExitStatus::Success);
    }

    let disk_usage = DiskUsage::new(vec![cache.root().to_path_buf()]);

    let total_bytes = disk_usage.count_ignoring_errors();

    if human_readable {
        let bytes = human_readable_bytes(total_bytes);
        writeln!(printer.stdout_important(), "{bytes:.1}")?;
    } else {
        writeln!(printer.stdout_important(), "{total_bytes}")?;
    }

    Ok(ExitStatus::Success)
}

/// Recursively count the number of filesystem entries (inodes) under `path`.
///
/// On Unix, files with multiple hard links sharing the same device and inode number
/// are only counted once.
fn count_inodes(path: &Path) -> u64 {
    #[cfg(unix)]
    use rustc_hash::FxHashSet;
    #[cfg(unix)]
    use std::sync::Mutex;

    let count = AtomicU64::new(0);
    #[cfg(unix)]
    let seen_inodes = Mutex::new(FxHashSet::default());

    let mut builder = ignore::WalkBuilder::new(path);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_ignore(false)
        .git_exclude(false)
        .require_git(false)
        .follow_links(false);

    builder.build_parallel().run(|| {
        let count = &count;
        #[cfg(unix)]
        let seen_inodes = &seen_inodes;

        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };

            let Ok(metadata) = entry.metadata() else {
                return ignore::WalkState::Continue;
            };

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.is_file() && metadata.nlink() > 1 {
                    let id = (metadata.dev(), metadata.ino());
                    if seen_inodes.lock().unwrap().insert(id) {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    count.fetch_add(1, Ordering::Relaxed);
                }
            }

            #[cfg(not(unix))]
            {
                let _ = metadata;
                count.fetch_add(1, Ordering::Relaxed);
            }

            ignore::WalkState::Continue
        })
    });

    count.into_inner()
}

/// Format an inode count with thousands separators for human-readable display.
///
/// For example, `1234567` becomes `1,234,567`.
fn format_inode_count(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}
