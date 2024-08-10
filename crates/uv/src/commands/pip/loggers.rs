use std::collections::BTreeSet;
use std::fmt;
use std::fmt::Write;

use crate::commands::{elapsed, ChangeEvent, ChangeEventKind};
use crate::printer::Printer;
use distribution_types::{CachedDist, InstalledDist, InstalledMetadata, LocalDist, Name};
use itertools::Itertools;
use owo_colors::OwoColorize;
use pep440_rs::Version;
use rustc_hash::{FxBuildHasher, FxHashMap};
use uv_normalize::PackageName;

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
                format!("{count} package{s}").bold(),
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
                format!("{count} package{s}").bold(),
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
                format!("{count} package{s}").bold(),
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
                format!("{count} package{s}").bold(),
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
    fn on_audit(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_prepare(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
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
                format!("{count} package{s}").bold(),
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
                format!("{count} package{s}").bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }

    fn on_complete(
        &self,
        _installed: Vec<CachedDist>,
        _reinstalled: Vec<InstalledDist>,
        _uninstalled: Vec<InstalledDist>,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }
}

/// A logger that only shows installs and uninstalls, the minimal logging necessary to understand
/// environment changes.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct UpgradeInstallLogger;

impl InstallLogger for UpgradeInstallLogger {
    fn on_audit(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_prepare(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_uninstall(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_install(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_complete(
        &self,
        installed: Vec<CachedDist>,
        reinstalled: Vec<InstalledDist>,
        uninstalled: Vec<InstalledDist>,
        printer: Printer,
    ) -> fmt::Result {
        // Index the removals by package name.
        let removals: FxHashMap<&PackageName, BTreeSet<Version>> =
            reinstalled.iter().chain(uninstalled.iter()).fold(
                FxHashMap::with_capacity_and_hasher(
                    reinstalled.len() + uninstalled.len(),
                    FxBuildHasher,
                ),
                |mut acc, distribution| {
                    acc.entry(distribution.name())
                        .or_default()
                        .insert(distribution.installed_version().version().clone());
                    acc
                },
            );

        // Index the additions by package name.
        let additions: FxHashMap<&PackageName, BTreeSet<Version>> = installed.iter().fold(
            FxHashMap::with_capacity_and_hasher(installed.len(), FxBuildHasher),
            |mut acc, distribution| {
                acc.entry(distribution.name())
                    .or_default()
                    .insert(distribution.installed_version().version().clone());
                acc
            },
        );

        // Summarize the changes.
        for name in removals
            .keys()
            .chain(additions.keys())
            .collect::<BTreeSet<_>>()
        {
            match (removals.get(name), additions.get(name)) {
                (Some(removals), Some(additions)) => {
                    if removals == additions {
                        let reinstalls = additions
                            .iter()
                            .map(|version| format!("v{version}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        writeln!(
                            printer.stderr(),
                            "{} {} {}",
                            "Reinstalled".yellow().bold(),
                            name,
                            reinstalls
                        )?;
                    } else {
                        let removals = removals
                            .iter()
                            .map(|version| format!("v{version}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let additions = additions
                            .iter()
                            .map(|version| format!("v{version}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        writeln!(
                            printer.stderr(),
                            "{} {} {} -> {}",
                            "Updated".green().bold(),
                            name,
                            removals,
                            additions
                        )?;
                    }
                }
                (Some(removals), None) => {
                    let removals = removals
                        .iter()
                        .map(|version| format!("v{version}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        printer.stderr(),
                        "{} {} {}",
                        "Removed".red().bold(),
                        name,
                        removals
                    )?;
                }
                (None, Some(additions)) => {
                    let additions = additions
                        .iter()
                        .map(|version| format!("v{version}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        printer.stderr(),
                        "{} {} {}",
                        "Added".green().bold(),
                        name,
                        additions
                    )?;
                }
                (None, None) => {
                    unreachable!("The key `{name}` should exist in at least one of the maps");
                }
            }
        }

        Ok(())
    }
}

/// A trait to handle logging during resolve operations.
pub(crate) trait ResolveLogger {
    /// Log the completion of the operation.
    fn on_complete(&self, count: usize, start: std::time::Instant, printer: Printer)
        -> fmt::Result;
}

/// The default logger for resolve operations.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultResolveLogger;

impl ResolveLogger for DefaultResolveLogger {
    fn on_complete(
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
                "Resolved {} {}",
                format!("{count} package{s}").bold(),
                format!("in {}", elapsed(start.elapsed())).dimmed()
            )
            .dimmed()
        )
    }
}

/// A logger that doesn't show any output.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SummaryResolveLogger;

impl ResolveLogger for SummaryResolveLogger {
    fn on_complete(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
    ) -> fmt::Result {
        Ok(())
    }
}
