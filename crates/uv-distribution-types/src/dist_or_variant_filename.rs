use uv_distribution_filename::DistFilename;
use uv_normalize::PackageName;

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

    /// Parse a filename as either a distribution filename or a `variants.json` filename.
    #[allow(clippy::manual_map)]
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        if let Some(dist_filename) = DistFilename::try_from_normalized_filename(filename) {
            Some(Self::DistFilename(dist_filename))
        } else if let Some(variant_json) = VariantsJson::try_from_normalized_filename(filename) {
            Some(Self::VariantJson(variant_json))
        } else {
            None
        }
    }
}
