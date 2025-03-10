use std::collections::{BTreeMap, Bound};
use std::ffi::OsStr;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use itertools::Itertools;
use serde::Deserialize;
use tracing::{debug, trace};
use version_ranges::Ranges;
use walkdir::WalkDir;

use uv_fs::Simplified;
use uv_globfilter::{parse_portable_glob, GlobDirFilter};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{
    ExtraOperator, MarkerExpression, MarkerTree, MarkerValueExtra, Requirement, VersionOrUrl,
};
use uv_pypi_types::{Identifier, Metadata23, VerbatimParsedUrl};

use crate::serde_verbatim::SerdeVerbatim;
use crate::Error;

/// By default, we ignore generated python files.
pub(crate) const DEFAULT_EXCLUDES: &[&str] = &["__pycache__", "*.pyc", "*.pyo"];

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
        "Entrypoint names must consist of letters, numbers, dots, underscores and dashes; invalid name: `{0}`"
    )]
    InvalidName(String),
    #[error("Use `project.scripts` instead of `project.entry-points.console_scripts`")]
    ReservedScripts,
    #[error("Use `project.gui-scripts` instead of `project.entry-points.gui_scripts`")]
    ReservedGuiScripts,
    #[error("`project.license` is not a valid SPDX expression: `{0}`")]
    InvalidSpdx(String, #[source] spdx::error::ParseError),
}

/// Check if the build backend is matching the currently running uv version.
pub fn check_direct_build(source_tree: &Path, name: impl Display) -> bool {
    let pyproject_toml: PyProjectToml =
        match fs_err::read_to_string(source_tree.join("pyproject.toml"))
            .map_err(|err| err.to_string())
            .and_then(|pyproject_toml| {
                toml::from_str(&pyproject_toml).map_err(|err| err.to_string())
            }) {
            Ok(pyproject_toml) => pyproject_toml,
            Err(err) => {
                debug!(
                    "Not using uv build backend direct build of {name}, no pyproject.toml: {err}"
                );
                return false;
            }
        };
    match pyproject_toml
        .check_build_system(uv_version::version())
        .as_slice()
    {
        // No warnings -> match
        [] => true,
        // Any warning -> no match
        [first, others @ ..] => {
            debug!(
                "Not using uv build backend direct build of {name}, pyproject.toml does not match: {first}"
            );
            for other in others {
                trace!("Further uv build backend direct build of {name} mismatch: {other}");
            }
            false
        }
    }
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Clone)]
#[serde(
    rename_all = "kebab-case",
    expecting = "The project table needs to follow \
    https://packaging.python.org/en/latest/guides/writing-pyproject-toml"
)]
pub struct PyProjectToml {
    /// Project metadata
    project: Project,
    /// uv-specific configuration
    tool: Option<Tool>,
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

    pub(crate) fn readme(&self) -> Option<&Readme> {
        self.project.readme.as_ref()
    }

    /// The license files that need to be included in the source distribution.
    pub(crate) fn license_files_source_dist(&self) -> impl Iterator<Item = &str> {
        let license_file = self
            .project
            .license
            .as_ref()
            .and_then(|license| license.file())
            .into_iter();
        let license_files = self
            .project
            .license_files
            .iter()
            .flatten()
            .map(String::as_str);
        license_files.chain(license_file)
    }

    /// The license files that need to be included in the wheel.
    pub(crate) fn license_files_wheel(&self) -> impl Iterator<Item = &str> {
        // The pre-PEP 639 `license = { file = "..." }` is included inline in `METADATA`.
        self.project
            .license_files
            .iter()
            .flatten()
            .map(String::as_str)
    }

    pub(crate) fn settings(&self) -> Option<&BuildBackendSettings> {
        self.tool.as_ref()?.uv.as_ref()?.build_backend.as_ref()
    }

