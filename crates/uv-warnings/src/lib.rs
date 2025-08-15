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
        err.to_string().trim()
    )?;
    for source in iter::successors(err.source(), |&err| err.source()) {
        writeln!(
            &mut stream,
            "  {}: {}",
            "Caused by".color(color).bold(),
            source.to_string().trim()
        )?;
    }
    Ok(())
}
