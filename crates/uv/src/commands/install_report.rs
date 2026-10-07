use std::fmt::Write;

use serde::Serialize;

use uv_cli::PipInstallFormat;
use uv_configuration::DryRun;
use uv_distribution_types::Name;
use uv_normalize::PackageName;

use crate::commands::pip::operations::{ChangedDist, Changelog};
use crate::printer::Printer;

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

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    /// An unstable, experimental schema.
    #[default]
    Preview,
}

#[derive(Serialize, Debug, Default)]
pub(crate) struct SchemaReport {
    /// The version of the schema.
    version: SchemaVersion,
}

/// A summary of all package changes made or planned during an installation.
#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct PackageChangesReport(Vec<PackageChangeReport>);

impl PackageChangesReport {
    pub(crate) fn from_changelog(changelog: &Changelog) -> Self {
        let mut changes: Vec<_> =
            changelog
                .uninstalled
                .iter()
                .map(|dist| PackageChangeReport::from_dist(dist, PackageChangeAction::Uninstalled))
                .chain(changelog.installed.iter().map(|dist| {
                    PackageChangeReport::from_dist(dist, PackageChangeAction::Installed)
                }))
                .chain(changelog.reinstalled.iter().map(|dist| {
                    PackageChangeReport::from_dist(dist, PackageChangeAction::Reinstalled)
                }))
                .collect();

        changes.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.action.cmp(&b.action))
                .then_with(|| a.version.cmp(&b.version))
        });
        Self(changes)
    }
}

/// A summary of a single package change made or planned during an installation.
#[derive(Serialize, Debug, Clone)]
struct PackageChangeReport {
    /// The normalized package name.
    name: PackageName,
    /// The resolved version of the package.
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<uv_pep440::Version>,
    /// The action that was taken for the package.
    action: PackageChangeAction,
}

impl PackageChangeReport {
    fn from_dist(dist: &ChangedDist, action: PackageChangeAction) -> Self {
        Self {
            name: dist.name().clone(),
            version: dist.version().cloned(),
            action,
        }
    }
}

/// The action taken or planned for an individual package.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum PackageChangeAction {
    Uninstalled,
    Installed,
    Reinstalled,
}