    /// Returns user-facing warnings if the `[build-system]` table looks suspicious.
    ///
    /// Example of a valid table:
    ///
    /// ```toml
    /// [build-system]
    /// requires = ["uv_build>=0.4.15,<5"]
    /// build-backend = "uv_build"
    /// ```
    pub fn check_build_system(&self, uv_version: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.build_system.build_backend.as_deref() != Some("uv_build") {
            warnings.push(format!(
                r#"The value for `build_system.build-backend` should be `"uv_build"`, not `"{}"`"#,
                self.build_system.build_backend.clone().unwrap_or_default()
            ));
        }

        let uv_version =
            Version::from_str(uv_version).expect("uv's own version is not PEP 440 compliant");
        let next_minor = uv_version.release().get(1).copied().unwrap_or_default() + 1;
        let next_breaking = Version::new([0, next_minor]);

        let expected = || {
            format!(
                "Expected a single uv requirement in `build-system.requires`, found `{}`",
                toml::to_string(&self.build_system.requires).unwrap_or_default()
            )
        };

        let [uv_requirement] = &self.build_system.requires.as_slice() else {
            warnings.push(expected());
            return warnings;
        };
        if uv_requirement.name.as_str() != "uv-build" {
            warnings.push(expected());
            return warnings;
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
                    warnings.push(format!(
                        r#"`build_system.requires = ["{uv_requirement}"]` does not contain the
                        current uv version {uv_version}"#,
                    ));
                }
                Ranges::from(specifier.clone())
                    .bounding_range()
                    .map(|bounding_range| bounding_range.1 != Bound::Unbounded)
                    .unwrap_or(false)
            }
        };

        if !bounded {
            warnings.push(format!(
                "`build_system.requires = [\"{}\"]` is missing an \
                upper bound on the `uv_build` version such as `<{next_breaking}`. \
                Without bounding the `uv_build` version, the source distribution will break \
                when a future, breaking version of `uv_build` is released.",
                // Use an underscore consistently, to avoid confusing users between a package name with dash and a
                // module name with underscore
                uv_requirement.verbatim()
            ));
        }

        warnings
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
                let mut license_globs_parsed = Vec::new();
                for license_glob in license_globs {
                    let pep639_glob =
                        parse_portable_glob(license_glob).map_err(|err| Error::PortableGlob {
                            field: license_glob.to_string(),
                            source: err,
                        })?;
                    license_globs_parsed.push(pep639_glob);
                }
                let license_globs =
                    GlobDirFilter::from_globs(&license_globs_parsed).map_err(|err| {
                        Error::GlobSetTooLarge {
                            field: "tool.uv.build-backend.source-include".to_string(),
                            source: err,
                        }
                    })?;

                for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
                    license_globs.match_directory(
                        entry
                            .path()
                            .strip_prefix(root)
                            .expect("walkdir starts with root"),
                    )
                }) {
                    let entry = entry.map_err(|err| Error::WalkDir {
                        root: root.to_path_buf(),
                        err,
                    })?;
                    let relative = entry
                        .path()
                        .strip_prefix(root)
                        .expect("walkdir starts with root");
                    if !license_globs.match_path(relative) {
                        trace!("Not a license files match: `{}`", relative.user_display());
                        continue;
                    }
                    if !entry.file_type().is_file() {
                        trace!(
                            "Not a file in license files match: `{}`",
                            relative.user_display()
                        );
                        continue;
                    }

                    debug!("License files match: `{}`", relative.user_display());
                    license_files.push(relative.portable_display().to_string());
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
            .collect::<Vec<_>>();

        let requires_dist =
            self.project
                .dependencies
                .iter()
                .flatten()
                .cloned()
                .chain(self.project.optional_dependencies.iter().flat_map(
                    |optional_dependencies| {
                        optional_dependencies
                            .iter()
                            .flat_map(|(extra, requirements)| {
                                requirements.iter().cloned().map(|mut requirement| {
                                    requirement.marker.and(MarkerTree::expression(
                                        MarkerExpression::Extra {
                                            operator: ExtraOperator::Equal,
                                            name: MarkerValueExtra::Extra(extra.clone()),
                                        },
                                    ));
                                    requirement
                                })
                            })
                    },
                ))
                .collect::<Vec<_>>();

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
            requires_dist: requires_dist.iter().map(ToString::to_string).collect(),
            provides_extras: extras.iter().map(ToString::to_string).collect(),
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
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
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
pub(crate) enum Readme {
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

impl Readme {
    /// If the readme is a file, return the path to the file.
    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Readme::String(path) => Some(path),
            Readme::File { file, .. } => Some(file),
            Readme::Text { .. } => None,
        }
    }
}

