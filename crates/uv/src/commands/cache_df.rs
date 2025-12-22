use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use diskus::DiskUsage;

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;
use uv_cache::Cache;
use uv_cache::CacheBucket;

fn format_human_readable(bytes: u64) -> (String, &'static str) {
    if bytes == 0 {
        ("0".to_string(), "B")
    } else {
        let (val, unit) = human_readable_bytes(bytes);
        (format!("{val:.1}"), unit)
    }
}

pub(crate) fn cache_df(cache: &Cache, printer: Printer) -> Result<ExitStatus> {
    // Define all cache buckets with their descriptions
    let buckets = [
        (
            CacheBucket::Wheels,
            "Wheels",
            "Downloaded and cached wheels from registries and direct URLs",
        ),
        (
            CacheBucket::SourceDistributions,
            "Source Distributions",
            "Source distributions and built wheels",
        ),
        (
            CacheBucket::Simple,
            "Simple Metadata",
            "Package metadata from simple repositories",
        ),
        (
            CacheBucket::Git,
            "Git Repositories",
            "Cloned git repositories",
        ),
        (
            CacheBucket::Interpreter,
            "Interpreter Info",
            "Cached Python interpreter information",
        ),
        (CacheBucket::FlatIndex, "Flat Index", "Flat index responses"),
        (
            CacheBucket::Archive,
            "Archive",
            "Shared archive storage for directories",
        ),
        (
            CacheBucket::Builds,
            "Build Environments",
            "Ephemeral environments for builds",
        ),
        (
            CacheBucket::Environments,
            "Environments",
            "Reusable tool environments",
        ),
        (CacheBucket::Python, "Python", "Cached Python downloads"),
        (
            CacheBucket::Binaries,
            "Binaries",
            "Downloaded tool binaries",
        ),
    ];

    // Calculate total size (will be 0 if cache directory doesn't exist)
    let total_bytes = if cache.root().exists() {
        let disk_usage = DiskUsage::new(vec![cache.root().to_path_buf()]);
        disk_usage.count_ignoring_errors()
    } else {
        0
    };

    writeln!(printer.stdout_important(), "CACHE UTILIZATION")?;
    writeln!(printer.stdout(), "{}", "=".repeat(80))?;

    // Display table header
    writeln!(
        printer.stdout(),
        "{:<25} {:>12} {:>12} {:<30}",
        "Cache Type",
        "Count",
        "Size",
        "Description"
    )?;
    writeln!(printer.stdout(), "{}", "-".repeat(80))?;

    let mut total_count = 0u64;

    for (bucket, name, description) in buckets {
        let bucket_path = cache.root().join(bucket.to_str());
        let (size, count) = if bucket_path.exists() {
            let disk_usage = DiskUsage::new(vec![bucket_path.clone()]);
            let size = disk_usage.count_ignoring_errors();
            let count = count_files_in_directory(&bucket_path);
            (size, count)
        } else {
            (0, 0)
        };

        total_count += count;

        let (size_val, size_unit) = format_human_readable(size);

        writeln!(
            printer.stdout(),
            "{name:<25} {count:>12} {size_val:>8}{size_unit:<4} {description:<30}"
        )?;
    }

    writeln!(printer.stdout(), "{}", "=".repeat(80))?;

    // Display total
    let (total_size_val, total_size_unit) = format_human_readable(total_bytes);

    writeln!(
        printer.stdout(),
        "{:<25} {:>12} {:>8}{:<4}",
        "TOTAL",
        total_count,
        total_size_val,
        total_size_unit
    )?;

    writeln!(printer.stdout())?;
    writeln!(
        printer.stdout(),
        "Cache directory: {}",
        cache.root().display()
    )?;

    Ok(ExitStatus::Success)
}

fn count_files_in_directory(dir: &Path) -> u64 {
    if !dir.exists() {
        return 0;
    }

    let mut count = 0u64;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = fs_err::read_dir(&current) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            count += 1;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
    }

    count
}
