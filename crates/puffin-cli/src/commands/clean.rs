use std::fmt::Write;

use anyhow::{Context, Result};
use fs_err as fs;
use owo_colors::OwoColorize;

use puffin_cache::Cache;
use puffin_normalize::PackageName;

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
            cache.root().display().cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    if packages.is_empty() {
        writeln!(
            printer,
            "Clearing cache at: {}",
            cache.root().display().cyan()
        )?;
        fs::remove_dir_all(cache.root())
            .with_context(|| format!("Failed to clear cache at: {}", cache.root().display()))?;
    } else {
        for package in packages {
            let count = cache.purge(package)?;
            match count {
                0 => writeln!(printer, "No entries found for package: {}", package.cyan())?,
                1 => writeln!(printer, "Cleared 1 entry for package: {}", package.cyan())?,
                count => writeln!(
                    printer,
                    "Cleared {count} entries for package: {}",
                    package.cyan()
                )?,
            }
        }
    }

    Ok(ExitStatus::Success)
}
