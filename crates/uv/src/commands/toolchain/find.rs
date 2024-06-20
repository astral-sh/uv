use anyhow::Result;
use std::fmt::Write;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::{EnvironmentPreference, Toolchain, ToolchainPreference, ToolchainRequest};
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Find a toolchain.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn find(
    request: Option<String>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv toolchain find` is experimental and may change without warning.");
    }

    let request = match request {
        Some(request) => ToolchainRequest::parse(&request),
        None => ToolchainRequest::Any,
    };
    let toolchain = Toolchain::find(
        &request,
        EnvironmentPreference::OnlySystem,
        ToolchainPreference::from_settings(PreviewMode::Enabled),
        cache,
    )?;

    writeln!(
        printer.stdout(),
        "{}",
        toolchain.interpreter().sys_executable().user_display()
    )?;

    Ok(ExitStatus::Success)
}
