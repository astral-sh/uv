//! Derived from `pypi_types_crate`.

use std::str::FromStr;

use indexmap::IndexMap;
use itertools::Itertools;
use mailparse::{MailHeaderMap, MailParseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use pep440_rs::{Version, VersionParseError, VersionSpecifiers, VersionSpecifiersParseError};
use pep508_rs::{Pep508Error, Requirement};
use uv_normalize::{ExtraName, InvalidNameError, PackageName};

use crate::lenient_requirement::LenientRequirement;
use crate::{LenientVersionSpecifiers, VerbatimParsedUrl};

/// Python Package Metadata 2.3 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
///
/// This is a subset of the full metadata specification, and only includes the
/// fields that are relevant to dependency resolution.
///
/// At present, we support up to version 2.3 of the metadata specification.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata23 {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

/// <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/error.rs>
///
/// The error type
#[derive(Error, Debug)]
pub enum MetadataError {
    #[error(transparent)]
    MailParse(#[from] MailParseError),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("metadata field {0} not found")]
    FieldNotFound(&'static str),
    #[error("invalid version: {0}")]
    Pep440VersionError(VersionParseError),
    #[error(transparent)]
    Pep440Error(#[from] VersionSpecifiersParseError),
    #[error(transparent)]
    Pep508Error(#[from] Box<Pep508Error<VerbatimParsedUrl>>),
    #[error(transparent)]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid `Metadata-Version` field: {0}")]
    InvalidMetadataVersion(String),
    #[error("Reading metadata from `PKG-INFO` requires Metadata 2.2 or later (found: {0})")]
    UnsupportedMetadataVersion(String),
    #[error("The following field was marked as dynamic: {0}")]
    DynamicField(&'static str),
    #[error("The project uses Poetry's syntax to declare its dependencies, despite including a `project` table in `pyproject.toml`")]
    PoetrySyntax,
}

impl From<Pep508Error<VerbatimParsedUrl>> for MetadataError {
    fn from(error: Pep508Error<VerbatimParsedUrl>) -> Self {
        Self::Pep508Error(Box::new(error))
    }
}

/// From <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/metadata.rs#LL78C2-L91C26>
impl Metadata23 {
    /// Parse the [`Metadata23`] from a `METADATA` file, as included in a built distribution (wheel).
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

    /// Read the [`Metadata23`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
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
        let (major, minor) = parse_version(&metadata_version)?;
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

    /// Extract the metadata from a `pyproject.toml` file, as specified in PEP 621.
    pub fn parse_pyproject_toml(contents: &str) -> Result<Self, MetadataError> {
        let pyproject_toml: PyProjectToml = toml::from_str(contents)?;

        let project = pyproject_toml
            .project
            .ok_or(MetadataError::FieldNotFound("project"))?;

        // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml` file.
        let dynamic = project.dynamic.unwrap_or_default();
        for field in dynamic {
            match field.as_str() {
                "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
                "optional-dependencies" => {
                    return Err(MetadataError::DynamicField("optional-dependencies"))
                }
                "requires-python" => return Err(MetadataError::DynamicField("requires-python")),
                "version" => return Err(MetadataError::DynamicField("version")),
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
        })
    }
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<Project>,
    tool: Option<Tool>,
}

/// PEP 621 project metadata.
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for dependency resolution.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Project {
    /// The name of the project
    name: PackageName,
    /// The version of the project as supported by PEP 440
    version: Option<Version>,
    /// The Python version requirements of the project
    requires_python: Option<String>,
    /// Project dependencies
    dependencies: Option<Vec<String>>,
    /// Optional dependencies
    optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    dynamic: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Tool {
    poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::empty_structs_with_brackets)]
struct ToolPoetry {}

/// Python Package Metadata 1.0 and later as specified in
/// <https://peps.python.org/pep-0241/>.
///
/// This is a subset of the full metadata specification, and only includes the
/// fields that have been consistent across all versions of the specification.
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
        let name = PackageName::new(
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

/// Python Package Metadata 1.2 and later as specified in
/// <https://peps.python.org/pep-0345/>.
///
/// This is a subset of the full metadata specification, and only includes the
/// fields that have been consistent across all versions of the specification later than 1.2.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata12 {
    pub name: PackageName,
    pub version: Version,
    pub requires_python: Option<VersionSpecifiers>,
}

impl Metadata12 {
    /// Parse the [`Metadata12`] from a `.dist-info` `METADATA` file, as included in a built
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

        Ok(Self {
            name,
            version,
            requires_python,
        })
    }
}

/// Parse a `Metadata-Version` field into a (major, minor) tuple.
fn parse_version(metadata_version: &str) -> Result<(u8, u8), MetadataError> {
    let (major, minor) =
        metadata_version
            .split_once('.')
            .ok_or(MetadataError::InvalidMetadataVersion(
                metadata_version.to_string(),
            ))?;
    let major = major
        .parse::<u8>()
        .map_err(|_| MetadataError::InvalidMetadataVersion(metadata_version.to_string()))?;
    let minor = minor
        .parse::<u8>()
        .map_err(|_| MetadataError::InvalidMetadataVersion(metadata_version.to_string()))?;
    Ok((major, minor))
}

/// Python Package Metadata 2.3 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
///
/// This is a subset of [`Metadata23`]; specifically, it omits the `version` and `requires-python`
/// fields, which aren't necessary when extracting the requirements of a package without installing
/// the package itself.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub provides_extras: Vec<ExtraName>,
}

impl RequiresDist {
    /// Extract the [`RequiresDist`] from a `pyproject.toml` file, as specified in PEP 621.
    pub fn parse_pyproject_toml(contents: &str) -> Result<Self, MetadataError> {
        let pyproject_toml: PyProjectToml = toml::from_str(contents)?;

        let project = pyproject_toml
            .project
            .ok_or(MetadataError::FieldNotFound("project"))?;

        // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml`
        // file.
        let dynamic = project.dynamic.unwrap_or_default();
        for field in dynamic {
            match field.as_str() {
                "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
                "optional-dependencies" => {
                    return Err(MetadataError::DynamicField("optional-dependencies"))
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
            requires_dist,
            provides_extras,
        })
    }
}

/// The headers of a distribution metadata file.
#[derive(Debug)]
struct Headers<'a>(Vec<mailparse::MailHeader<'a>>);

impl<'a> Headers<'a> {
    /// Parse the headers from the given metadata file content.
    fn parse(content: &'a [u8]) -> Result<Self, MailParseError> {
        let (headers, _) = mailparse::parse_headers(content)?;
        Ok(Self(headers))
    }

    /// Return the first value associated with the header with the given name.
    fn get_first_value(&self, name: &str) -> Option<String> {
        self.0.get_first_header(name).and_then(|header| {
            let value = header.get_value();
            if value == "UNKNOWN" {
                None
            } else {
                Some(value)
            }
        })
    }

    /// Return all values associated with the header with the given name.
    fn get_all_values(&self, name: &str) -> impl Iterator<Item = String> {
        self.0
            .get_all_values(name)
            .into_iter()
            .filter(|value| value != "UNKNOWN")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pep440_rs::Version;
    use uv_normalize::PackageName;

    use crate::MetadataError;

    use super::Metadata23;

    #[test]
    fn test_parse_metadata() {
        let s = "Metadata-Version: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Name"))));

        let s = "Metadata-Version: 1.0\nName: asdf";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?foobar?=\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("foobar").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?=C3=A4_space?= <x@y.org>\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::InvalidName(_))));
    }

    #[test]
    fn test_parse_pkg_info() {
        let s = "Metadata-Version: 2.1";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(
            meta,
            Err(MetadataError::UnsupportedMetadataVersion(_))
        ));

        let s = "Metadata-Version: 2.2\nName: asdf";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nDynamic: Requires-Dist";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap_err();
        assert!(matches!(meta, MetadataError::DynamicField("Requires-Dist")));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nRequires-Dist: foo";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap();
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
        let meta = Metadata23::parse_pyproject_toml(s);
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("version"))));

        let s = r#"
            [project]
            name = "asdf"
            dynamic = ["version"]
        "#;
        let meta = Metadata23::parse_pyproject_toml(s);
        assert!(matches!(meta, Err(MetadataError::DynamicField("version"))));

        let s = r#"
            [project]
            name = "asdf"
            version = "1.0"
        "#;
        let meta = Metadata23::parse_pyproject_toml(s).unwrap();
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
        let meta = Metadata23::parse_pyproject_toml(s).unwrap();
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
        let meta = Metadata23::parse_pyproject_toml(s).unwrap();
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
        let meta = Metadata23::parse_pyproject_toml(s).unwrap();
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