/// The optional `project.license` key in a pyproject.toml as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml/#license>.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum License {
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
        file: String,
    },
}

impl License {
    fn file(&self) -> Option<&str> {
        if let Self::File { file } = self {
            Some(file)
        } else {
            None
        }
    }
}

/// A `project.authors` or `project.maintainers` entry as specified in
/// <https://packaging.python.org/en/latest/specifications/pyproject-toml/#authors-maintainers>.
///
/// The entry is derived from the email format of `John Doe <john.doe@example.net>`. You need to
/// provide at least name or email.
#[derive(Deserialize, Debug, Clone)]
// deny_unknown_fields prevents using the name field when the email is not a string.
#[serde(
    untagged,
    deny_unknown_fields,
    expecting = "a table with 'name' and/or 'email' keys"
)]
pub(crate) enum Contact {
    /// TODO(konsti): RFC 822 validation.
    NameEmail { name: String, email: String },
    /// TODO(konsti): RFC 822 validation.
    Name { name: String },
    /// TODO(konsti): RFC 822 validation.
    Email { email: String },
}

/// The `[build-system]` section of a pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct BuildSystem {
    /// PEP 508 dependencies required to execute the build system.
    requires: Vec<SerdeVerbatim<Requirement<VerbatimParsedUrl>>>,
    /// A string naming a Python object that will be used to perform the build.
    build_backend: Option<String>,
    /// <https://peps.python.org/pep-0517/#in-tree-build-backends>
    backend_path: Option<Vec<String>>,
}

/// The `tool` section as specified in PEP 517.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Tool {
    /// uv-specific configuration
    uv: Option<ToolUv>,
}

/// The `tool.uv` section with build configuration.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ToolUv {
    /// Configuration for building source distributions and wheels with the uv build backend
    build_backend: Option<BuildBackendSettings>,
}

