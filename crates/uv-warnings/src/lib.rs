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
pub fn write_warning_chain(err: &dyn Error, hints: Hints<'_>) -> fmt::Result {
    write_warning_chain_with_options(err, hints, ErrorOptions::default())
}

fn write_warning_chain_with_options<C, W: fmt::Write>(
    err: &dyn Error,
    hints: Hints<'_>,
    options: ErrorOptions<'_, C, W>,
) -> fmt::Result {
    write_error_chain_with_options(
        err,
        hints,
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
            Hints::none(),
            ErrorOptions::default().with_stream(&mut output),
        )
        .unwrap();
        assert_snapshot!(format!("{output:?}"), @r#""\u{1b}[1m\u{1b}[33mwarning\u{1b}[39m\u{1b}[0m\u{1b}[1m:\u{1b}[0m Failed to create registry entry\n""#);
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @"warning: Failed to create registry entry
");
    }
}
