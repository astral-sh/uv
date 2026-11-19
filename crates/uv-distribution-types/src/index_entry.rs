use std::fmt::Display;
use std::str::FromStr;

use uv_distribution_filename::{DistFilename, DistFilenameError};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::VariantsJsonFilename;

/// A distribution or variant metadata filename listed by an index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexEntryFilename {
    DistFilename(DistFilename),
    VariantJson(VariantsJsonFilename),
}

impl IndexEntryFilename {
    pub fn name(&self) -> &PackageName {
        match self {
            Self::DistFilename(filename) => filename.name(),
            Self::VariantJson(filename) => &filename.name,
        }
    }

    pub fn version(&self) -> &Version {
        match self {
            Self::DistFilename(filename) => filename.version(),
            Self::VariantJson(filename) => &filename.version,
        }
    }

    /// Whether the entry refers to an installable file.
    pub fn is_distribution(&self) -> bool {
        match self {
            Self::DistFilename(_) => true,
            Self::VariantJson(_) => false,
        }
    }

    /// Parse a filename without knowing its package name.
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        // Check the extension first to avoid allocating the error.
        if filename.ends_with("-variants.json")
            && let Ok(filename) = VariantsJsonFilename::from_str(filename)
        {
            Some(Self::VariantJson(filename))
        } else {
            DistFilename::try_from_normalized_filename(filename).map(Self::DistFilename)
        }
    }

    /// Parse a filename belonging to the given package.
    pub fn try_from_filename(filename: &str, package_name: &PackageName) -> Option<Self> {
        Self::try_from_filename_with_reason(filename, package_name).ok()
    }

    /// Parse a filename, returning the reason a distribution filename was rejected.
    pub fn try_from_filename_with_reason(
        filename: &str,
        package_name: &PackageName,
    ) -> Result<Self, DistFilenameError> {
        match DistFilename::try_from_filename_with_reason(filename, package_name) {
            Ok(filename) => Ok(Self::DistFilename(filename)),
            Err(err) => {
                if let Ok(filename) = VariantsJsonFilename::from_str(filename)
                    && &filename.name == package_name
                {
                    Ok(Self::VariantJson(filename))
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl Display for IndexEntryFilename {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DistFilename(filename) => filename.fmt(f),
            Self::VariantJson(filename) => filename.fmt(f),
        }
    }
}
