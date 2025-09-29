pub use env_vars::*;

mod env_vars;

/// Parse a boolean environment variable.
///
/// Adapted from Clap's `BoolishValueParser` which is dual licensed under the MIT and Apache-2.0.
pub fn parse_boolish_environment_variable(name: &'static str) -> Result<Option<bool>, String> {
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
        return Err(format!(
            "Failed to parse environment variable `{}` with invalid value `{}`: expected a valid UTF-8 string",
            name,
            value.to_string_lossy()
        ));
    };

    let Some(value) = str_to_bool(value) else {
        return Err(format!(
            "Failed to parse environment variable `{name}` with invalid value `{value}`: expected a boolish value"
        ));
    };

    Ok(Some(value))
}

/// Parse a string environment variable.
pub fn parse_string_environment_variable(name: &'static str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(v) => {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        Err(e) => match e {
            std::env::VarError::NotPresent => Ok(None),
            std::env::VarError::NotUnicode(err) => Err(format!(
                "Failed to parse environment variable `{}` with invalid value `{}`: expected a valid UTF-8 string",
                name,
                err.to_string_lossy()
            )),
        },
    }
}
