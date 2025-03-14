//! Derived from `pypi_types_crate`.

use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::warn;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;

use crate::lenient_requirement::LenientRequirement;
use crate::metadata::pyproject_toml::PyProjectToml;
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
    /// Whether the version field is dynamic.
    #[serde(default)]
    pub dynamic: bool,
}

/// From <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/metadata.rs#LL78C2-L91C26>
impl ResolutionMetadata {
    /// Parse the [`ResolutionMetadata`] from a `METADATA` file, as included in a built distribution (wheel).
    pub fn parse_metadata(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;

        let name = PackageName::from_owned(
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
            .filter_map(
                |provides_extra| match ExtraName::from_owned(provides_extra) {
                    Ok(extra_name) => Some(extra_name),
                    Err(err) => {
                        warn!("Ignoring invalid extra: {err}");
                        None
                    }
                },
            )
            .collect::<Vec<_>>();
        let dynamic = headers
            .get_all_values("Dynamic")
            .any(|field| field == "Version");

        Ok(Self {
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
            dynamic,
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
        let mut dynamic = false;
        for field in headers.get_all_values("Dynamic") {
            match field.as_str() {
                "Requires-Python" => return Err(MetadataError::DynamicField("Requires-Python")),
                "Requires-Dist" => return Err(MetadataError::DynamicField("Requires-Dist")),
                "Provides-Extra" => return Err(MetadataError::DynamicField("Provides-Extra")),
                "Version" => dynamic = true,
                _ => (),
            }
        }

        // The `Name` and `Version` fields are required, and can't be dynamic.
        let name = PackageName::from_owned(
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
            .filter_map(
                |provides_extra| match ExtraName::from_owned(provides_extra) {
                    Ok(extra_name) => Some(extra_name),
                    Err(err) => {
                        warn!("Ignoring invalid extra: {err}");
                        None
                    }
                },
            )
            .collect::<Vec<_>>();

        Ok(Self {
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
            dynamic,
        })
    }

    /// Extract the metadata from a `pyproject.toml` file, as specified in PEP 621.
    ///
    /// If we're coming from a source distribution, we may already know the version (unlike for a
    /// source tree), so we can tolerate dynamic versions.
    pub fn parse_pyproject_toml(
        pyproject_toml: PyProjectToml,
        sdist_version: Option<&Version>,
    ) -> Result<Self, MetadataError> {
        let project = pyproject_toml
            .project
            .ok_or(MetadataError::FieldNotFound("project"))?;

        // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml` file.
        let mut dynamic = false;
        for field in project.dynamic.unwrap_or_default() {
            match field.as_str() {
                "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
                "optional-dependencies" => {
                    return Err(MetadataError::DynamicField("optional-dependencies"))
                }
                "requires-python" => return Err(MetadataError::DynamicField("requires-python")),
                // When building from a source distribution, the version is known from the filename and
                // fixed by it, so we can pretend it's static.
                "version" => {
                    if sdist_version.is_none() {
                        return Err(MetadataError::DynamicField("version"));
                    }
                    dynamic = true;
                }
                _ => (),
            }
        }

        // If dependencies are declared with Poetry, and `project.dependencies` is omitted, treat
        // the dependencies as dynamic. The inclusion of a `project` table without defining
        // `project.dependencies` is almost certainly an error.
        if project.dependencies.is_none()
            && pyproject_toml.tool.and_then(|tool| tool.poetry).is_some()
        {
            return Err(MetadataError::PoetrySyntax);
        }

        let name = project.name;
        let version = project
            .version
            // When building from a source distribution, the version is known from the filename and
            // fixed by it, so we can pretend it's static.
            .or_else(|| sdist_version.cloned())
            .ok_or(MetadataError::FieldNotFound("version"))?;

        // Parse the Python version requirements.
        let requires_python = project
            .requires_python
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?;

        // Extract the requirements.
        let mut requires_dist = project
            .dependencies
            .unwrap_or_default()
            .into_iter()
            .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
            .map_ok(Requirement::from)
            .collect::<Result<Vec<_>, _>>()?;

        // Extract the optional dependencies.
        let mut provides_extras: Vec<ExtraName> = Vec::new();
        for (extra, requirements) in project.optional_dependencies.unwrap_or_default() {
            requires_dist.extend(
                requirements
                    .into_iter()
                    .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
                    .map_ok(Requirement::from)
                    .map_ok(|requirement| requirement.with_extra_marker(&extra))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            provides_extras.push(extra);
        }

        Ok(Self {
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
            dynamic,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;
    use uv_pep440::Version;

    use super::*;
    use crate::MetadataError;

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

    #[test]
    fn test_parse_pyproject_toml() {
        let s = r#"
        [project]
        name = "asdf"
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None);
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("version"))));

        let s = r#"
        [project]
        name = "asdf"
        dynamic = ["version"]
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None);
        assert!(matches!(meta, Err(MetadataError::DynamicField("version"))));

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert!(meta.requires_python.is_none());
        assert!(meta.requires_dist.is_empty());
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert!(meta.requires_dist.is_empty());
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]

        [project.optional-dependencies]
        dotenv = ["bar"]
    "#;
        let pyproject = PyProjectToml::from_toml(s).unwrap();
        let meta = ResolutionMetadata::parse_pyproject_toml(pyproject, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert_eq!(
            meta.requires_dist,
            vec![
                "foo".parse().unwrap(),
                "bar; extra == \"dotenv\"".parse().unwrap()
            ]
        );
        assert_eq!(meta.provides_extras, vec!["dotenv".parse().unwrap()]);
    }
}
