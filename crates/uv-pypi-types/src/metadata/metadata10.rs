use serde::Deserialize;

use uv_normalize::PackageName;

use crate::metadata::Headers;
use crate::MetadataError;

/// A subset of the full core metadata specification, including only the
/// fields that have been consistent across all versions of the specification.
///
/// Core Metadata 1.0 is specified in <https://peps.python.org/pep-0241/>.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata10 {
    pub name: PackageName,
    pub version: String,
}

impl Metadata10 {
    /// Parse the [`Metadata10`] from a `PKG-INFO` file, as included in a source distribution.
    pub fn parse_pkg_info(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;
        let name = PackageName::from_owned(
            headers
                .get_first_value("Name")
                .ok_or(MetadataError::FieldNotFound("Name"))?,
        )?;
        let version = headers
            .get_first_value("Version")
            .ok_or(MetadataError::FieldNotFound("Version"))?;
        Ok(Self { name, version })
    }
}
