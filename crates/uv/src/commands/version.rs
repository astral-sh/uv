use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Serialize;

use uv_cache::CacheBucket;
use uv_cli::version::{SelfVersionInfo, uv_self_version};
use uv_command_support::{ExitStatus, Printer};
use uv_configuration::VersionFormat;

#[derive(Serialize)]
struct SelfVersionOutput {
    #[serde(flatten)]
    version_info: SelfVersionInfo,
    cache_bucket_versions: Vec<&'static str>,
}

/// Display version information for uv itself (`uv self version`)
pub(crate) fn self_version(
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    let version_info = uv_self_version();
    match output_format {
        VersionFormat::Text => {
            if short {
                writeln!(printer.stdout(), "{}", version_info.version().cyan())?;
            } else {
                writeln!(printer.stdout(), "uv {}", version_info.cyan())?;
            }
        }
        VersionFormat::Json => {
            let mut cache_bucket_versions: Vec<_> =
                CacheBucket::iter().map(CacheBucket::to_str).collect();
            cache_bucket_versions.sort_unstable();
            let string = serde_json::to_string_pretty(&SelfVersionOutput {
                version_info,
                cache_bucket_versions,
            })?;
            writeln!(printer.stdout_important(), "{string}")?;
        }
    }

    Ok(ExitStatus::Success)
}
