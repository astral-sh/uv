use std::str::FromStr;

use uv_distribution_filename::DistFilename;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::VariantsJson;

/// On an index page, there can be wheels, source distributions and `variants.json` files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexEntryFilename {
    DistFilename(DistFilename),
    VariantJson(VariantsJson),
}

impl IndexEntryFilename {
    pub fn name(&self) -> &PackageName {
        match self {
            Self::DistFilename(filename) => filename.name(),
            Self::VariantJson(variant_json) => &variant_json.name,
        }
    }

    pub fn version(&self) -> &Version {
        match self {
            Self::DistFilename(filename) => filename.version(),
            Self::VariantJson(variant_json) => &variant_json.version,
        }
    }

    /// Parse a filename as either a distribution filename or a `variants.json` filename.
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        if let Some(dist_filename) = DistFilename::try_from_normalized_filename(filename) {
            Some(Self::DistFilename(dist_filename))
        } else if let Ok(variant_json) = VariantsJson::from_str(filename) {
            Some(Self::VariantJson(variant_json))
        } else {
            None
        }
    }

    /// Parse a filename as either a distribution filename or a `variants.json` filename.
    pub fn try_from_filename(filename: &str, package_name: &PackageName) -> Option<Self> {
        if let Some(dist_filename) = DistFilename::try_from_filename(filename, package_name) {
            Some(Self::DistFilename(dist_filename))
        } else if let Ok(variant_json) = VariantsJson::from_str(filename) {
            Some(Self::VariantJson(variant_json))
        } else {
            None
        }
    }
}
