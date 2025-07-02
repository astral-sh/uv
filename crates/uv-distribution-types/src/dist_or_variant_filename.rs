use crate::VariantJson;
use uv_distribution_filename::DistFilename;

/// On an index page, there can be wheels, source distributions and `variant.json` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistOrVariantFilename {
    DistFilename(DistFilename),
    VariantJson(VariantJson),
}
