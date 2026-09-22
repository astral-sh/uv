use std::borrow::Cow;
use std::fmt::{Display, Formatter};

use uv_normalize::PackageName;

use crate::Error;

/// The stem of a `.dist-info` directory whose normalized form starts with the expected package name.
///
/// Stores the original spelling, without the `.dist-info` suffix, so it can be used to locate files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistInfoStem<'a>(Cow<'a, str>);

impl<'a> DistInfoStem<'a> {
    /// Validate a `.dist-info` directory stem against its package name.
    ///
    /// Like `pip`, only require the normalized stem to start with the canonical package name. Some
    /// wheels use names that do not follow the current name and version rules.
    pub(crate) fn new(
        stem: impl Into<Cow<'a, str>>,
        package_name: &PackageName,
    ) -> Result<Self, Error> {
        let stem = stem.into();
        if !normalize(&stem).starts_with(package_name.as_str()) {
            return Err(Error::MissingDistInfoPackageName(
                stem.into_owned(),
                package_name.to_string(),
            ));
        }
        Ok(Self(stem))
    }

    /// Return the original directory stem, without the `.dist-info` suffix.
    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for DistInfoStem<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Normalize a `.dist-info` stem, converting it to lowercase and collapsing runs of `-`, `_`,
/// and `.` down to a single `-`.
///
/// Unlike [`PackageName`], this does not restrict the allowed characters.
fn normalize(stem: &str) -> Cow<'_, str> {
    if is_normalized(stem) {
        return Cow::Borrowed(stem);
    }

    let mut normalized = String::with_capacity(stem.len());
    let mut last = None;
    for char in stem.bytes() {
        match char {
            b'A'..=b'Z' => {
                normalized.push(char.to_ascii_lowercase() as char);
            }
            b'-' | b'_' | b'.' => {
                if matches!(last, Some(b'-' | b'_' | b'.')) {
                    continue;
                }
                normalized.push('-');
            }
            _ => {
                normalized.push(char as char);
            }
        }
        last = Some(char);
    }
    Cow::Owned(normalized)
}

/// Returns `true` if the stem is already normalized.
fn is_normalized(stem: &str) -> bool {
    let mut last = None;
    for char in stem.bytes() {
        match char {
            b'A'..=b'Z' => {
                // Uppercase characters need to be converted to lowercase.
                return false;
            }
            b'_' | b'.' => {
                // `_` and `.` are normalized to `-`.
                return false;
            }
            b'-' => {
                if matches!(last, Some(b'-')) {
                    // Runs of `-` are normalized to a single `-`.
                    return false;
                }
            }
            _ => {}
        }
        last = Some(char);
    }
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalize() {
        let inputs = [
            "friendly-bard",
            "Friendly-Bard",
            "FRIENDLY-BARD",
            "friendly.bard",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(super::normalize(input), "friendly-bard");
        }
    }
}
