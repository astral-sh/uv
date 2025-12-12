use std::collections::BTreeSet;
use std::fmt;
use std::fmt::Write;

use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_configuration::DryRun;
use uv_distribution_types::Name;
use uv_normalize::PackageName;

use crate::commands::pip::operations::{Changelog, ShortSpecifier};
use crate::commands::{ChangeEvent, ChangeEventKind, elapsed};
use crate::printer::Printer;

/// A trait to handle logging during install operations.
pub(crate) trait InstallLogger {
    /// Log the completion of the audit phase.
    fn on_audit(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result;

    /// Log the completion of the preparation phase.
    fn on_prepare(
        &self,
        count: usize,
        suffix: Option<&str>,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result;

    /// Log the completion of the uninstallation phase.
    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result;

    /// Log the completion of the installation phase.
    fn on_install(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result;

    /// Log the completion of the operation.
    fn on_complete(&self, changelog: &Changelog, printer: Printer, dry_run: DryRun) -> fmt::Result;
}

/// The default logger for install operations.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DefaultInstallLogger;

impl InstallLogger for DefaultInstallLogger {
    fn on_audit(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        if count == 0 {
            writeln!(
                printer.stderr(),
                "{}",
                format!("Audited in {}", elapsed(start.elapsed())).dimmed()
            )?;
        } else {
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
            )?;
        }
        if dry_run.enabled() {
            writeln!(printer.stderr(), "Would make no changes")?;
        }
        Ok(())
    }

    fn on_prepare(
        &self,
        count: usize,
        suffix: Option<&str>,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        let what = if let Some(suffix) = suffix {
            format!("{count} package{s} {suffix}")
        } else {
            format!("{count} package{s}")
        };
        let what = what.bold();
        writeln!(
            printer.stderr(),
            "{}",
            if dry_run.enabled() {
                format!("Would download {what}")
            } else {
                format!(
                    "Prepared {what} {}",
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
            }
            .dimmed()
        )
    }

    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        let what = format!("{count} package{s}");
        let what = what.bold();
        writeln!(
            printer.stderr(),
            "{}",
            if dry_run.enabled() {
                format!("Would uninstall {what}")
            } else {
                format!(
                    "Uninstalled {what} {}",
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
            }
            .dimmed()
        )
    }

    fn on_install(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        let s = if count == 1 { "" } else { "s" };
        let what = format!("{count} package{s}");
        let what = what.bold();
        writeln!(
            printer.stderr(),
            "{}",
            if dry_run.enabled() {
                format!("Would install {what}")
            } else {
                format!(
                    "Installed {what} {}",
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
            }
            .dimmed()
        )
    }

    fn on_complete(
        &self,
        changelog: &Changelog,
        printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        for event in changelog
            .uninstalled
            .iter()
            .map(|distribution| ChangeEvent {
                dist: distribution,
                kind: ChangeEventKind::Removed,
            })
            .chain(changelog.installed.iter().map(|distribution| ChangeEvent {
                dist: distribution,
                kind: ChangeEventKind::Added,
            }))
            .chain(
                changelog
                    .reinstalled
                    .iter()
                    .map(|distribution| ChangeEvent {
                        dist: distribution,
                        kind: ChangeEventKind::Reinstalled,
                    }),
            )
            .sorted_unstable_by(|a, b| {
                a.dist
                    .name()
                    .cmp(b.dist.name())
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.dist.long_specifier().cmp(&b.dist.long_specifier()))
            })
        {
            match event.kind {
                ChangeEventKind::Added => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "+".green(),
                        event.dist.name().bold(),
                        event.dist.long_specifier().dimmed()
                    )?;
                }
                ChangeEventKind::Removed => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "-".red(),
                        event.dist.name().bold(),
                        event.dist.long_specifier().dimmed()
                    )?;
                }
                ChangeEventKind::Reinstalled => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "~".yellow(),
                        event.dist.name().bold(),
                        event.dist.long_specifier().dimmed()
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
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_prepare(
        &self,
        _count: usize,
        _suffix: Option<&str>,
        _start: std::time::Instant,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_uninstall(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        DefaultInstallLogger.on_uninstall(count, start, printer, dry_run)
    }

    fn on_install(
        &self,
        count: usize,
        start: std::time::Instant,
        printer: Printer,
        dry_run: DryRun,
    ) -> fmt::Result {
        DefaultInstallLogger.on_install(count, start, printer, dry_run)
    }

    fn on_complete(
        &self,
        _changelog: &Changelog,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }
}

/// A logger that shows special output for the modification of the given target.
#[derive(Debug, Clone)]
pub(crate) struct UpgradeInstallLogger {
    target: PackageName,
}

impl UpgradeInstallLogger {
    /// Create a new logger for the given target.
    pub(crate) fn new(target: PackageName) -> Self {
        Self { target }
    }
}

impl InstallLogger for UpgradeInstallLogger {
    fn on_audit(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_prepare(
        &self,
        _count: usize,
        _suffix: Option<&str>,
        _start: std::time::Instant,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_uninstall(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_install(
        &self,
        _count: usize,
        _start: std::time::Instant,
        _printer: Printer,
        _dry_run: DryRun,
    ) -> fmt::Result {
        Ok(())
    }

    fn on_complete(
        &self,
        changelog: &Changelog,
        printer: Printer,
        // TODO(tk): Adjust format for dry_run
        _dry_run: DryRun,
    ) -> fmt::Result {
        // Index the removals by package name.
        let removals: FxHashMap<&PackageName, BTreeSet<ShortSpecifier>> =
            changelog.uninstalled.iter().fold(
                FxHashMap::with_capacity_and_hasher(changelog.uninstalled.len(), FxBuildHasher),
                |mut acc, distribution| {
                    acc.entry(distribution.name())
                        .or_default()
                        .insert(distribution.short_specifier());
                    acc
                },
            );

        // Index the additions by package name.
        let additions: FxHashMap<&PackageName, BTreeSet<ShortSpecifier>> =
            changelog.installed.iter().fold(
                FxHashMap::with_capacity_and_hasher(changelog.installed.len(), FxBuildHasher),
                |mut acc, distribution| {
                    acc.entry(distribution.name())
                        .or_default()
                        .insert(distribution.short_specifier());
                    acc
                },
            );

        // Summarize the change for the target.
        match (removals.get(&self.target), additions.get(&self.target)) {
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
                        &self.target,
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
                        &self.target,
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
                    &self.target,
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
                    &self.target,
                    additions
                )?;
            }
            (None, None) => {
                writeln!(
                    printer.stderr(),
                    "{} {} {}",
                    "Modified".dimmed(),
                    &self.target.dimmed().bold(),
                    "environment".dimmed()
                )?;
            }
        }

        // Follow-up with a detailed summary of all changes.
        DefaultInstallLogger.on_complete(changelog, printer, _dry_run)?;

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
        if count == 0 {
            writeln!(
                printer.stderr(),
                "{}",
                format!("Resolved in {}", elapsed(start.elapsed())).dimmed()
            )
        } else {
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
