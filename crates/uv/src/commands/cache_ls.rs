use std::collections::HashSet;
use std::fmt::Write;
use std::io;

use anyhow::Result;
use serde::Serialize;
use tracing::warn;

use uv_cache::{Cache, CacheBucket};
use uv_cli::OutputFormat;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::ResolutionMetadata;

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

#[derive(Debug, Serialize)]
struct CachedPackage {
    name: PackageName,
    version: Version,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CacheLsOutput {
    packages: Vec<CachedPackage>,
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

/// List the contents of the cache.
pub(crate) fn cache_ls(
    cache: &Cache,
    size: bool,
    quiet: bool,
    format: &OutputFormat,
    package: Option<&str>,
    printer: Printer,
) -> Result<ExitStatus> {
    let archive_dir = cache.bucket(CacheBucket::Archive);

    if !archive_dir.exists() {
        return print_empty(quiet, format, printer);
    }

    let mut packages = Vec::new();
    let mut total_size: u64 = 0;
    let mut sized_dirs = HashSet::new();

    // Single pass: walk archive-v0/ looking for `**/METADATA`
    // Structure: archive-v0/{hash}/{pkg-version}.dist-info/METADATA
    for entry in walkdir::WalkDir::new(&archive_dir).max_depth(3) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to walk cache: {err}");
                continue;
            }
        };

        if entry.file_name() != "METADATA" || !entry.file_type().is_file() {
            continue;
        }

        let metadata_path = entry.path();
        let Some(dist_info_dir) = metadata_path.parent() else {
            continue;
        };
        let Some(hash_dir) = dist_info_dir.parent() else {
            continue;
        };
        let hash_dir = hash_dir.to_path_buf();

        let content = match fs_err::read(metadata_path) {
            Ok(c) => c,
            Err(err) => {
                warn!("Failed to read `{}`: {err}", metadata_path.user_display());
                continue;
            }
        };

        let resolution = match ResolutionMetadata::parse_metadata(&content) {
            Ok(m) => m,
            Err(err) => {
                warn!("Failed to parse `{}`: {err}", metadata_path.user_display());
                continue;
            }
        };

        if let Some(pattern) = package {
            if !resolution.name.as_ref().contains(pattern) {
                continue;
            }
        }

        let pkg_size = if size && sized_dirs.insert(hash_dir.clone()) {
            match compute_dir_size(&hash_dir) {
                Ok(s) => {
                    total_size += s;
                    Some(s)
                }
                Err(err) => {
                    warn!(
                        "Failed to compute size for `{}`: {err}",
                        hash_dir.user_display()
                    );
                    None
                }
            }
        } else {
            None
        };

        packages.push(CachedPackage {
            name: resolution.name,
            version: resolution.version,
            size: pkg_size,
        });
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| b.version.cmp(&a.version)));

    let total = packages.len();

    if quiet {
        writeln!(printer.stdout(), "{total}")?;
        return Ok(ExitStatus::Success);
    }

    match format {
        OutputFormat::Json => {
            let output = CacheLsOutput {
                packages,
                total,
                size_bytes: if size { Some(total_size) } else { None },
            };
            writeln!(printer.stdout(), "{}", serde_json::to_string(&output)?)?;
        }
        OutputFormat::Text => {
            if size {
                writeln!(printer.stdout(), "{:<24} {:<12} Size", "Package", "Version")?;
            } else {
                writeln!(printer.stdout(), "{:<24} Version", "Package")?;
            }

            for pkg in &packages {
                if let Some(s) = pkg.size {
                    let (bytes, unit) = human_readable_bytes(s);
                    writeln!(
                        printer.stdout(),
                        "{:<24} {version:<12} {bytes:.1}{unit}",
                        pkg.name.as_str(),
                        version = pkg.version,
                    )?;
                } else {
                    writeln!(
                        printer.stdout(),
                        "{:<24} {version}",
                        pkg.name.as_str(),
                        version = pkg.version,
                    )?;
                }
            }

            let summary = if size {
                let (bytes, unit) = human_readable_bytes(total_size);
                format!("\n{total} packages cached ({bytes:.1}{unit})")
            } else {
                format!("\n{total} packages cached")
            };
            writeln!(printer.stdout(), "{summary}")?;
        }
    }

    Ok(ExitStatus::Success)
}

fn print_empty(quiet: bool, format: &OutputFormat, printer: Printer) -> Result<ExitStatus> {
    if quiet {
        writeln!(printer.stdout(), "0")?;
    } else {
        match format {
            OutputFormat::Json => {
                let output = CacheLsOutput {
                    packages: Vec::new(),
                    total: 0,
                    size_bytes: None,
                };
                writeln!(printer.stdout(), "{}", serde_json::to_string(&output)?)?;
            }
            OutputFormat::Text => {
                writeln!(printer.stdout(), "0 packages cached")?;
            }
        }
    }
    Ok(ExitStatus::Success)
}

fn compute_dir_size(path: &std::path::Path) -> io::Result<u64> {
    let mut size = 0u64;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            size += entry.metadata()?.len();
        }
    }
    Ok(size)
}
