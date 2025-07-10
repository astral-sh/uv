use std::borrow::Cow;
use std::fmt;
use std::fmt::{Display, Formatter};

/// The normalized name of a `.dist-info` directory.
///
/// Like [`PackageName`](crate::PackageName), but without restrictions on the set of allowed
/// characters, etc.
///
/// See: <https://github.com/pypa/pip/blob/111eed14b6e9fba7c78a5ec2b7594812d17b5d2b/src/pip/_vendor/packaging/utils.py#L45>
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DistInfoName<'a>(Cow<'a, str>);

impl<'a> DistInfoName<'a> {
    /// Create a validated, normalized `.dist-info` directory name.
    pub fn new(name: &'a str) -> Self {
        if Self::is_normalized(name) {
            Self(Cow::Borrowed(name))
        } else {
            Self(Cow::Owned(Self::normalize(name)))
        }
    }

    /// Normalize a `.dist-info` name, converting it to lowercase and collapsing runs
    /// of `-`, `_`, and `.` down to a single `-`.
    fn normalize(name: impl AsRef<str>) -> String {
        let mut normalized = String::with_capacity(name.as_ref().len());
        let mut last = None;
        for char in name.as_ref().bytes() {
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
        normalized
    }

    /// Returns `true` if the name is already normalized.
    fn is_normalized(name: impl AsRef<str>) -> bool {
        let mut last = None;
        for char in name.as_ref().bytes() {
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
}

impl Display for DistInfoName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for DistInfoName<'_> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

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
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(DistInfoName::normalize(input), "friendly-bard");
        }
    }
}
