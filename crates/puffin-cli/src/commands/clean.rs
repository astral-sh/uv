use std::fmt::Write;

use anyhow::{Context, Result};
use colored::Colorize;
use fs_err as fs;

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
            format!("{}", cache.root().display()).cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    if packages.is_empty() {
        writeln!(
            printer,
            "Clearing cache at: {}",
            format!("{}", cache.root().display()).cyan()
        )?;
        fs::remove_dir_all(cache.root())
            .with_context(|| format!("Failed to clear cache at: {}", cache.root().display()))?;
    } else {
        for package in packages {
            let count = cache.purge(package)?;
            match count {
                0 => writeln!(
                    printer,
                    "No entries found for package: {}",
                    format!("{package}").cyan()
                )?,
                1 => writeln!(
                    printer,
                    "Cleared 1 entry for package: {}",
                    format!("{package}").cyan()
                )?,
                count => writeln!(
                    printer,
                    "Cleared {count} entries for package: {}",
                    format!("{package}").cyan()
                )?,
            }
        }
    }

    Ok(ExitStatus::Success)
}
