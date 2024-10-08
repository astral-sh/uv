use crate::pep639_glob::parse_pep639_glob;
use crate::Error;
use itertools::Itertools;
use serde::Deserialize;
use std::collections::{BTreeMap, Bound};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::debug;
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{Requirement, VersionOrUrl};
use uv_pubgrub::PubGrubSpecifier;
use uv_pypi_types::{Metadata23, VerbatimParsedUrl};
use uv_warnings::warn_user_once;

#[derive(Debug, Error)]
pub enum ValidationError {
    /// The spec isn't clear about what the values in that field would be, and we only support the
    /// default value (UTF-8).
    #[error("Charsets other than UTF-8 are not supported. Please convert your README to UTF-8 and remove `project.readme.charset`.")]
    ReadmeCharset,
    #[error("Unknown Readme extension `{0}`, can't determine content type. Please use a support extension (`.md`, `.rst`, `.txt`) or set the content type manually.")]
    UnknownExtension(String),
    #[error("Can't infer content type because `{}` does not have an extension. Please use a support extension (`.md`, `.rst`, `.txt`) or set the content type manually.", _0.user_display())]
    MissingExtension(PathBuf),
    #[error("Unsupported content type: `{0}`")]
    UnsupportedContentType(String),
    #[error("`project.description` must be a single line")]
    DescriptionNewlines,
    #[error("Dynamic metadata is not supported")]
    Dynamic,
    #[error("When `project.license-files` is defined, `project.license` must be an SPDX expression string")]
    MixedLicenseGenerations,
    #[error("Entrypoint groups must consist of letters and numbers separated by dots, invalid group: `{0}`")]
    InvalidGroup(String),
    #[error(
        "Entrypoint names must consist of letters, numbers, dots and dashes; invalid name: `{0}`"
    )]
    InvalidName(String),
    #[error("Use `project.scripts` instead of `project.entry-points.console_scripts`")]
    ReservedScripts,
    #[error("Use `project.gui-scripts` instead of `project.entry-points.gui_scripts`")]
    ReservedGuiScripts,
    #[error("`project.license` is not a valid SPDX expression: `{0}`")]
    InvalidSpdx(String, #[source] spdx::error::ParseError),
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Clone)]
#[serde(
    rename_all = "kebab-case",
    expecting = "The project table needs to follow \
    https://packaging.python.org/en/latest/guides/writing-pyproject-toml"
)]
pub(crate) struct PyProjectToml {
    /// Project metadata
    project: Project,
    /// Build-related data
    build_system: BuildSystem,
}

impl PyProjectToml {
    pub(crate) fn name(&self) -> &PackageName {
        &self.project.name
    }

    pub(crate) fn version(&self) -> &Version {
        &self.project.version
    }

    pub(crate) fn parse(contents: &str) -> Result<Self, Error> {
        Ok(toml::from_str(contents)?)
    }

