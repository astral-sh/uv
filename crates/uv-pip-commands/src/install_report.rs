use std::fmt::Write;

use serde::Serialize;
use uv_command_support::Printer;
use uv_configuration::DryRun;
use uv_configuration::PipInstallFormat;
use uv_install_operations::Changelog;
use uv_install_operations::report::{PackageChangesReport, SchemaReport};

/// Write the package changes as JSON when requested.
pub(crate) fn write_install_report(
    changelog: &Changelog,
    dry_run: DryRun,
    output_format: PipInstallFormat,
    printer: Printer,
) -> anyhow::Result<()> {
    match output_format {
        PipInstallFormat::Text => {}
        PipInstallFormat::Json => {
            let report = InstallReport {
                schema: SchemaReport::default(),
                changes: PackageChangesReport::from_changelog(changelog),
                dry_run: dry_run.enabled(),
            };
            writeln!(
                printer.stdout_important(),
                "{}",
                serde_json::to_string_pretty(&report)?
            )?;
        }
    }
    Ok(())
}

/// A report of the changes made or planned by `uv pip install` or `uv pip sync`.
#[derive(Debug, Serialize)]
struct InstallReport {
    schema: SchemaReport,
    changes: PackageChangesReport,
    dry_run: bool,
}
