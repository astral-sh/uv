use std::error::Error;
use std::fmt::{Display, Formatter};

pub use dist_info_name::DistInfoName;
pub use extra_name::ExtraName;
pub use group_name::{GroupName, DEV_DEPENDENCIES};
pub use package_name::PackageName;

mod dist_info_name;
mod extra_name;
mod group_name;
mod package_name;

/// Validate and normalize an owned package or extra name.
pub(crate) fn validate_and_normalize_owned(name: String) -> Result<String, InvalidNameError> {
    if is_normalized(&name)? {
        Ok(name)
    } else {
        validate_and_normalize_ref(name)
    }
}

/// Validate and normalize an unowned package or extra name.
pub(crate) fn validate_and_normalize_ref(
    name: impl AsRef<str>,
) -> Result<String, InvalidNameError> {
    let name = name.as_ref();
    let mut normalized = String::with_capacity(name.len());

    let mut last = None;
    for char in name.bytes() {
        match char {
            b'A'..=b'Z' => {
                normalized.push(char.to_ascii_lowercase() as char);
            }
            b'a'..=b'z' | b'0'..=b'9' => {
                normalized.push(char as char);
            }
            b'-' | b'_' | b'.' => {
                match last {
                    // Names can't start with punctuation.
                    None => return Err(InvalidNameError(name.to_string())),
                    Some(b'-' | b'_' | b'.') => {}
                    Some(_) => normalized.push('-'),
                }
            }
            _ => return Err(InvalidNameError(name.to_string())),
        }
        last = Some(char);
    }

    // Names can't end with punctuation.
    if matches!(last, Some(b'-' | b'_' | b'.')) {
        return Err(InvalidNameError(name.to_string()));
    }

    Ok(normalized)
}

/// Returns `true` if the name is already normalized.
fn is_normalized(name: impl AsRef<str>) -> Result<bool, InvalidNameError> {
    let mut last = None;
    for char in name.as_ref().bytes() {
        match char {
            b'A'..=b'Z' => {
                // Uppercase characters need to be converted to lowercase.
                return Ok(false);
            }
            b'a'..=b'z' | b'0'..=b'9' => {}
            b'_' | b'.' => {
                // `_` and `.` are normalized to `-`.
                return Ok(false);
            }
            b'-' => {
                match last {
                    // Names can't start with punctuation.
                    None => return Err(InvalidNameError(name.as_ref().to_string())),
                    Some(b'-') => {
                        // Runs of `-` are normalized to a single `-`.
                        return Ok(false);
                    }
                    Some(_) => {}
                }
            }
            _ => return Err(InvalidNameError(name.as_ref().to_string())),
        }
        last = Some(char);
    }

    // Names can't end with punctuation.
    if matches!(last, Some(b'-' | b'_' | b'.')) {
        return Err(InvalidNameError(name.as_ref().to_string()));
    }

    Ok(true)
}

/// Invalid [`crate::PackageName`] or [`crate::ExtraName`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidNameError(String);

impl InvalidNameError {
    /// Returns the invalid name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for InvalidNameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Not a valid package or extra name: \"{}\". Names must start and end with a letter or \
            digit and may only contain -, _, ., and alphanumeric characters.",
            self.0
        )
    }
}

impl Error for InvalidNameError {}

#[cfg(test)]
mod tests;