    /// Warn if the `[build-system]` table looks suspicious.
    ///
    /// Example of a valid table:
    ///
    /// ```toml
    /// [build-system]
    /// requires = ["uv>=0.4.15,<5"]
    /// build-backend = "uv"
    /// ```
    ///
    /// Returns whether all checks passed.
    pub(crate) fn check_build_system(&self) -> bool {
        let mut passed = true;
        if self.build_system.build_backend.as_deref() != Some("uv") {
            warn_user_once!(
                r#"The value for `build_system.build-backend` should be `"uv"`, not `"{}"`"#,
                self.build_system.build_backend.clone().unwrap_or_default()
            );
            passed = false;
        }

        let uv_version = Version::from_str(uv_version::version())
            .expect("uv's own version is not PEP 440 compliant");
        let next_minor = uv_version.release().get(1).copied().unwrap_or_default() + 1;
        let next_breaking = Version::new([0, next_minor]);

        let expected = || {
            format!(
                "Expected a single uv requirement in `build-system.requires`, found `{}`",
                toml::to_string(&self.build_system.requires).unwrap_or_default()
            )
        };

        let [uv_requirement] = &self.build_system.requires.as_slice() else {
            warn_user_once!("{}", expected());
            return false;
        };
        if uv_requirement.name.as_str() != "uv" {
            warn_user_once!("{}", expected());
            return false;
        }
        let bounded = match &uv_requirement.version_or_url {
            None => false,
            Some(VersionOrUrl::Url(_)) => {
                // We can't validate the url
                true
            }
            Some(VersionOrUrl::VersionSpecifier(specifier)) => {
                // We don't check how wide the range is (that's up to the user), we just
                // check that the current version is compliant, to avoid accidentally using a
                // too new or too old uv, and we check that an upper bound exists. The latter
                // is very important to allow making breaking changes in uv without breaking
                // the existing immutable source distributions on pypi.
                if !specifier.contains(&uv_version) {
                    // This is allowed to happen when testing prereleases, but we should still warn.
                    warn_user_once!(
                        r#"`build_system.requires = ["{uv_requirement}"]` does not contain the
                        current uv version {}"#,
                        uv_version::version()
                    );
                    passed = false;
                }
                PubGrubSpecifier::from_pep440_specifiers(specifier)
                    .ok()
                    .and_then(|specifier| Some(specifier.bounding_range()?.1 != Bound::Unbounded))
                    .unwrap_or(false)
            }
        };

        if !bounded {
            warn_user_once!(
                r#"`build_system.requires = ["{uv_requirement}"]` is missing an
                upper bound on the uv version such as `<{next_breaking}`.
                Without bounding the uv version, the source distribution will break
                when a future, breaking version of uv is released."#,
            );
            passed = false;
        }

        passed
    }

