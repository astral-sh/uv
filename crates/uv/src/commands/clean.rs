use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_fs::Normalized;
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Clear the cache.
pub(crate) fn clean(
    cache: &Cache,
    packages: &[PackageName],
    mut printer: Printer,
) -> Result<ExitStatus> {
    if !cache.root().exists() {
        writeln!(
            printer,
            "No cache found at: {}",
            cache.root().normalized_display().cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    if packages.is_empty() {
        writeln!(
            printer,
            "Clearing cache at: {}",
            cache.root().normalized_display().cyan()
        )?;

        let summary = cache.clear().with_context(|| {
            format!(
                "Failed to clear cache at: {}",
                cache.root().normalized_display()
            )
        })?;

        // Write a summary of the number of files and directories removed.
        match (summary.num_files, summary.num_dirs) {
            (0, 0) => {
                write!(printer, "No cache entries found")?;
            }
            (0, 1) => {
                write!(printer, "Removed 1 directory")?;
            }
            (0, num_dirs_removed) => {
                write!(printer, "Removed {num_dirs_removed} directories")?;
            }
            (1, _) => {
                write!(printer, "Removed 1 file")?;
            }
            (num_files_removed, _) => {
                write!(printer, "Removed {num_files_removed} files")?;
            }
        }

        // If any, write a summary of the total byte count removed.
        if summary.total_bytes > 0 {
            let bytes = if summary.total_bytes < 1024 {
                format!("{}B", summary.total_bytes)
            } else {
                let (bytes, unit) = human_readable_bytes(summary.total_bytes);
                format!("{bytes:.1}{unit}")
            };
            write!(printer, " ({})", bytes.green())?;
        }

        writeln!(printer)?;
    } else {
        for package in packages {
            let summary = cache.remove(package)?;

            // Write a summary of the number of files and directories removed.
            match (summary.num_files, summary.num_dirs) {
                (0, 0) => {
                    write!(printer, "No cache entries found for {}", package.cyan())?;
                }
                (0, 1) => {
                    write!(printer, "Removed 1 directory for {}", package.cyan())?;
                }
                (0, num_dirs_removed) => {
                    write!(
                        printer,
                        "Removed {num_dirs_removed} directories for {}",
                        package.cyan()
                    )?;
                }
                (1, _) => {
                    write!(printer, "Removed 1 file for {}", package.cyan())?;
                }
                (num_files_removed, _) => {
                    write!(
                        printer,
                        "Removed {num_files_removed} files for {}",
                        package.cyan()
                    )?;
                }
            }

            // If any, write a summary of the total byte count removed.
            if summary.total_bytes > 0 {
                let bytes = if summary.total_bytes < 1024 {
                    format!("{}B", summary.total_bytes)
                } else {
                    let (bytes, unit) = human_readable_bytes(summary.total_bytes);
                    format!("{bytes:.1}{unit}")
                };
                write!(printer, " ({})", bytes.green())?;
            }

            writeln!(printer)?;
        }
    }

    Ok(ExitStatus::Success)
}

/// Formats a number of bytes into a human readable SI-prefixed size.
///
/// Returns a tuple of `(quantity, units)`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn human_readable_bytes(bytes: u64) -> (f32, &'static str) {
    static UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let bytes = bytes as f32;
    let i = ((bytes.log2() / 10.0) as usize).min(UNITS.len() - 1);
    (bytes / 1024_f32.powi(i as i32), UNITS[i])
}
