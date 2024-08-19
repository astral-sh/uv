use std::fmt::Write;

use anyhow::Result;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::{EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Find a Python interpreter.
pub(crate) async fn find(
    request: Option<String>,
    python_preference: PythonPreference,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let request = match request {
        Some(request) => PythonRequest::parse(&request),
        None => PythonRequest::Any,
    };
    let python = PythonInstallation::find(
        &request,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    )?;

    writeln!(
        printer.stdout(),
        "{}",
        python.interpreter().sys_executable().user_display()
    )?;

    Ok(ExitStatus::Success)
}
