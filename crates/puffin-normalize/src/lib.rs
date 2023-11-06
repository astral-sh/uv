use std::error::Error;
use std::fmt::{Display, Formatter};

use once_cell::sync::Lazy;
use regex::Regex;

pub use extra_name::ExtraName;
pub use package_name::PackageName;

mod extra_name;
mod package_name;

pub(crate) static NAME_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-_.]+").unwrap());
pub(crate) static NAME_VALIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$").unwrap());

pub(crate) fn validate_and_normalize_ref(
    name: impl AsRef<str>,
) -> Result<String, InvalidNameError> {
    if !NAME_VALIDATE.is_match(name.as_ref()) {
        return Err(InvalidNameError(name.as_ref().to_string()));
    }
    let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "-").to_string();
    normalized.make_ascii_lowercase();
    Ok(normalized)
}

pub(crate) fn validate_and_normalize_owned(mut name: String) -> Result<String, InvalidNameError> {
    if !NAME_VALIDATE.is_match(name.as_ref()) {
        return Err(InvalidNameError(name));
    }
    let normalized = NAME_NORMALIZE.replace_all(&name, "-");
    // fast path: Don't allocate if we don't need to. An inplace ascii char replace would be
    // nicer but doesn't exist
    if normalized != name {
        name = normalized.to_string();
    }
    name.make_ascii_lowercase();
    Ok(name)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidNameError(String);

impl Display for InvalidNameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Not a valid package or extra name: \"{}\". Names must start and end with a letter or \
            digit and may only contain -, _, ., and alphanumeric characters",
            self.0
        )
    }
}

impl Error for InvalidNameError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        let inputs = [
            "friendly-bard",
            "Friendly-Bard",
            "FRIENDLY-BARD",
            "friendly.bard",
            "friendly_bard",
            "friendly--bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(validate_and_normalize_ref(input).unwrap(), "friendly-bard");
            assert_eq!(
                validate_and_normalize_owned(input.to_string()).unwrap(),
                "friendly-bard"
            );
        }
    }

    #[test]
    fn unchanged() {
        // Unchanged
        let unchanged = ["friendly-bard", "1okay", "okay2"];
        for input in unchanged {
            assert_eq!(validate_and_normalize_ref(input).unwrap(), input);
            assert_eq!(
                validate_and_normalize_owned(input.to_string()).unwrap(),
                input
            );
        }
    }

    #[test]
    fn failures() {
        let failures = [
            " starts-with-space",
            "-starts-with-dash",
            "ends-with-dash-",
            "ends-with-space ",
            "includes!invalid-char",
            "space in middle",
        ];
        for input in failures {
            assert!(validate_and_normalize_ref(input).is_err());
            assert!(validate_and_normalize_owned(input.to_string()).is_err(),);
        }
    }
}