    /// Validate and convert a `pyproject.toml` to core metadata.
    ///
    /// <https://packaging.python.org/en/latest/guides/writing-pyproject-toml/>
    /// <https://packaging.python.org/en/latest/specifications/pyproject-toml/>
    /// <https://packaging.python.org/en/latest/specifications/core-metadata/>
    pub(crate) fn to_metadata(&self, root: &Path) -> Result<Metadata23, Error> {
        let summary = if let Some(description) = &self.project.description {
            if description.contains('\n') {
                return Err(ValidationError::DescriptionNewlines.into());
            }
            Some(description.clone())
        } else {
            None
        };

        let supported_content_types = ["text/plain", "text/x-rst", "text/markdown"];
        let (description, description_content_type) = match &self.project.readme {
            Some(Readme::String(path)) => {
                let content = fs_err::read_to_string(root.join(path))?;
                let content_type = match path.extension().and_then(OsStr::to_str) {
                    Some("txt") => "text/plain",
                    Some("rst") => "text/x-rst",
                    Some("md") => "text/markdown",
                    Some(unknown) => {
                        return Err(ValidationError::UnknownExtension(unknown.to_owned()).into())
                    }
                    None => return Err(ValidationError::MissingExtension(path.clone()).into()),
                }
                .to_string();
                (Some(content), Some(content_type))
            }
            Some(Readme::File {
                file,
                content_type,
                charset,
            }) => {
                let content = fs_err::read_to_string(root.join(file))?;
                if !supported_content_types.contains(&content_type.as_str()) {
                    return Err(
                        ValidationError::UnsupportedContentType(content_type.clone()).into(),
                    );
                }
                if charset.as_ref().is_some_and(|charset| charset != "UTF-8") {
                    return Err(ValidationError::ReadmeCharset.into());
                }
                (Some(content), Some(content_type.clone()))
            }
            Some(Readme::Text {
                text,
                content_type,
                charset,
            }) => {
                if !supported_content_types.contains(&content_type.as_str()) {
                    return Err(
                        ValidationError::UnsupportedContentType(content_type.clone()).into(),
                    );
                }
                if charset.as_ref().is_some_and(|charset| charset != "UTF-8") {
                    return Err(ValidationError::ReadmeCharset.into());
                }
                (Some(text.clone()), Some(content_type.clone()))
            }
            None => (None, None),
        };

        if self
            .project
            .dynamic
            .as_ref()
            .is_some_and(|dynamic| !dynamic.is_empty())
        {
            return Err(ValidationError::Dynamic.into());
        }

        let author = self
            .project
            .authors
            .as_ref()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(|author| match author {
                        Contact::Name { name } => Some(name),
                        Contact::Email { .. } => None,
                        Contact::NameEmail { name, .. } => Some(name),
                    })
                    .join(", ")
            })
            .filter(|author| !author.is_empty());
        let author_email = self
            .project
            .authors
            .as_ref()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(|author| match author {
                        Contact::Name { .. } => None,
                        Contact::Email { email } => Some(email.clone()),
                        Contact::NameEmail { name, email } => Some(format!("{name} <{email}>")),
                    })
                    .join(", ")
            })
            .filter(|author_email| !author_email.is_empty());
        let maintainer = self
            .project
            .maintainers
            .as_ref()
            .map(|maintainers| {
                maintainers
                    .iter()
                    .filter_map(|maintainer| match maintainer {
                        Contact::Name { name } => Some(name),
                        Contact::Email { .. } => None,
                        Contact::NameEmail { name, .. } => Some(name),
                    })
                    .join(", ")
            })
            .filter(|maintainer| !maintainer.is_empty());
        let maintainer_email = self
            .project
            .maintainers
            .as_ref()
            .map(|maintainers| {
                maintainers
                    .iter()
                    .filter_map(|maintainer| match maintainer {
                        Contact::Name { .. } => None,
                        Contact::Email { email } => Some(email.clone()),
                        Contact::NameEmail { name, email } => Some(format!("{name} <{email}>")),
                    })
                    .join(", ")
            })
            .filter(|maintainer_email| !maintainer_email.is_empty());

        // Using PEP 639 bumps the METADATA version
        let metadata_version = if self.project.license_files.is_some()
            || matches!(self.project.license, Some(License::Spdx(_)))
        {
            debug!("Found PEP 639 license declarations, using METADATA 2.4");
            "2.4"
        } else {
            "2.3"
        };

        // TODO(konsti): Issue a warning on old license metadata once PEP 639 is universal.
        let (license, license_expression, license_files) =
            if let Some(license_globs) = &self.project.license_files {
                let license_expression = match &self.project.license {
                    None => None,
                    Some(License::Spdx(license_expression)) => Some(license_expression.clone()),
                    Some(License::Text { .. } | License::File { .. }) => {
                        return Err(ValidationError::MixedLicenseGenerations.into())
                    }
                };

                let mut license_files = Vec::new();
                for license_glob in license_globs {
                    let pep639_glob = parse_pep639_glob(license_glob)
                        .map_err(|err| Error::Pep639Glob(license_glob.to_string(), err))?;
                    let absolute_glob = PathBuf::from(glob::Pattern::escape(
                        root.simplified().to_string_lossy().as_ref(),
                    ))
                    .join(pep639_glob.to_string())
                    .to_string_lossy()
                    .to_string();
                    for license_file in glob::glob(&absolute_glob)
                        .map_err(|err| Error::Pattern(absolute_glob.to_string(), err))?
                    {
                        let license_file = license_file
                            .map_err(Error::Glob)?
                            .to_string_lossy()
                            .to_string();
                        if !license_files.contains(&license_file) {
                            license_files.push(license_file);
                        }
                    }
                }
                // The glob order may be unstable
                license_files.sort();

                (None, license_expression, license_files)
            } else {
                match &self.project.license {
                    None => (None, None, Vec::new()),
                    Some(License::Spdx(license_expression)) => {
                        (None, Some(license_expression.clone()), Vec::new())
                    }
                    Some(License::Text { text }) => (Some(text.clone()), None, Vec::new()),
                    Some(License::File { file }) => {
                        let text = fs_err::read_to_string(root.join(file))?;
                        (Some(text), None, Vec::new())
                    }
                }
            };

        // Check that the license expression is a valid SPDX identifier.
        if let Some(license_expression) = &license_expression {
            if let Err(err) = spdx::Expression::parse(license_expression) {
                return Err(ValidationError::InvalidSpdx(license_expression.clone(), err).into());
            }
        }

        // TODO(konsti): https://peps.python.org/pep-0753/#label-normalization (Draft)
        let project_urls = self
            .project
            .urls
            .iter()
            .flatten()
            .map(|(key, value)| format!("{key}, {value}"))
            .collect();

        let extras = self
            .project
            .optional_dependencies
            .iter()
            .flat_map(|optional_dependencies| optional_dependencies.keys())
            .map(ToString::to_string)
            .collect();

        Ok(Metadata23 {
            metadata_version: metadata_version.to_string(),
            name: self.project.name.to_string(),
            version: self.project.version.to_string(),
            // Not supported.
            platforms: vec![],
            // Not supported.
            supported_platforms: vec![],
            summary,
            description,
            description_content_type,
            keywords: self
                .project
                .keywords
                .as_ref()
                .map(|keywords| keywords.join(",")),
            home_page: None,
            download_url: None,
            author,
            author_email,
            maintainer,
            maintainer_email,
            license,
            license_expression,
            license_files,
            classifiers: self.project.classifiers.clone().unwrap_or_default(),
            requires_dist: self
                .project
                .dependencies
                .iter()
                .flatten()
                .map(ToString::to_string)
                .collect(),
            // Not commonly set.
            provides_dist: vec![],
            // Not supported.
            obsoletes_dist: vec![],
            requires_python: self
                .project
                .requires_python
                .as_ref()
                .map(ToString::to_string),
            // Not used by other tools, not supported.
            requires_external: vec![],
            project_urls,
            provides_extras: extras,
            dynamic: vec![],
        })
    }

    /// Validate and convert the entrypoints in `pyproject.toml`, including console and GUI scripts,
    /// to an `entry_points.txt`.
    ///
    /// <https://packaging.python.org/en/latest/specifications/entry-points/>
    ///
    /// Returns `None` if no entrypoints were defined.
    pub(crate) fn to_entry_points(&self) -> Result<Option<String>, ValidationError> {
        let mut writer = String::new();

        if self.project.scripts.is_none()
            && self.project.gui_scripts.is_none()
            && self.project.entry_points.is_none()
        {
            return Ok(None);
        }

        if let Some(scripts) = &self.project.scripts {
            Self::write_group(&mut writer, "console_scripts", scripts)?;
        }
        if let Some(gui_scripts) = &self.project.gui_scripts {
            Self::write_group(&mut writer, "gui_scripts", gui_scripts)?;
        }
        for (group, entries) in self.project.entry_points.iter().flatten() {
            if group == "console_scripts" {
                return Err(ValidationError::ReservedScripts);
            }
            if group == "gui_scripts" {
                return Err(ValidationError::ReservedGuiScripts);
            }
            Self::write_group(&mut writer, group, entries)?;
        }
        Ok(Some(writer))
    }

    /// Write a group to `entry_points.txt`.
    fn write_group<'a>(
        writer: &mut String,
        group: &str,
        entries: impl IntoIterator<Item = (&'a String, &'a String)>,
    ) -> Result<(), ValidationError> {
        if !group
            .chars()
            .next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false)
            || !group
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
        {
            return Err(ValidationError::InvalidGroup(group.to_string()));
        }

        writer.push_str(&format!("[{group}]\n"));
        for (name, object_reference) in entries {
            // More strict than the spec, we enforce the recommendation
            if !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            {
                return Err(ValidationError::InvalidName(name.to_string()));
            }

            // TODO(konsti): Validate that the object references are valid Python identifiers.
            writer.push_str(&format!("{name} = {object_reference}\n"));
        }
        writer.push('\n');
        Ok(())
    }
}

