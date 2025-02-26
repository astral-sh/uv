pub use env_vars::*;

mod env_vars;

/// Error messages for logging setup defined here for consistency
pub const LOG_DIR_ERROR: &str = "Error writing to log directory:";

pub const LOG_FILE_ERROR: &str = "Error writing to log file:";
