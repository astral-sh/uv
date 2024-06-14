use anyhow::Result;
use std::fmt::Write;
use std::str::FromStr;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::{ImplementationName, SystemPython, Toolchain, ToolchainRequest, VersionRequest};
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Find a toolchain.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn find(
    version: Option<String>,
    implementation: Option<String>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv toolchain find` is experimental and may change without warning.");
    }

    let implementation = implementation
        .as_deref()
        .map(ImplementationName::from_str)
        .transpose()?;
    let version = version
        .as_deref()
        .map(VersionRequest::from_str)
        .transpose()?;

    let request = match (version, implementation) {
        (None, None) => ToolchainRequest::Any,
        (Some(version), None) => ToolchainRequest::Version(version),
        (Some(version), Some(implementation)) => {
            ToolchainRequest::ImplementationVersion(implementation, version)
        }
        (None, Some(implementation)) => ToolchainRequest::Implementation(implementation),
    };

    let toolchain = Toolchain::find_requested(
        &request,
        SystemPython::Required,
        PreviewMode::Enabled,
        cache,
    )?;

    writeln!(
        printer.stdout(),
        "{}",
        toolchain.interpreter().sys_executable().user_display()
    )?;

    Ok(ExitStatus::Success)
}
