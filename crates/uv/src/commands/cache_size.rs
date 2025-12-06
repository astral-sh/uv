use std::collections::HashSet;
use std::fmt::Write;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::Result;
use crossbeam_channel as channel;
use rayon::{self, prelude::*};

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

    let total_bytes = calculate_cache_size(num_threads, cache.root());

    if human_readable {
        let (bytes, unit) = human_readable_bytes(total_bytes);
        writeln!(printer.stdout_important(), "{bytes:.1}{unit}")?;
    } else {
        writeln!(printer.stdout_important(), "{total_bytes}")?;
    }

    Ok(ExitStatus::Success)
}

/// Calculate the total size of the cache using multiple threads.
///
/// Vendored from <https://github.com/sharkdp/diskus>.
fn calculate_cache_size(num_threads: usize, path: &Path) -> u64 {
    let (tx, rx) = channel::unbounded();

    let receiver_thread = thread::spawn(move || {
        let mut total = 0;
        let mut ids = HashSet::new();
        for (unique_id, size) in rx {
            if let Some(unique_id) = unique_id {
                if ids.insert(unique_id) {
                    total += size;
                }
            } else {
                total += size;
            }
        }
        total
    });

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .unwrap();
    pool.install(|| walk(tx, &[path.to_path_buf()]));

    receiver_thread.join().unwrap()
}

fn walk(tx: channel::Sender<(Option<UniqueID>, u64)>, entries: &[PathBuf]) {
    entries.into_par_iter().for_each_with(tx, |tx_ref, entry| {
        if let Ok(metadata) = entry.symlink_metadata() {
            let unique_id = generate_unique_id(&metadata);

            let size = &metadata.len();

            tx_ref.send((unique_id, *size)).unwrap();

            if metadata.is_dir() {
                let mut children = vec![];
                if let Ok(child_entries) = fs_err::read_dir(entry) {
                    for child_entry in child_entries.flatten() {
                        children.push(child_entry.path());
                    }
                }

                walk(tx_ref.clone(), &children[..]);
            }
        }
    });
}

#[derive(Eq, PartialEq, Hash)]
struct UniqueID {
    device: u64,
    inode: u64,
}

#[cfg(not(windows))]
fn generate_unique_id(metadata: &std::fs::Metadata) -> Option<UniqueID> {
    use std::os::unix::fs::MetadataExt;
    // If the entry has more than one hard link, generate
    // a unique ID consisting of device and inode in order
    // not to count this entry twice.
    if metadata.is_file() && metadata.nlink() > 1 {
        Some(UniqueID {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    } else {
        None
    }
}

#[cfg(windows)]
fn generate_unique_id(_metadata: &std::fs::Metadata) -> Option<UniqueID> {
    // Windows-internal tools such as Powershell, Explorer or `dir` are not respecting hardlinks
    // or junction points when determining the size of a directory. `diskus` does the same and
    // counts such entries multiple times (on Unix systems, multiple hardlinks to a single file are
    // counted just once).
    None
}
