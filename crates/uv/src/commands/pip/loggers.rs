use std::fmt;
use std::fmt::Write;

use itertools::Itertools;
use owo_colors::OwoColorize;

use distribution_types::{CachedDist, InstalledDist, InstalledMetadata, LocalDist, Name};

use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
use crate::printer::Printer;

/// A trait to handle logging during install operations.
pub(crate) trait InstallLogger {
    /// Log the completion of the audit phase.
    fn on_audit(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result;

    /// Log the completion of the preparation phase.
    fn on_prepare(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result;

    /// Log the completion of the uninstallation phase.
    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
    ) -> fmt::Result;

    /// Log the completion of the installation phase.
    fn on_install(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result;

    /// Log the completion of the operation.
    fn on_complete(
        &self,
        installed: Vec<CachedDist>,
        reinstalled: Vec<InstalledDist>,
        uninstalled: Vec<InstalledDist>,
        printer: Printer,
    ) -> fmt::Result;
}

/// The default logger for install operations.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultInstallLogger;

impl InstallLogger for DefaultInstallLogger {
    fn on_audit(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Audited {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_prepare(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Prepared {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
    ) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Uninstalled {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_install(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Installed {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_complete(
        &self,
        installed: Vec<CachedDist>,
        reinstalled: Vec<InstalledDist>,
        uninstalled: Vec<InstalledDist>,
        printer: Printer,
    ) -> fmt::Result {
        for event in uninstalled
            .into_iter()
            .chain(reinstalled)
            .map(|distribution| ChangeEvent {
                dist: LocalDist::from(distribution),
                kind: ChangeEventKind::Removed,
            })
            .chain(installed.into_iter().map(|distribution| ChangeEvent {
                dist: LocalDist::from(distribution),
                kind: ChangeEventKind::Added,
            }))
            .sorted_unstable_by(|a, b| {
                a.dist
                    .name()
                    .cmp(b.dist.name())
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.dist.installed_version().cmp(&b.dist.installed_version()))
            })
        {
            match event.kind {
                ChangeEventKind::Added => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "+".green(),
                        event.dist.name().bold(),
                        event.dist.installed_version().dimmed()
                    )?;
                }
                ChangeEventKind::Removed => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "-".red(),
                        event.dist.name().bold(),
                        event.dist.installed_version().dimmed()
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// A logger that only shows installs and uninstalls, the minimal logging necessary to understand
/// environment changes.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SummaryInstallLogger;

impl InstallLogger for SummaryInstallLogger {
    fn on_audit(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        Ok(())
    }

    fn on_prepare(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        Ok(())
    }

    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
    ) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Uninstalled {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_install(&self, count: usize, start: std::time::Instant, printer: Printer) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Installed {} {}",
                format!("{} package{}", count, s).bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_complete(
        &self,
        installed: Vec<CachedDist>,
        reinstalled: Vec<InstalledDist>,
        uninstalled: Vec<InstalledDist>,
        printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }
}
