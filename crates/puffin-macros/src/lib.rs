use std::sync::Mutex;

use fxhash::FxHashSet;
use once_cell::sync::Lazy;

pub static WARNINGS: Lazy<Mutex<FxHashSet<String>>> = Lazy::new(Mutex::default);

/// Warn a user once, with uniqueness determined by the content of the message.
#[macro_export]
macro_rules! warn_once {
    ($($arg:tt)*) => {
        use colored::Colorize;
        use tracing::warn;

        if let Ok(mut states) = $crate::WARNINGS.lock() {
            let message = format!("{}", format_args!($($arg)*));
            let formatted = message.bold();
            if states.insert(message) {
                warn!("{formatted}");
            }
        }
    };
}
