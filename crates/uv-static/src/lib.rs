pub use env_vars::*;

mod env_vars;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("Failed to parse environment variable `{name}` with invalid value `{value}`: {err}")]
pub struct InvalidEnvironmentVariable {
    pub name: String,
    pub value: String,
    pub err: String,
}

/// Parse a boolean environment variable.
///
/// Adapted from Clap's `BoolishValueParser` which is dual licensed under the MIT and Apache-2.0.
pub fn parse_boolish_environment_variable(
    name: &'static str,
) -> Result<Option<bool>, InvalidEnvironmentVariable> {
    // See `clap_builder/src/util/str_to_bool.rs`
    // We want to match Clap's accepted values

    // True values are `y`, `yes`, `t`, `true`, `on`, and `1`.
    const TRUE_LITERALS: [&str; 6] = ["y", "yes", "t", "true", "on", "1"];

    // False values are `n`, `no`, `f`, `false`, `off`, and `0`.
    const FALSE_LITERALS: [&str; 6] = ["n", "no", "f", "false", "off", "0"];

    // Converts a string literal representation of truth to true or false.
    //
    // `false` values are `n`, `no`, `f`, `false`, `off`, and `0` (case insensitive).
    //
    // Any other value will be considered as `true`.
    fn str_to_bool(val: impl AsRef<str>) -> Option<bool> {
        let pat: &str = &val.as_ref().to_lowercase();
        if TRUE_LITERALS.contains(&pat) {
            Some(true)
        } else if FALSE_LITERALS.contains(&pat) {
            Some(false)
        } else {
            None
        }
    }

    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };

    let Some(value) = value.to_str() else {
        return Err(InvalidEnvironmentVariable {
            name: name.to_string(),
            value: value.to_string_lossy().to_string(),
            err: "expected a valid UTF-8 string".to_string(),
        });
    };

    let Some(value) = str_to_bool(value) else {
        return Err(InvalidEnvironmentVariable {
            name: name.to_string(),
            value: value.to_string(),
            err: "expected a boolish value".to_string(),
        });
    };

    Ok(Some(value))
}
