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
    let mut last_was_separator = false;
    let normalized = stem.bytes().filter_map(move |byte| {
        let byte = match byte {
            b'-' | b'_' | b'.' => b'-',
            byte => byte.to_ascii_lowercase(),
        };
        let is_separator = byte == b'-';
        let repeated_separator = last_was_separator && is_separator;
        last_was_separator = is_separator;
        (!repeated_separator).then_some(byte)
    });

    if normalized.clone().eq(stem.bytes()) {
        return Cow::Borrowed(stem);
    }

    let mut output = String::with_capacity(stem.len());
    output.extend(normalized.map(char::from));
    Cow::Owned(output)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

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

    #[test]
    fn normalize_borrowed() {
        for input in ["", "-", "friendly-bard", "friendly+local!", "café"] {
            assert_eq!(
                match super::normalize(input) {
                    Cow::Borrowed(normalized) => Some(normalized),
                    Cow::Owned(_) => None,
                },
                Some(input)
            );
        }
    }
}