/// To select which files to include in the source distribution, we first add the includes, then
/// remove the excludes from that.
///
/// ## Include and exclude configuration
///
/// When building the source distribution, the following files and directories are included:
/// * `pyproject.toml`
/// * The module under `tool.uv.build-backend.module-root`, by default
///   `src/<module-name or project_name_with_underscores>/**`.
/// * `project.license-files` and `project.readme`.
/// * All directories under `tool.uv.build-backend.data`.
/// * All patterns from `tool.uv.build-backend.source-include`.
///
/// From these, we remove the `tool.uv.build-backend.source-exclude` matches.
///
/// When building the wheel, the following files and directories are included:
/// * The module under `tool.uv.build-backend.module-root`, by default
///   `src/<module-name or project_name_with_underscores>/**`.
/// * `project.license-files` and `project.readme`, as part of the project metadata.
/// * Each directory under `tool.uv.build-backend.data`, as data directories.
///
/// From these, we remove the `tool.uv.build-backend.source-exclude` and
/// `tool.uv.build-backend.wheel-exclude` matches. The source dist excludes are applied to avoid
/// source tree -> wheel source including more files than
/// source tree -> source distribution -> wheel.
///
/// There are no specific wheel includes. There must only be one top level module, and all data
/// files must either be under the module root or in a data directory. Most packages store small
/// data in the module root alongside the source code.
///
/// ## Include and exclude syntax
///
/// Includes are anchored, which means that `pyproject.toml` includes only
/// `<project root>/pyproject.toml`. Use for example `assets/**/sample.csv` to include for all
/// `sample.csv` files in `<project root>/assets` or any child directory. To recursively include
/// all files under a directory, use a `/**` suffix, e.g. `src/**`. For performance and
/// reproducibility, avoid unanchored matches such as `**/sample.csv`.
///
/// Excludes are not anchored, which means that `__pycache__` excludes all directories named
/// `__pycache__` and it's children anywhere. To anchor a directory, use a `/` prefix, e.g.,
/// `/dist` will exclude only `<project root>/dist`.
///
/// The glob syntax is the reduced portable glob from
/// [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).
#[derive(Deserialize, Debug, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub(crate) struct BuildBackendSettings {
    /// The directory that contains the module directory, usually `src`, or an empty path when
    /// using the flat layout over the src layout.
    pub(crate) module_root: PathBuf,

    /// The name of the module directory inside `module-root`.
    ///
    /// The default module name is the package name with dots and dashes replaced by underscores.
    ///
    /// Note that using this option runs the risk of creating two packages with different names but
    /// the same module names. Installing such packages together leads to unspecified behavior,
    /// often with corrupted files or directory trees.
    pub(crate) module_name: Option<Identifier>,

    /// Glob expressions which files and directories to additionally include in the source
    /// distribution.
    ///
    /// `pyproject.toml` and the contents of the module directory are always included.
    ///
    /// The glob syntax is the reduced portable glob from
    /// [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).
    pub(crate) source_include: Vec<String>,

    /// If set to `false`, the default excludes aren't applied.
    ///
    /// Default excludes: `__pycache__`, `*.pyc`, and `*.pyo`.
    pub(crate) default_excludes: bool,

    /// Glob expressions which files and directories to exclude from the source distribution.
    pub(crate) source_exclude: Vec<String>,

    /// Glob expressions which files and directories to exclude from the wheel.
    pub(crate) wheel_exclude: Vec<String>,

    /// Data includes for wheels.
    ///
    /// The directories included here are also included in the source distribution. They are copied
    /// to the right wheel subdirectory on build.
    pub(crate) data: WheelDataIncludes,
}

impl Default for BuildBackendSettings {
    fn default() -> Self {
        Self {
            module_root: PathBuf::from("src"),
            module_name: None,
            source_include: Vec::new(),
            default_excludes: true,
            source_exclude: Vec::new(),
            wheel_exclude: Vec::new(),
            data: WheelDataIncludes::default(),
        }
    }
}

/// Data includes for wheels.
///
/// Each entry is a directory, whose contents are copied to the matching directory in the wheel in
/// `<name>-<version>.data/(purelib|platlib|headers|scripts|data)`. Upon installation, this data
/// is moved to its target location, as defined by
/// <https://docs.python.org/3.12/library/sysconfig.html#installation-paths>:
/// - `data`: Installed over the virtualenv environment root. Warning: This may override existing
///   files!
/// - `scripts`: Installed to the directory for executables, `<venv>/bin` on Unix or
///   `<venv>\Scripts` on Windows. This directory is added to PATH when the virtual environment is
///   activated or when using `uv run`, so this data type can be used to install additional
///   binaries. Consider using `project.scripts` instead for starting Python code.
/// - `headers`: Installed to the include directory, where compilers building Python packages with
///   this package as built requirement will search for header files.
/// - `purelib` and `platlib`: Installed to the `site-packages` directory. It is not recommended to
///   uses these two options.
#[derive(Default, Deserialize, Debug, Clone)]
// `deny_unknown_fields` to catch typos such as `header` vs `headers`.
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct WheelDataIncludes {
    purelib: Option<String>,
    platlib: Option<String>,
    headers: Option<String>,
    scripts: Option<String>,
    data: Option<String>,
}

