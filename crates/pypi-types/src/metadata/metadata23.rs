//! Vendored from <https://github.com/PyO3/python-pkginfo-rs>

use crate::metadata::Headers;
use crate::MetadataError;
use std::str;
use std::str::FromStr;

/// Code Metadata 2.3 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata23 {
    /// Version of the file format; legal values are `1.0`, `1.1`, `1.2`, `2.1`, `2.2` and `2.3`.
    pub metadata_version: String,
    /// The name of the distribution.
    pub name: String,
    /// A string containing the distribution’s version number.
    pub version: String,
    /// A Platform specification describing an operating system supported by the distribution
    /// which is not listed in the “Operating System” Trove classifiers.
    pub platforms: Vec<String>,
    /// Binary distributions containing a PKG-INFO file will use the Supported-Platform field
    /// in their metadata to specify the OS and CPU for which the binary distribution was compiled.
    pub supported_platforms: Vec<String>,
    /// A one-line summary of what the distribution does.
    pub summary: Option<String>,
    /// A longer description of the distribution that can run to several paragraphs.
    pub description: Option<String>,
    /// A list of additional keywords, separated by commas, to be used to
    /// assist searching for the distribution in a larger catalog.
    pub keywords: Option<String>,
    /// A string containing the URL for the distribution’s home page.
    pub home_page: Option<String>,
    /// A string containing the URL from which this version of the distribution can be downloaded.
    pub download_url: Option<String>,
    /// A string containing the author’s name at a minimum; additional contact information may be provided.
    pub author: Option<String>,
    /// A string containing the author’s e-mail address. It can contain a name and e-mail address in the legal forms for a RFC-822 `From:` header.
    pub author_email: Option<String>,
    /// Text indicating the license covering the distribution where the license is not a selection from the `License` Trove classifiers or an SPDX license expression.
    pub license: Option<String>,
    /// An SPDX expression indicating the license covering the distribution.
    pub license_expression: Option<String>,
    /// Paths to files containing the text of the licenses covering the distribution.
    pub license_files: Vec<String>,
    /// Each entry is a string giving a single classification value for the distribution.
    pub classifiers: Vec<String>,
    /// Each entry contains a string naming some other distutils project required by this distribution.
    pub requires_dist: Vec<String>,
    /// Each entry contains a string naming a Distutils project which is contained within this distribution.
    pub provides_dist: Vec<String>,
    /// Each entry contains a string describing a distutils project’s distribution which this distribution renders obsolete,
    /// meaning that the two projects should not be installed at the same time.
    pub obsoletes_dist: Vec<String>,
    /// A string containing the maintainer’s name at a minimum; additional contact information may be provided.
    ///
    /// Note that this field is intended for use when a project is being maintained by someone other than the original author:
    /// it should be omitted if it is identical to `author`.
    pub maintainer: Option<String>,
    /// A string containing the maintainer’s e-mail address.
    /// It can contain a name and e-mail address in the legal forms for a RFC-822 `From:` header.
    ///
    /// Note that this field is intended for use when a project is being maintained by someone other than the original author:
    /// it should be omitted if it is identical to `author_email`.
    pub maintainer_email: Option<String>,
    /// This field specifies the Python version(s) that the distribution is guaranteed to be compatible with.
    pub requires_python: Option<String>,
    /// Each entry contains a string describing some dependency in the system that the distribution is to be used.
    pub requires_external: Vec<String>,
    /// A string containing a browsable URL for the project and a label for it, separated by a comma.
    pub project_urls: Vec<String>,
    /// A string containing the name of an optional feature. Must be a valid Python identifier.
    /// May be used to make a dependency conditional on whether the optional feature has been requested.
    pub provides_extras: Vec<String>,
    /// A string stating the markup syntax (if any) used in the distribution’s description,
    /// so that tools can intelligently render the description.
    pub description_content_type: Option<String>,
    /// A string containing the name of another core metadata field.
    pub dynamic: Vec<String>,
}

