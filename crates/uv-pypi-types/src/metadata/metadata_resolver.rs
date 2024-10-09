//! Derived from `pypi_types_crate`.

use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::warn;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;

use crate::lenient_requirement::LenientRequirement;
use crate::metadata::pyproject_toml::parse_pyproject_toml;
use crate::metadata::Headers;
use crate::{metadata, LenientVersionSpecifiers, MetadataError, VerbatimParsedUrl};

/// A subset of the full core metadata specification, including only the
/// fields that are relevant to dependency resolution.
///
/// Core Metadata 2.3 is specified in <https://packaging.python.org/specifications/core-metadata/>.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ResolutionMetadata {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

/// From <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/metadata.rs#LL78C2-L91C26>
impl ResolutionMetadata {
    /// Parse the [`ResolutionMetadata`] from a `METADATA` file, as included in a built distribution (wheel).
    pub fn parse_metadata(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;

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
        let requires_dist = headers
            .get_all_values("Requires-Dist")
            .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
            .map_ok(Requirement::from)
            .collect::<Result<Vec<_>, _>>()?;
        let requires_python = headers
            .get_first_value("Requires-Python")
            .map(|requires_python| LenientVersionSpecifiers::from_str(&requires_python))
            .transpose()?
            .map(VersionSpecifiers::from);
        let provides_extras = headers
            .get_all_values("Provides-Extra")
            .filter_map(|provides_extra| match ExtraName::new(provides_extra) {
                Ok(extra_name) => Some(extra_name),
                Err(err) => {
                    warn!("Ignoring invalid extra: {err}");
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
        })
    }

    /// Read the [`ResolutionMetadata`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
    /// or later _and_ none of the required fields (`Requires-Python`, `Requires-Dist`, and
    /// `Provides-Extra`) are marked as dynamic.
    pub fn parse_pkg_info(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;

        // To rely on a source distribution's `PKG-INFO` file, the `Metadata-Version` field must be
        // present and set to a value of at least `2.2`.
        let metadata_version = headers
            .get_first_value("Metadata-Version")
            .ok_or(MetadataError::FieldNotFound("Metadata-Version"))?;

        // Parse the version into (major, minor).
        let (major, minor) = metadata::parse_version(&metadata_version)?;
        if (major, minor) < (2, 2) || (major, minor) >= (3, 0) {
            return Err(MetadataError::UnsupportedMetadataVersion(metadata_version));
        }

        // If any of the fields we need are marked as dynamic, we can't use the `PKG-INFO` file.
        let dynamic = headers.get_all_values("Dynamic").collect::<Vec<_>>();
        for field in dynamic {
            match field.as_str() {
                "Requires-Python" => return Err(MetadataError::DynamicField("Requires-Python")),
                "Requires-Dist" => return Err(MetadataError::DynamicField("Requires-Dist")),
                "Provides-Extra" => return Err(MetadataError::DynamicField("Provides-Extra")),
                _ => (),
            }
        }

        // The `Name` and `Version` fields are required, and can't be dynamic.
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

        // The remaining fields are required to be present.
        let requires_dist = headers
            .get_all_values("Requires-Dist")
            .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
            .map_ok(Requirement::from)
            .collect::<Result<Vec<_>, _>>()?;
        let requires_python = headers
            .get_first_value("Requires-Python")
            .map(|requires_python| LenientVersionSpecifiers::from_str(&requires_python))
            .transpose()?
            .map(VersionSpecifiers::from);
        let provides_extras = headers
            .get_all_values("Provides-Extra")
            .filter_map(|provides_extra| match ExtraName::new(provides_extra) {
                Ok(extra_name) => Some(extra_name),
                Err(err) => {
                    warn!("Ignoring invalid extra: {err}");
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
        })
    }

    pub fn parse_pyproject_toml(toml: &str) -> Result<Self, MetadataError> {
        parse_pyproject_toml(toml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetadataError;
    use std::str::FromStr;
    use uv_normalize::PackageName;
    use uv_pep440::Version;

    #[test]
    fn test_parse_metadata() {
        let s = "Metadata-Version: 1.0";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Name"))));

        let s = "Metadata-Version: 1.0\nName: asdf";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?foobar?=\nVersion: 1.0";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("foobar").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?=C3=A4_space?= <x@y.org>\nVersion: 1.0";
        let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::InvalidName(_))));
    }

    #[test]
    fn test_parse_pkg_info() {
        let s = "Metadata-Version: 2.1";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
        assert!(matches!(
            meta,
            Err(MetadataError::UnsupportedMetadataVersion(_))
        ));

        let s = "Metadata-Version: 2.2\nName: asdf";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nDynamic: Requires-Dist";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap_err();
        assert!(matches!(meta, MetadataError::DynamicField("Requires-Dist")));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nRequires-Dist: foo";
        let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
    }
}
