use std::fmt::{Display, Formatter};
use std::str::FromStr;

use thiserror::Error;
use uv_normalize::PackageName;
use uv_pep440::Version;

pub use build_tag::{BuildTag, BuildTagError};
pub use egg::{EggInfoFilename, EggInfoFilenameError};
pub use expanded_tags::{ExpandedTagError, ExpandedTags};
pub use extension::{DistExtension, ExtensionError, SourceDistExtension};
pub use source_dist::{SourceDistFilename, SourceDistFilenameError};
pub use wheel::{WheelFilename, WheelFilenameError};

mod build_tag;
mod egg;
mod expanded_tags;
mod extension;
mod source_dist;
mod splitter;
mod wheel;
mod wheel_tag;

pub(crate) fn normalized_package_name_matches(actual: &str, expected: &PackageName) -> bool {
    actual
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' => byte.to_ascii_lowercase(),
            b'_' | b'.' => b'-',
            _ => byte,
        })
        .eq(expected.as_ref().bytes())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DistFilename {
    SourceDistFilename(SourceDistFilename),
    WheelFilename(WheelFilename),
}

impl DistFilename {
    /// Parse a filename as wheel or source dist name.
    pub fn try_from_filename(filename: &str, package_name: &PackageName) -> Option<Self> {
        Self::try_from_filename_with_reason(filename, package_name).ok()
    }

    /// Parse a filename as wheel or source dist name, returning the reason the filename was
    /// rejected when parsing fails.
    ///
    /// This is useful for surfacing actionable diagnostics when a registry returns entries that
    /// do not look like distribution files (for example, devpi index entries like `+searchhelp`,
    /// bare version directory links, or files with unrecognized extensions).
    pub fn try_from_filename_with_reason(
        filename: &str,
        package_name: &PackageName,
    ) -> Result<Self, DistFilenameError> {
        match DistExtension::from_path(filename) {
            Ok(DistExtension::Wheel) => {
                match WheelFilename::from_str_with_expected_package_name(filename, package_name) {
                    Ok(filename) => Ok(Self::WheelFilename(filename)),
                    Err(err) => Err(DistFilenameError::InvalidWheel(err)),
                }
            }
            Ok(DistExtension::Source(extension)) => {
                match SourceDistFilename::parse(filename, extension, package_name) {
                    Ok(filename) => Ok(Self::SourceDistFilename(filename)),
                    Err(err) => Err(DistFilenameError::InvalidSourceDist(err)),
                }
            }
            Err(err) => Err(DistFilenameError::NoRecognizedExtension(err)),
        }
    }

    /// Like [`DistFilename::try_from_normalized_filename`], but without knowing the package name.
    ///
    /// Source dist filenames can be ambiguous, e.g. `a-1-1.tar.gz`. Without knowing the package name, we assume that
    /// source dist filename version doesn't contain minus (the version is normalized).
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        if let Ok(filename) = WheelFilename::from_str(filename) {
            Some(Self::WheelFilename(filename))
        } else if let Ok(filename) = SourceDistFilename::parsed_normalized_filename(filename) {
            Some(Self::SourceDistFilename(filename))
        } else {
            None
        }
    }

    pub fn name(&self) -> &PackageName {
        match self {
            Self::SourceDistFilename(filename) => &filename.name,
            Self::WheelFilename(filename) => &filename.name,
        }
    }

    pub fn version(&self) -> &Version {
        match self {
            Self::SourceDistFilename(filename) => &filename.version,
            Self::WheelFilename(filename) => &filename.version,
        }
    }

    pub fn into_version(self) -> Version {
        match self {
            Self::SourceDistFilename(filename) => filename.version,
            Self::WheelFilename(filename) => filename.version,
        }
    }

    /// Whether the file is a `bdist_wheel` or an `sdist`.
    pub fn filetype(&self) -> &'static str {
        match self {
            Self::SourceDistFilename(_) => "sdist",
            Self::WheelFilename(_) => "bdist_wheel",
        }
    }
}

impl Display for DistFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceDistFilename(filename) => Display::fmt(filename, f),
            Self::WheelFilename(filename) => Display::fmt(filename, f),
        }
    }
}

/// The reason a registry entry could not be parsed as a wheel or source distribution filename.
#[derive(Error, Debug)]
pub enum DistFilenameError {
    /// The filename does not have a recognized wheel or source distribution extension.
    ///
    /// This typically indicates the registry returned a non-distribution entry, such as a
    /// directory listing link (e.g., a bare version like `0.1.0`) or an index management endpoint
    /// (e.g., devpi's `+searchhelp`, `+status`).
    #[error("not a wheel or source distribution archive (expected one of {0})")]
    NoRecognizedExtension(#[source] ExtensionError),
    /// The filename has a `.whl` extension but is otherwise an invalid wheel filename.
    #[error(transparent)]
    InvalidWheel(#[from] WheelFilenameError),
    /// The filename has a source distribution extension but is otherwise invalid.
    #[error(transparent)]
    InvalidSourceDist(#[from] SourceDistFilenameError),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;

    use crate::{DistFilename, DistFilenameError, WheelFilename};

    #[test]
    fn wheel_filename_size() {
        assert_eq!(size_of::<WheelFilename>(), 48);
    }

    #[test]
    fn try_from_filename_with_reason_no_extension() {
        // A bare version string (the kind of entry devpi serves for its directory listings)
        // is rejected because it has no recognized distribution extension.
        let name = PackageName::from_str("my-package").unwrap();
        let err = DistFilename::try_from_filename_with_reason("0.1.0", &name).unwrap_err();
        assert!(
            matches!(err, DistFilenameError::NoRecognizedExtension(_)),
            "unexpected error variant: {err:?}"
        );
        let rendered = err.to_string();
        assert!(
            rendered.contains("not a wheel or source distribution archive"),
            "unexpected error message: {rendered}"
        );
    }

    #[test]
    fn try_from_filename_with_reason_empty_filename() {
        // An empty filename (which is what devpi reports for its top-level index entries) is
        // similarly rejected with the no-extension reason rather than silently swallowing it.
        let name = PackageName::from_str("my-package").unwrap();
        let err = DistFilename::try_from_filename_with_reason("", &name).unwrap_err();
        assert!(
            matches!(err, DistFilenameError::NoRecognizedExtension(_)),
            "unexpected error variant: {err:?}"
        );
    }

    #[test]
    fn try_from_filename_with_reason_invalid_wheel() {
        // A file that looks like a wheel by extension but isn't a valid wheel name should bubble
        // up the wheel parsing error rather than a generic extension error.
        let name = PackageName::from_str("my-package").unwrap();
        let err =
            DistFilename::try_from_filename_with_reason("not-a-wheel.whl", &name).unwrap_err();
        assert!(
            matches!(err, DistFilenameError::InvalidWheel(_)),
            "unexpected error variant: {err:?}"
        );
    }

    #[test]
    fn try_from_filename_accepts_valid_wheel() {
        let name = PackageName::from_str("my-package").unwrap();
        let parsed = DistFilename::try_from_filename("my_package-0.1.0-py3-none-any.whl", &name);
        assert!(parsed.is_some(), "expected wheel to parse successfully");
    }
}