impl WheelDataIncludes {
    /// Yield all data directories name and corresponding paths.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("purelib", self.purelib.as_deref()),
            ("platlib", self.platlib.as_deref()),
            ("headers", self.headers.as_deref()),
            ("scripts", self.scripts.as_deref()),
            ("data", self.data.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
    }
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
            requires = ["uv_build>=0.4.15,<5"]
            build-backend = "uv_build"
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
            requires = ["uv_build>=0.4.15,<5"]
            build-backend = "uv_build"
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
        Author-email: Ferris the crab <ferris@rustacean.net>
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
        Requires-Dist: pymysql>=1.1.1,<2 ; extra == 'mysql'
        Requires-Dist: psycopg>=3.2.2,<4 ; extra == 'postgres'
        Maintainer: Konsti
        Maintainer-email: Konsti <konstin@mailbox.org>
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
    fn self_extras() {
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
            postgres = ["psycopg>=3.2.2,<4 ; sys_platform == 'linux'"]
            mysql = ["pymysql>=1.1.1,<2"]
            databases = ["hello-world[mysql]", "hello-world[postgres]"]
            all = ["hello-world[databases]", "hello-world[postgres]", "hello-world[mysql]"]

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
            requires = ["uv_build>=0.4.15,<5"]
            build-backend = "uv_build"
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
        Author-email: Ferris the crab <ferris@rustacean.net>
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
        Requires-Dist: hello-world[databases] ; extra == 'all'
        Requires-Dist: hello-world[postgres] ; extra == 'all'
        Requires-Dist: hello-world[mysql] ; extra == 'all'
        Requires-Dist: hello-world[mysql] ; extra == 'databases'
        Requires-Dist: hello-world[postgres] ; extra == 'databases'
        Requires-Dist: pymysql>=1.1.1,<2 ; extra == 'mysql'
        Requires-Dist: psycopg>=3.2.2,<4 ; sys_platform == 'linux' and extra == 'postgres'
        Maintainer: Konsti
        Maintainer-email: Konsti <konstin@mailbox.org>
        Project-URL: Homepage, https://github.com/astral-sh/uv
        Project-URL: Repository, https://astral.sh
        Provides-Extra: all
        Provides-Extra: databases
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
        assert_snapshot!(
            pyproject_toml.check_build_system("1.0.0+test").join("\n"),
            @""
        );
    }

    #[test]
    fn build_system_no_bound() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv_build"]
            build-backend = "uv_build"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert_snapshot!(
            pyproject_toml.check_build_system("0.4.15+test").join("\n"),
            @r###"`build_system.requires = ["uv_build"]` is missing an upper bound on the `uv_build` version such as `<0.5`. Without bounding the `uv_build` version, the source distribution will break when a future, breaking version of `uv_build` is released."###
        );
    }

    #[test]
    fn build_system_multiple_packages() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv_build>=0.4.15,<5", "wheel"]
            build-backend = "uv_build"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert_snapshot!(
            pyproject_toml.check_build_system("0.4.15+test").join("\n"),
            @"Expected a single uv requirement in `build-system.requires`, found ``"
        );
    }

    #[test]
    fn build_system_no_requires_uv() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["setuptools"]
            build-backend = "uv_build"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert_snapshot!(
            pyproject_toml.check_build_system("0.4.15+test").join("\n"),
            @"Expected a single uv requirement in `build-system.requires`, found ``"
        );
    }

    #[test]
    fn build_system_not_uv() {
        let contents = indoc! {r#"
            [project]
            name = "hello-world"
            version = "0.1.0"

            [build-system]
            requires = ["uv_build>=0.4.15,<5"]
            build-backend = "setuptools"
        "#};
        let pyproject_toml = PyProjectToml::parse(contents).unwrap();
        assert_snapshot!(
            pyproject_toml.check_build_system("0.4.15+test").join("\n"),
            @r###"The value for `build_system.build-backend` should be `"uv_build"`, not `"setuptools"`"###
        );
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
        // Strip away OS specific part.
        let err = err
            .to_string()
            .replace('\\', "/")
            .split_once(':')
            .unwrap()
            .0
            .to_string();
        assert_snapshot!(err, @"failed to open file `/do/not/read/Readme.md`");
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
        assert_snapshot!(script_error(&contents), @"Entrypoint names must consist of letters, numbers, dots, underscores and dashes; invalid name: `a@b`");
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
