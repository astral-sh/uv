use std::error::Error;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

// macro hygiene: The user might not have direct dependencies on those crates
#[doc(hidden)]
pub use anstream;
#[doc(hidden)]
pub use owo_colors;
use rustc_hash::FxHashSet;
use uv_errors::{ErrorOptions, Hints, write_error_chain_with_options};

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

/// Format a warning chain to standard error.
pub fn write_warning_chain(err: &dyn Error) -> fmt::Result {
    write_warning_chain_with_hints(err, Hints::none())
}

/// Format a warning chain with user-facing [`Hints`] to standard error.
pub fn write_warning_chain_with_hints(err: &dyn Error, hints: Hints<'_>) -> fmt::Result {
    write_warning_chain_with_options(err, ErrorOptions::default().with_hints(hints))
}

/// Format a warning with user-facing [`Hints`] to standard error.
pub fn write_warning_with_hints(message: &str, hints: Hints<'_>) -> fmt::Result {
    write_warning_chain_with_hints(&WarningMessage(message), hints)
}

#[derive(Debug)]
struct WarningMessage<'a>(&'a str);

impl fmt::Display for WarningMessage<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for WarningMessage<'_> {}

fn write_warning_chain_with_options<C, W: fmt::Write>(
    err: &dyn Error,
    options: ErrorOptions<'_, C, W>,
) -> fmt::Result {
    write_error_chain_with_options(
        err,
        options
            .with_level("warning")
            .with_color(owo_colors::AnsiColors::Yellow),
    )
}

/// Warn a user, if warnings are enabled.
#[macro_export]
macro_rules! warn_user {
    ($($arg:tt)*) => {{
        use $crate::anstream::eprintln;
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            let message = format!("{}", format_args!($($arg)*));
            let formatted = message.bold();
            eprintln!("{}{} {formatted}", "warning".yellow().bold(), ":".bold());
        }
    }};
}

/// Warn a user with additional hints, if warnings are enabled.
#[macro_export]
macro_rules! warn_user_with_hints {
    ($hints:expr, $($arg:tt)*) => {{
        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            let message = format!("{}", format_args!($($arg)*));
            $crate::write_warning_with_hints(&message, $hints)
                .expect("writing to stderr should not fail");
        }
    }};
}

pub static WARNINGS: LazyLock<Mutex<FxHashSet<String>>> = LazyLock::new(Mutex::default);

/// Warn a user once, if warnings are enabled, with uniqueness determined by the content of the
/// message.
#[macro_export]
macro_rules! warn_user_once {
    ($($arg:tt)*) => {{
        use $crate::anstream::eprintln;
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut states) = $crate::WARNINGS.lock() {
                let message = format!("{}", format_args!($($arg)*));
                if states.insert(message.clone()) {
                    eprintln!("{}{} {}", "warning".yellow().bold(), ":".bold(), message.bold());
                }
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use insta::assert_snapshot;
    use uv_errors::{ErrorOptions, Hints};

    use super::write_warning_chain_with_options;

    #[test]
    fn format_warning_chain() {
        let error = anyhow!("Failed to create registry entry");
        let mut output = String::new();
        write_warning_chain_with_options(
            error.as_ref(),
            ErrorOptions::default().with_stream(&mut output),
        )
        .unwrap();
        assert_snapshot!(format!("{output:?}"), @r#""\u{1b}[1m\u{1b}[33mwarning\u{1b}[39m\u{1b}[0m\u{1b}[1m:\u{1b}[0m Failed to create registry entry\n""#);
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @"warning: Failed to create registry entry
");
    }

    #[test]
    fn format_warning_chain_with_hints() {
        let error =
            anyhow!("The existing virtual environment was not created by uv and will be replaced");
        let mut output = String::new();
        write_warning_chain_with_options(
            error.as_ref(),
            ErrorOptions::default()
                .with_hints(Hints::from(
                    "Use `--no-sync` to avoid replacing the environment",
                ))
                .with_stream(&mut output),
        )
        .expect("writing to the test output should not fail");
        assert_snapshot!(format!("{output:?}"), @r#""\u{1b}[1m\u{1b}[33mwarning\u{1b}[39m\u{1b}[0m\u{1b}[1m:\u{1b}[0m The existing virtual environment was not created by uv and will be replaced\n\n\u{1b}[36m\u{1b}[1mhint\u{1b}[0m\u{1b}[39m\u{1b}[1m:\u{1b}[0m Use `--no-sync` to avoid replacing the environment\n""#);
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @r"
        warning: The existing virtual environment was not created by uv and will be replaced

        hint: Use `--no-sync` to avoid replacing the environment
        ");
    }
}
