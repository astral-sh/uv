use crate::metadata::{parse_version, Headers};
use crate::{LenientVersionSpecifiers, MetadataError};
use serde::Deserialize;
use std::str::FromStr;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};

/// A subset of the full cure metadata specification, only including the
/// fields that have been consistent across all versions of the specification later than 1.2, with
/// the exception of `Dynamic`, which is optional (but introduced in Metadata 2.2).
///
/// Python Package Metadata 1.2 is specified in <https://peps.python.org/pep-0345/>.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata12 {
    pub name: PackageName,
    pub version: Version,
    pub requires_python: Option<VersionSpecifiers>,
    pub dynamic: Vec<String>,
}

impl Metadata12 {
    /// Parse the [`Metadata12`] from a `.dist-info/METADATA` file, as included in a built
    /// distribution.
    pub fn parse_metadata(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;

        // To rely on a source distribution's `PKG-INFO` file, the `Metadata-Version` field must be
        // present and set to a value of at least `2.2`.
        let metadata_version = headers
            .get_first_value("Metadata-Version")
            .ok_or(MetadataError::FieldNotFound("Metadata-Version"))?;

        // Parse the version into (major, minor).
        let (major, minor) = parse_version(&metadata_version)?;

        // At time of writing:
        // > Version of the file format; legal values are “1.0”, “1.1”, “1.2”, “2.1”, “2.2”, and “2.3”.
        if (major, minor) < (1, 0) || (major, minor) >= (3, 0) {
            return Err(MetadataError::InvalidMetadataVersion(metadata_version));
        }

        let name = PackageName::new(
            headers
                .get_first_value("Name")
                .ok_or(MetadataError::FieldNotFound("Name"))?,
        )?;
        let version = Version::from_str(
            &headers
                .get_first_value("Version")
                .ok_or(MetadataError::FieldNotFound("Version"))?,
        )
        .map_err(MetadataError::Pep440VersionError)?;
        let requires_python = headers
            .get_first_value("Requires-Python")
            .map(|requires_python| LenientVersionSpecifiers::from_str(&requires_python))
            .transpose()?
            .map(VersionSpecifiers::from);
        let dynamic = headers.get_all_values("Dynamic").collect::<Vec<_>>();

        Ok(Self {
            name,
            version,
            requires_python,
            dynamic,
        })
    }
}
