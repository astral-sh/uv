use std::fmt::Write;

use anyhow::Context;
use owo_colors::OwoColorize;
use serde::Serialize;

use uv_cli::PythonDirFormat;
use uv_fs::{PortablePath, Simplified};
use uv_preview::{Preview, PreviewFeature};
use uv_python::managed::{ManagedPythonInstallations, python_executable_dir};
use uv_warnings::warn_user;

use crate::printer::Printer;

#[derive(Serialize)]
struct PythonDirData<'a> {
    path: PortablePath<'a>,
}

/// Show the Python installation directory.
pub(crate) fn dir(
    bin: bool,
    output_format: PythonDirFormat,
    preview: Preview,
    printer: Printer,
) -> anyhow::Result<()> {
    if matches!(output_format, PythonDirFormat::Json)
        && !preview.is_enabled(PreviewFeature::JsonOutput)
    {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    let path = if bin {
        python_executable_dir()?
    } else {
        let installed_toolchains = ManagedPythonInstallations::from_settings(None)
            .context("Failed to initialize toolchain settings")?;
        installed_toolchains.root().to_path_buf()
    };

    match output_format {
        PythonDirFormat::Text => {
            writeln!(printer.stdout(), "{}", path.simplified_display().cyan())?;
        }
        PythonDirFormat::Json => {
            let data = PythonDirData {
                path: path.simplified().into(),
            };
            writeln!(printer.stdout(), "{}", serde_json::to_string(&data)?)?;
        }
    }

    Ok(())
}