impl Metadata23 {
    /// Parse distribution metadata from metadata `MetadataError`
    pub fn parse(content: &[u8]) -> Result<Self, MetadataError> {
        let headers = Headers::parse(content)?;

        let metadata_version = headers
            .get_first_value("Metadata-Version")
            .ok_or(MetadataError::FieldNotFound("Metadata-Version"))?;
        let name = headers
            .get_first_value("Name")
            .ok_or(MetadataError::FieldNotFound("Name"))?;
        let version = headers
            .get_first_value("Version")
            .ok_or(MetadataError::FieldNotFound("Version"))?;
        let platforms = headers.get_all_values("Platform").collect();
        let supported_platforms = headers.get_all_values("Supported-Platform").collect();
        let summary = headers.get_first_value("Summary");
        let body = str::from_utf8(&content[headers.body_start..])
            .map_err(MetadataError::DescriptionEncoding)?;
        let description = if body.trim().is_empty() {
            headers.get_first_value("Description")
        } else {
            Some(body.to_string())
        };
        let keywords = headers.get_first_value("Keywords");
        let home_page = headers.get_first_value("Home-Page");
        let download_url = headers.get_first_value("Download-URL");
        let author = headers.get_first_value("Author");
        let author_email = headers.get_first_value("Author-email");
        let license = headers.get_first_value("License");
        let license_expression = headers.get_first_value("License-Expression");
        let license_files = headers.get_all_values("License-File").collect();
        let classifiers = headers.get_all_values("Classifier").collect();
        let requires_dist = headers.get_all_values("Requires-Dist").collect();
        let provides_dist = headers.get_all_values("Provides-Dist").collect();
        let obsoletes_dist = headers.get_all_values("Obsoletes-Dist").collect();
        let maintainer = headers.get_first_value("Maintainer");
        let maintainer_email = headers.get_first_value("Maintainer-email");
        let requires_python = headers.get_first_value("Requires-Python");
        let requires_external = headers.get_all_values("Requires-External").collect();
        let project_urls = headers.get_all_values("Project-URL").collect();
        let provides_extras = headers.get_all_values("Provides-Extra").collect();
        let description_content_type = headers.get_first_value("Description-Content-Type");
        let dynamic = headers.get_all_values("Dynamic").collect();
        Ok(Metadata23 {
            metadata_version,
            name,
            version,
            platforms,
            supported_platforms,
            summary,
            description,
            keywords,
            home_page,
            download_url,
            author,
            author_email,
            license,
            license_expression,
            license_files,
            classifiers,
            requires_dist,
            provides_dist,
            obsoletes_dist,
            maintainer,
            maintainer_email,
            requires_python,
            requires_external,
            project_urls,
            provides_extras,
            description_content_type,
            dynamic,
        })
    }
}

impl FromStr for Metadata23 {
    type Err = MetadataError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Metadata23::parse(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetadataError;

    #[test]
    fn test_parse_from_str() {
        let s = "Metadata-Version: 1.0";
        let meta: Result<Metadata23, MetadataError> = s.parse();
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Name"))));

        let s = "Metadata-Version: 1.0\nName: asdf";
        let meta = Metadata23::parse(s.as_bytes());
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
        let meta = Metadata23::parse(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, "asdf");
        assert_eq!(meta.version, "1.0");

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nDescription: a Python package";
        let meta: Metadata23 = s.parse().unwrap();
        assert_eq!(meta.description.as_deref(), Some("a Python package"));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\n\na Python package";
        let meta: Metadata23 = s.parse().unwrap();
        assert_eq!(meta.description.as_deref(), Some("a Python package"));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
        let meta: Metadata23 = s.parse().unwrap();
        assert_eq!(meta.author.as_deref(), Some("中文"));
        assert_eq!(meta.description.as_deref(), Some("一个 Python 包"));
    }
}
