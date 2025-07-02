use crate::FileLocation;
use uv_normalize::PackageName;
use uv_pep440::Version;

/// A `<name>-<version>-variant.json` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantJson {
    pub name: PackageName,
    pub version: Version,
    pub file_location: FileLocation,
}
