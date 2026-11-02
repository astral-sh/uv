use std::fmt::{self, Write};

use owo_colors::OwoColorize;
use uv_command_support::{Printer, elapsed};

/// A trait to handle logging during resolve operations.
pub trait ResolveLogger {
    /// Log the completion of the operation.
    fn on_complete(&self, count: usize, start: std::time::Instant, printer: Printer)
    -> fmt::Result;
}

/// The default logger for resolve operations.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultResolveLogger;

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
pub struct SummaryResolveLogger;

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