/// The `[project]` section of a pyproject.toml as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
///
/// This struct does not have schema export; the schema is shared between all Python tools, and we
/// should update the shared schema instead.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
struct Project {
    /// The name of the project.
    name: PackageName,
    /// The version of the project.
    version: Version,
    /// The summary description of the project in one line.
    description: Option<String>,
    /// The full description of the project (i.e. the README).
    readme: Option<Readme>,
    /// The Python version requirements of the project.
    requires_python: Option<VersionSpecifiers>,
    /// The license under which the project is distributed.
    ///
    /// Supports both the current standard and the provisional PEP 639.
    license: Option<License>,
    /// The paths to files containing licenses and other legal notices to be distributed with the
    /// project.
    ///
    /// From the provisional PEP 639
    license_files: Option<Vec<String>>,
    /// The people or organizations considered to be the "authors" of the project.
    authors: Option<Vec<Contact>>,
    /// The people or organizations considered to be the "maintainers" of the project.
    maintainers: Option<Vec<Contact>>,
    /// The keywords for the project.
    keywords: Option<Vec<String>>,
    /// Trove classifiers which apply to the project.
    classifiers: Option<Vec<String>>,
    /// A table of URLs where the key is the URL label and the value is the URL itself.
    ///
    /// PyPI shows all URLs with their name. For some known patterns, they add favicons.
    /// main: <https://github.com/pypi/warehouse/blob/main/warehouse/templates/packaging/detail.html>
    /// archived: <https://github.com/pypi/warehouse/blob/e3bd3c3805ff47fff32b67a899c1ce11c16f3c31/warehouse/templates/packaging/detail.html>
    urls: Option<BTreeMap<String, String>>,
    /// The console entrypoints of the project.
    ///
    /// The key of the table is the name of the entry point and the value is the object reference.
    scripts: Option<BTreeMap<String, String>>,
    /// The GUI entrypoints of the project.
    ///
    /// The key of the table is the name of the entry point and the value is the object reference.
    gui_scripts: Option<BTreeMap<String, String>>,
    /// Entrypoints groups of the project.
    ///
    /// The key of the table is the name of the entry point and the value is the object reference.
    entry_points: Option<BTreeMap<String, BTreeMap<String, String>>>,
    /// The dependencies of the project.
    dependencies: Option<Vec<Requirement>>,
    /// The optional dependencies of the project.
    optional_dependencies: Option<BTreeMap<ExtraName, Vec<Requirement>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified so another tool
    /// can/will provide such metadata dynamically.
    ///
    /// Not supported, an error if anything but the default empty list.
    dynamic: Option<Vec<String>>,
}

