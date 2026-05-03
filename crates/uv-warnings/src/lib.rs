use std::error::Error;
use std::iter;
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

// macro hygiene: The user might not have direct dependencies on those crates
#[doc(hidden)]
pub use anstream;
#[doc(hidden)]
pub use owo_colors;
use owo_colors::{DynColor, OwoColorize};
use rustc_hash::FxHashSet;

/// Whether user-facing warnings are enabled.
pub static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable user-facing warnings.
pub fn enable() {
    ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Disable user-facing warnings.
pub fn disable() {
    ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
}

/// A callback function for printing warnings.
type PrinterCallback = Box<dyn Fn(&str) + Send + Sync>;

/// A global printer callback that, when set, is used to print warnings instead of writing
/// directly to stderr. This allows coordinating warning output with indicatif's progress bar
/// system via `MultiProgress::suspend()`.
static PRINTER: Mutex<Option<PrinterCallback>> = Mutex::new(None);

/// Set a global printer callback for warning output.
///
/// When set, all warnings will be routed through this callback instead of writing directly
/// to stderr. This is used to coordinate with indicatif progress bars.
///
/// Note: only one printer callback can be active at a time. If multiple reporters call
/// `set_printer` concurrently, the last one wins and `clear_printer` from an earlier
/// reporter will remove the later reporter's callback. Callers should ensure only one
/// reporter is active at a time.
pub fn set_printer(callback: Box<dyn Fn(&str) + Send + Sync>) {
    if let Ok(mut printer) = PRINTER.lock() {
        *printer = Some(callback);
    }
}

/// Clear the global printer callback, restoring direct stderr output for warnings.
pub fn clear_printer() {
    if let Ok(mut printer) = PRINTER.lock() {
        *printer = None;
    }
}

/// Print a warning line, routing through the global printer callback if set,
/// or falling back to `anstream::eprintln!`.
///
/// Uses `try_lock()` instead of `lock()` to avoid deadlocking if the callback
/// (or anything it transitively calls) triggers another warning on the same thread.
#[doc(hidden)]
pub fn print_warning(line: &str) {
    if let Ok(printer) = PRINTER.try_lock() {
        if let Some(callback) = printer.as_ref() {
            callback(line);
            return;
        }
    }
    anstream::eprintln!("{line}");
}

/// Warn a user, if warnings are enabled.
#[macro_export]
macro_rules! warn_user {
    ($($arg:tt)*) => {{
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            let message = format!("{}", format_args!($($arg)*));
            let formatted = message.bold();
            let line = format!("{}{} {formatted}", "warning".yellow().bold(), ":".bold());
            $crate::print_warning(&line);
        }
    }};
}

pub static WARNINGS: LazyLock<Mutex<FxHashSet<String>>> = LazyLock::new(Mutex::default);

/// Warn a user once, if warnings are enabled, with uniqueness determined by the content of the
/// message.
#[macro_export]
macro_rules! warn_user_once {
    ($($arg:tt)*) => {{
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut states) = $crate::WARNINGS.lock() {
                let message = format!("{}", format_args!($($arg)*));
                if states.insert(message.clone()) {
                    let line = format!("{}{} {}", "warning".yellow().bold(), ":".bold(), message.bold());
                    $crate::print_warning(&line);
                }
            }
        }
    }};
}

/// Format an error or warning chain.
///
/// # Example
///
/// ```text
/// error: Failed to install app
///   Caused By: Failed to install dependency
///   Caused By: Error writing failed `/home/ferris/deps/foo`: Permission denied
/// ```
///
/// ```text
/// warning: Failed to create registry entry for Python 3.12
///   Caused By: Security policy forbids chaining registry entries
/// ```
///
/// ```text
/// error: Failed to download Python 3.12
///  Caused by: Failed to fetch https://example.com/upload/python3.13.tar.zst
///             Server says: This endpoint only support POST requests.
///
///             For downloads, please refer to https://example.com/download/python3.13.tar.zst
///  Caused by: Caused By: HTTP Error 400
/// ```
pub fn write_error_chain(
    err: &dyn Error,
    mut stream: impl std::fmt::Write,
    level: impl AsRef<str>,
    color: impl DynColor + Copy,
) -> std::fmt::Result {
    writeln!(
        &mut stream,
        "{}{} {}",
        level.as_ref().color(color).bold(),
        ":".bold(),
        err.to_string().trim().bold()
    )?;
    for source in iter::successors(err.source(), |&err| err.source()) {
        let msg = source.to_string();
        let mut lines = msg.lines();
        if let Some(first) = lines.next() {
            let padding = "  ";
            let cause = "Caused by";
            let child_padding = " ".repeat(padding.len() + cause.len() + 2);
            writeln!(
                &mut stream,
                "{}{}: {}",
                padding,
                cause.color(color).bold(),
                first.trim()
            )?;
            for line in lines {
                let line = line.trim_end();
                if line.is_empty() {
                    // Avoid showing indents on empty lines
                    writeln!(&mut stream)?;
                } else {
                    writeln!(&mut stream, "{}{}", child_padding, line.trim_end())?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::write_error_chain;
    use crate::{clear_printer, print_warning, set_printer};
    use anyhow::anyhow;
    use indoc::indoc;
    use insta::assert_snapshot;
    use owo_colors::AnsiColors;

    #[test]
    fn set_printer_routes_warnings_through_callback() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::default();
        let captured_clone = captured.clone();
        set_printer(Box::new(move |line| {
            captured_clone.lock().unwrap().push(line.to_string());
        }));

        print_warning("test warning message");

        let messages = captured.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "test warning message");

        clear_printer();
    }

    #[test]
    fn clear_printer_restores_direct_stderr() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::default();
        let captured_clone = captured.clone();
        set_printer(Box::new(move |line| {
            captured_clone.lock().unwrap().push(line.to_string());
        }));

        clear_printer();
        // After clearing, this should go to stderr, not the callback.
        print_warning("after clear");

        let messages = captured.lock().unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn print_warning_falls_through_when_no_printer_set() {
        // Ensure no printer is set.
        clear_printer();
        // Should write to stderr without panicking.
        print_warning("fallthrough warning");
    }

    #[test]
    fn format_multiline_message() {
        let err_middle = indoc! {"Failed to fetch https://example.com/upload/python3.13.tar.zst
        Server says: This endpoint only support POST requests.

        For downloads, please refer to https://example.com/download/python3.13.tar.zst"};
        let err = anyhow!("Caused By: HTTP Error 400")
            .context(err_middle)
            .context("Failed to download Python 3.12");

        let mut rendered = String::new();
        write_error_chain(err.as_ref(), &mut rendered, "error", AnsiColors::Red).unwrap();
        let rendered = anstream::adapter::strip_str(&rendered);

        assert_snapshot!(rendered, @"
        error: Failed to download Python 3.12
          Caused by: Failed to fetch https://example.com/upload/python3.13.tar.zst
                     Server says: This endpoint only support POST requests.

                     For downloads, please refer to https://example.com/download/python3.13.tar.zst
          Caused by: Caused By: HTTP Error 400
        ");
    }
}
