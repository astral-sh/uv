use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

// macro hygiene: The user might not have direct dependencies on those crates
#[doc(hidden)]
pub use anstream;
#[doc(hidden)]
pub use owo_colors;
use rustc_hash::FxHashSet;
#[doc(hidden)]
pub use uv_static::EnvVars;

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

/// Create a warning for a visibility in a CI provider, if warnings are enabled. Only shows the
/// warning once in a given uv invocation, with uniqueness determined by the content of the message.
#[macro_export]
macro_rules! warn_ci_once {
    ($($arg:tt)*) => {{
        use $crate::anstream::eprintln;
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut states) = $crate::WARNINGS.lock() {
                if let Some(provider) = $crate::CIProvider::from_env() {
                    let message = provider.format(&format!("{}", format_args!($($arg)*)));
                    if states.insert(message.clone()) {
                        eprintln!("{message}");
                    }
                }
            }
        }
    }};
}

pub enum CIProvider {
    GitHubActions,
}

impl CIProvider {
    pub fn from_env() -> Option<Self> {
        if std::env::var_os(EnvVars::GITHUB_ACTIONS).is_some() {
            Some(Self::GitHubActions)
        } else {
            None
        }
    }

    pub fn format(&self, message: &str) -> String {
        match self {
            Self::GitHubActions => {
                format!("::warning ::{message}")
            }
        }
    }
}