/// The optional `project.readme` key in a pyproject.toml as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml/#readme>.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged, rename_all = "kebab-case")]
enum Readme {
    /// Relative path to the README.
    String(PathBuf),
    /// Relative path to the README.
    File {
        file: PathBuf,
        content_type: String,
        charset: Option<String>,
    },
    /// The full description of the project as inline value.
    Text {
        text: String,
        content_type: String,
        charset: Option<String>,
    },
}

/// The optional `project.license` key in a pyproject.toml as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml/#license>.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum License {
    /// An SPDX Expression.
    ///
    /// From the provisional PEP 639.
    Spdx(String),
    Text {
        /// The full text of the license.
        text: String,
    },
    File {
        /// The file containing the license text.
        file: PathBuf,
    },
}

/// A `project.authors` or `project.maintainers` entry as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml/#authors-maintainers>.
///
/// The entry is derived from the email format of `John Doe <john.doe@example.net>`. You need to
/// provide at least name or email.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged, expecting = "a table with 'name' and/or 'email' keys")]
enum Contact {
    /// TODO(konsti): RFC 822 validation.
    Name { name: String },
    /// TODO(konsti): RFC 822 validation.
    Email { email: String },
    /// TODO(konsti): RFC 822 validation.
    NameEmail { name: String, email: String },
}

/// The `[build-system]` section of a pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct BuildSystem {
    /// PEP 508 dependencies required to execute the build system.
    requires: Vec<Requirement<VerbatimParsedUrl>>,
    /// A string naming a Python object that will be used to perform the build.
    build_backend: Option<String>,
    /// <https://peps.python.org/pep-0517/#in-tree-build-backends>
    backend_path: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::{formatdoc, indoc};
    use insta::assert_snapshot;
    use std::iter;
    use tempfile::TempDir;

    fn extend_project(payload: &str) -> String {
        formatdoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"
            {payload}

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "uv"
        "#
        }
    }

    fn format_err(err: impl std::error::Error) -> String {
        let mut formatted = err.to_string();
        for source in iter::successors(err.source(), |&err| err.source()) {
            formatted += &format!("\n  Caused by: {source}");
        }
        formatted
    }

    #[test]
    fn valid() {
        let temp_dir = TempDir::new().unwrap();

        fs_err::write(
            temp_dir.path().join("Readme.md"),
            indoc! {r"
            # Foo

            This is the foo library.
        "},
        )
        .unwrap();

        fs_err::write(
            temp_dir.path().join("License.txt"),
            indoc! {r#"
                THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
                INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
                PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
                HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
                CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
                OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
        "#},
        )
        .unwrap();

        let contents = indoc! {r#"
            # See https://github.com/pypa/sampleproject/blob/main/pyproject.toml for another example

            [project]
            name = "hello-world"
            version = "0.1.0"
            description = "A Python package"
            readme = "Readme.md"
            requires_python = ">=3.12"
            license = { file = "License.txt" }
            authors = [{ name = "Ferris the crab", email = "ferris@rustacean.net" }]
            maintainers = [{ name = "Konsti", email = "konstin@mailbox.org" }]
            keywords = ["demo", "example", "package"]
            classifiers = [
                "Development Status :: 6 - Mature",
                "License :: OSI Approved :: MIT License",
                # https://github.com/pypa/trove-classifiers/issues/17
                "License :: OSI Approved :: Apache Software License",
                "Programming Language :: Python",
            ]
            dependencies = ["flask>=3,<4", "sqlalchemy[asyncio]>=2.0.35,<3"]
            # We don't support dynamic fields, the default empty array is the only allowed value.
            dynamic = []

            [project.optional-dependencies]
            postgres = ["psycopg>=3.2.2,<4"]
            mysql = ["pymysql>=1.1.1,<2"]

            [project.urls]
            "Homepage" = "https://github.com/astral-sh/uv"
            "Repository" = "https://astral.sh"

            [project.scripts]
            foo = "foo.cli:__main__"

            [project.gui-scripts]
            foo-gui = "foo.gui"

            [project.entry-points.bar_group]
            foo-bar = "foo:bar"

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "uv"
        "#
        };

        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        let metadata = pyproject_toml.to_metadata(temp_dir.path()).unwrap();

        assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.3
        Name: hello-world
        Version: 0.1.0
        Summary: A Python package
        Keywords: demo,example,package
        Author: Ferris the crab
        License: THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
                 INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
                 PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
                 HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
                 CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
                 OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
        Classifier: Development Status :: 6 - Mature
        Classifier: License :: OSI Approved :: MIT License
        Classifier: License :: OSI Approved :: Apache Software License
        Classifier: Programming Language :: Python
        Requires-Dist: flask>=3,<4
        Requires-Dist: sqlalchemy[asyncio]>=2.0.35,<3
        Maintainer: Konsti
        Project-URL: Homepage, https://github.com/astral-sh/uv
        Project-URL: Repository, https://astral.sh
        Provides-Extra: mysql
        Provides-Extra: postgres
        Description-Content-Type: text/markdown

        # Foo

        This is the foo library.
        "###);

        assert_snapshot!(pyproject_toml.to_entry_points().unwrap().unwrap(), @r###"
        [console_scripts]
        foo = foo.cli:__main__

        [gui_scripts]
        foo-gui = foo.gui

        [bar_group]
        foo-bar = foo:bar

        "###);
    }

    #[test]
    fn build_system_valid() {
        let contents = extend_project("");
        let pyproject_toml = PyProjectToml::parse(&contents).unwrap();
        assert!(pyproject_toml.check_build_system());
    }

    #[test]
    fn build_system_no_bound() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv"]
            build-backend = "uv"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert!(!pyproject_toml.check_build_system());
    }

    #[test]
    fn build_system_multiple_packages() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv>=0.4.15,<5", "wheel"]
            build-backend = "uv"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert!(!pyproject_toml.check_build_system());
    }

    #[test]
    fn build_system_no_requires_uv() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["setuptools"]
            build-backend = "uv"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert!(!pyproject_toml.check_build_system());
    }

    #[test]
    fn build_system_not_uv() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv>=0.4.15,<5"]
            build-backend = "setuptools"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert!(!pyproject_toml.check_build_system());
    }

    #[test]
    fn minimal() {
        let contents = extend_project("");

        let metadata = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap();

        assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.3
        Name: hello-world
        Version: 0.1.0
        "###);
    }

    #[test]
    fn invalid_readme_spec() {
        let contents = extend_project(indoc! {r#"
            readme = { path = "Readme.md" }
        "#
        });

        let err = PyProjectToml::parse(&contents).unwrap_err();
        assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: TOML parse error at line 4, column 10
          |
        4 | readme = { path = "Readme.md" }
          |          ^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Readme
        "###);
    }

    #[test]
    fn missing_readme() {
        let contents = extend_project(indoc! {r#"
            readme = "Readme.md"
        "#
        });

        let err = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap_err();
        // Simplified for windows compatibility.
        assert_snapshot!(err.to_string().replace('\\', "/"), @"failed to open file `/do/not/read/Readme.md`");
    }

    #[test]
    fn multiline_description() {
        let contents = extend_project(indoc! {r#"
            description = "Hi :)\nThis is my project"
        "#
        });

        let err = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap_err();
        assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: `project.description` must be a single line
        "###);
    }

    #[test]
    fn mixed_licenses() {
        let contents = extend_project(indoc! {r#"
            license-files = ["licenses/*"]
            license =  { text = "MIT" }
        "#
        });

        let err = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap_err();
        assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: When `project.license-files` is defined, `project.license` must be an SPDX expression string
        "###);
    }

    #[test]
    fn valid_license() {
        let contents = extend_project(indoc! {r#"
            license = "MIT OR Apache-2.0"
        "#
        });
        let metadata = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap();
        assert_snapshot!(metadata.core_metadata_format(), @r###"
        Metadata-Version: 2.4
        Name: hello-world
        Version: 0.1.0
        License-Expression: MIT OR Apache-2.0
        "###);
    }

    #[test]
    fn invalid_license() {
        let contents = extend_project(indoc! {r#"
            license = "MIT XOR Apache-2"
        "#
        });
        let err = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap_err();
        // TODO(konsti): We mess up the indentation in the error.
        assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: `project.license` is not a valid SPDX expression: `MIT XOR Apache-2`
          Caused by: MIT XOR Apache-2
            ^^^ unknown term
        "###);
    }

    #[test]
    fn dynamic() {
        let contents = extend_project(indoc! {r#"
            dynamic = ["dependencies"]
        "#
        });

        let err = PyProjectToml::parse(&contents)
            .unwrap()
            .to_metadata(Path::new("/do/not/read"))
            .unwrap_err();
        assert_snapshot!(format_err(err), @r###"
        Invalid pyproject.toml
          Caused by: Dynamic metadata is not supported
        "###);
    }

    fn script_error(contents: &str) -> String {
        let err = PyProjectToml::parse(contents)
            .unwrap()
            .to_entry_points()
            .unwrap_err();
        format_err(err)
    }

    #[test]
    fn invalid_entry_point_group() {
        let contents = extend_project(indoc! {r#"
            [project.entry-points."a@b"]
            foo = "bar"
        "#
        });
        assert_snapshot!(script_error(&contents), @"Entrypoint groups must consist of letters and numbers separated by dots, invalid group: `a@b`");
    }

    #[test]
    fn invalid_entry_point_name() {
        let contents = extend_project(indoc! {r#"
            [project.scripts]
            "a@b" = "bar"
        "#
        });
        assert_snapshot!(script_error(&contents), @"Entrypoint names must consist of letters, numbers, dots and dashes; invalid name: `a@b`");
    }

    #[test]
    fn invalid_entry_point_conflict_scripts() {
        let contents = extend_project(indoc! {r#"
            [project.entry-points.console_scripts]
            foo = "bar"
        "#
        });
        assert_snapshot!(script_error(&contents), @"Use `project.scripts` instead of `project.entry-points.console_scripts`");
    }

    #[test]
    fn invalid_entry_point_conflict_gui_scripts() {
        let contents = extend_project(indoc! {r#"
            [project.entry-points.gui_scripts]
            foo = "bar"
        "#
        });
        assert_snapshot!(script_error(&contents), @"Use `project.gui-scripts` instead of `project.entry-points.gui_scripts`");
    }
}
