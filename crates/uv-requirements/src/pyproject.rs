//! Reading from `pyproject.toml`
//! * `project.{dependencies,optional-dependencies}`,
//! * `tool.uv.sources` and
//! * `tool.uv.workspace`
//!
//! and lowering them into a dependency specification.

use std::collections::HashMap;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use glob::Pattern;
use indexmap::IndexMap;
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use distribution_types::{ParsedUrlError, Requirement, RequirementSource, Requirements};
use pep508_rs::{VerbatimUrl, VersionOrUrl};
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::{ExtraName, PackageName};

use crate::ExtrasSpecification;

#[derive(Debug, Error)]
pub enum Pep621Error {
    #[error(transparent)]
    Pep508(#[from] pep508_rs::Pep508Error),
    #[error("Must specify a `[project]` section alongside `[tool.uv.sources]`")]
    MissingProjectSection,
    #[error("pyproject.toml section is declared as dynamic, but must be static: `{0}`")]
    CantBeDynamic(&'static str),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] LoweringError),
}

/// An error parsing and merging `tool.uv.sources` with
/// `project.{dependencies,optional-dependencies}`.
#[derive(Debug, Error)]
pub enum LoweringError {
    #[error("Invalid URL structure")]
    DirectUrl(#[from] Box<ParsedUrlError>),
    #[error("Unsupported path (can't convert to URL): `{}`", _0.user_display())]
    PathToUrl(PathBuf),
    #[error("Package is not included as workspace package in `tool.uv.workspace`")]
    UndeclaredWorkspacePackage,
    #[error("Must specify a version constraint")]
    UnconstrainedVersion,
    #[error("Can only specify one of rev, tag, or branch")]
    MoreThanOneGitRef,
    #[error("Unable to combine options in `tool.uv.sources`")]
    InvalidEntry,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error("Can't combine URLs from both `project.dependencies` and `tool.uv.sources`")]
    ConflictingUrls,
    #[error("Could not normalize path: `{0}`")]
    AbsolutizeError(String, #[source] io::Error),
    #[error("Fragments are not allowed in URLs: `{0}`")]
    ForbiddenFragment(Url),
    #[error("`workspace = false` is not yet supported")]
    WorkspaceFalse,
    #[error("`tool.uv.sources` is a preview feature; use `--preview` or set `UV_PREVIEW=1` to enable it")]
    MissingPreview,
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Proprietary additions.
    pub tool: Option<Tool>,
}

/// PEP 621 project metadata (`project`).
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for extracting static requirements.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    /// The name of the project
    pub name: PackageName,
    /// Project dependencies
    pub dependencies: Option<Vec<String>>,
    /// Optional dependencies
    pub optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    pub dynamic: Option<Vec<String>>,
}

/// `tool`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    pub uv: Option<ToolUv>,
}

/// `tool.uv`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ToolUv {
    pub sources: Option<HashMap<PackageName, Source>>,
    pub workspace: Option<ToolUvWorkspace>,
}

/// `tool.uv.workspace`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ToolUvWorkspace {
    pub members: Option<Vec<SerdePattern>>,
    pub exclude: Option<Vec<SerdePattern>>,
}

/// (De)serialize globs as strings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SerdePattern(#[serde(with = "serde_from_and_to_string")] pub Pattern);

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SerdePattern {
    fn schema_name() -> String {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(gen)
    }
}

impl Deref for SerdePattern {
    type Target = Pattern;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A `tool.uv.sources` value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, deny_unknown_fields)]
pub enum Source {
    /// A remote Git repository, available over HTTPS or SSH.
    ///
    /// Example:
    /// ```toml
    /// flask = { git = "https://github.com/pallets/flask", tag = "3.0.0" }
    /// ```
    Git {
        /// The repository URL (without the `git+` prefix).
        git: Url,
        /// The path to the directory with the `pyproject.toml`, if it's not in the archive root.
        subdirectory: Option<String>,
        // Only one of the three may be used; we'll validate this later and emit a custom error.
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
    },
    /// A remote `http://` or `https://` URL, either a wheel (`.whl`) or a source distribution
    /// (`.zip`, `.tar.gz`).
    ///
    /// Example:
    /// ```toml
    /// flask = { url = "https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl" }
    /// ```
    Url {
        url: Url,
        /// For source distributions, the path to the directory with the `pyproject.toml`, if it's
        /// not in the archive root.
        subdirectory: Option<String>,
    },
    /// The path to a dependency, either a wheel (a `.whl` file), source distribution (a `.zip` or
    /// `.tag.gz` file), or source tree (i.e., a directory containing a `pyproject.toml` or
    /// `setup.py` file in the root).
    Path {
        path: String,
        /// `false` by default.
        editable: Option<bool>,
    },
    /// A dependency pinned to a specific index, e.g., `torch` after setting `torch` to `https://download.pytorch.org/whl/cu118`.
    Registry {
        // TODO(konstin): The string is more-or-less a placeholder
        index: String,
    },
    /// A dependency on another package in the workspace.
    Workspace {
        /// When set to `false`, the package will be fetched from the remote index, rather than
        /// included as a workspace package.
        workspace: bool,
        /// `true` by default.
        editable: Option<bool>,
    },
    /// A catch-all variant used to emit precise error messages when deserializing.
    CatchAll {
        git: String,
        subdirectory: Option<String>,
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
        url: String,
        patch: String,
        index: String,
        workspace: bool,
    },
}

/// The PEP 621 project metadata, with static requirements extracted in advance, joined
/// with `tool.uv.sources`.
#[derive(Debug)]
pub(crate) struct Pep621Metadata {
    /// The name of the project.
    pub(crate) name: PackageName,
    /// The requirements extracted from the project.
    pub(crate) requirements: Vec<Requirement>,
    /// The extras used to collect requirements.
    pub(crate) used_extras: FxHashSet<ExtraName>,
}

impl Pep621Metadata {
    /// Extract the static [`Pep621Metadata`] from a [`Project`] and [`ExtrasSpecification`], if
    /// possible.
    ///
    /// If the project specifies dynamic dependencies, or if the project specifies dynamic optional
    /// dependencies and the extras are requested, the requirements cannot be extracted.
    ///
    /// Returns an error if the requirements are not valid PEP 508 requirements.
    pub(crate) fn try_from(
        pyproject: PyProjectToml,
        extras: &ExtrasSpecification,
        project_dir: &Path,
        workspace_sources: &HashMap<PackageName, Source>,
        workspace_packages: &HashMap<PackageName, String>,
        preview: PreviewMode,
    ) -> Result<Option<Self>, Pep621Error> {
        let project_sources = pyproject
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.clone());

        let has_sources = project_sources.is_some() || !workspace_sources.is_empty();

        let Some(project) = pyproject.project else {
            return if has_sources {
                Err(Pep621Error::MissingProjectSection)
            } else {
                Ok(None)
            };
        };
        if let Some(dynamic) = project.dynamic.as_ref() {
            // If the project specifies dynamic dependencies, we can't extract the requirements.
            if dynamic.iter().any(|field| field == "dependencies") {
                return if has_sources {
                    Err(Pep621Error::CantBeDynamic("project.dependencies"))
                } else {
                    Ok(None)
                };
            }
            // If we requested extras, and the project specifies dynamic optional dependencies, we can't
            // extract the requirements.
            if !extras.is_empty() && dynamic.iter().any(|field| field == "optional-dependencies") {
                return if has_sources {
                    Err(Pep621Error::CantBeDynamic("project.optional-dependencies"))
                } else {
                    Ok(None)
                };
            }
        }

        let requirements = lower_requirements(
            &project.dependencies.unwrap_or_default(),
            &project.optional_dependencies.unwrap_or_default(),
            &project.name,
            project_dir,
            &project_sources.unwrap_or_default(),
            workspace_sources,
            workspace_packages,
            preview,
        )?;

        // Parse out the project requirements.
        let mut requirements_with_extras = requirements.dependencies;

        // Include any optional dependencies specified in `extras`.
        let mut used_extras = FxHashSet::default();
        if !extras.is_empty() {
            // Include the optional dependencies if the extras are requested.
            for (extra, optional_requirements) in &requirements.optional_dependencies {
                if extras.contains(extra) {
                    used_extras.insert(extra.clone());
                    requirements_with_extras.extend(flatten_extra(
                        &project.name,
                        optional_requirements,
                        &requirements.optional_dependencies,
                    ));
                }
            }
        }

        Ok(Some(Self {
            name: project.name,
            requirements: requirements_with_extras,
            used_extras,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_requirements(
    dependencies: &[String],
    optional_dependencies: &IndexMap<ExtraName, Vec<String>>,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &HashMap<PackageName, Source>,
    workspace_sources: &HashMap<PackageName, Source>,
    workspace_packages: &HashMap<PackageName, String>,
    preview: PreviewMode,
) -> Result<Requirements, Pep621Error> {
    let dependencies = dependencies
        .iter()
        .map(|dependency| {
            let requirement = pep508_rs::Requirement::from_str(dependency)?;
            let name = requirement.name.clone();
            lower_requirement(
                requirement,
                project_name,
                project_dir,
                project_sources,
                workspace_sources,
                workspace_packages,
                preview,
            )
            .map_err(|err| Pep621Error::LoweringError(name, err))
        })
        .collect::<Result<_, Pep621Error>>()?;
    let optional_dependencies = optional_dependencies
        .iter()
        .map(|(extra_name, dependencies)| {
            let dependencies: Vec<_> = dependencies
                .iter()
                .map(|dependency| {
                    let requirement = pep508_rs::Requirement::from_str(dependency)?;
                    let name = requirement.name.clone();
                    lower_requirement(
                        requirement,
                        project_name,
                        project_dir,
                        project_sources,
                        workspace_sources,
                        workspace_packages,
                        preview,
                    )
                    .map_err(|err| Pep621Error::LoweringError(name, err))
                })
                .collect::<Result<_, Pep621Error>>()?;
            Ok((extra_name.clone(), dependencies))
        })
        .collect::<Result<_, Pep621Error>>()?;
    Ok(Requirements {
        dependencies,
        optional_dependencies,
    })
}

/// Combine `project.dependencies` or `project.optional-dependencies` with `tool.uv.sources`.
pub(crate) fn lower_requirement(
    requirement: pep508_rs::Requirement,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &HashMap<PackageName, Source>,
    workspace_sources: &HashMap<PackageName, Source>,
    workspace_packages: &HashMap<PackageName, String>,
    preview: PreviewMode,
) -> Result<Requirement, LoweringError> {
    let source = project_sources
        .get(&requirement.name)
        .or(workspace_sources.get(&requirement.name))
        .cloned();
    if !matches!(
        source,
        Some(Source::Workspace {
            // By using toml, we technically support `workspace = false`.
            workspace: true,
            ..
        })
    ) && workspace_packages.contains_key(&requirement.name)
    {
        return Err(LoweringError::UndeclaredWorkspacePackage);
    }

    let Some(source) = source else {
        // Support recursive editable inclusions.
        // TODO(konsti): This is a workspace feature.
        return if requirement.version_or_url.is_none() && &requirement.name != project_name {
            Err(LoweringError::UnconstrainedVersion)
        } else {
            Ok(Requirement::from_pep508(requirement)?)
        };
    };

    if preview.is_disabled() {
        return Err(LoweringError::MissingPreview);
    }

    let source = match source {
        Source::Git {
            git,
            subdirectory,
            rev,
            tag,
            branch,
        } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            let reference = match (rev, tag, branch) {
                (None, None, None) => GitReference::DefaultBranch,
                (Some(rev), None, None) => {
                    if rev.starts_with("refs/") {
                        GitReference::NamedRef(rev.clone())
                    } else if rev.len() == 40 {
                        GitReference::FullCommit(rev.clone())
                    } else {
                        GitReference::ShortCommit(rev.clone())
                    }
                }
                (None, Some(tag), None) => GitReference::Tag(tag),
                (None, None, Some(branch)) => GitReference::Branch(branch),
                _ => return Err(LoweringError::MoreThanOneGitRef),
            };

            // Create a PEP 508-compatible URL.
            let mut url = Url::parse(&format!("git+{git}"))?;
            if let Some(rev) = reference.as_str() {
                url.set_path(&format!("{}@{}", url.path(), rev));
            }
            if let Some(subdirectory) = &subdirectory {
                url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
            }
            let url = VerbatimUrl::from_url(url);

            let repository = git.clone();

            RequirementSource::Git {
                url,
                repository,
                reference,
                subdirectory: subdirectory.map(PathBuf::from),
            }
        }
        Source::Url { url, subdirectory } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }

            let mut verbatim_url = url.clone();
            if verbatim_url.fragment().is_some() {
                return Err(LoweringError::ForbiddenFragment(url));
            }
            if let Some(subdirectory) = &subdirectory {
                verbatim_url.set_fragment(Some(subdirectory));
            }

            let verbatim_url = VerbatimUrl::from_url(verbatim_url);
            RequirementSource::Url {
                location: url,
                subdirectory: subdirectory.map(PathBuf::from),
                url: verbatim_url,
            }
        }
        Source::Path { path, editable } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            path_source(path, project_dir, editable)?
        }
        Source::Registry { index } => match requirement.version_or_url {
            None => return Err(LoweringError::UnconstrainedVersion),
            Some(VersionOrUrl::VersionSpecifier(version)) => RequirementSource::Registry {
                specifier: version,
                index: Some(index),
            },
            Some(VersionOrUrl::Url(_)) => return Err(LoweringError::ConflictingUrls),
        },
        Source::Workspace {
            workspace,
            editable,
        } => {
            if !workspace {
                return Err(LoweringError::WorkspaceFalse);
            }
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            let path = workspace_packages
                .get(&requirement.name)
                .ok_or(LoweringError::UndeclaredWorkspacePackage)?
                .clone();
            path_source(path, project_dir, editable)?
        }
        Source::CatchAll { .. } => {
            // Emit a dedicated error message, which is an improvement over Serde's default error.
            return Err(LoweringError::InvalidEntry);
        }
    };
    Ok(Requirement {
        name: requirement.name,
        extras: requirement.extras,
        marker: requirement.marker,
        source,
    })
}

/// Convert a path string to a path section.
fn path_source(
    path: String,
    project_dir: &Path,
    editable: Option<bool>,
) -> Result<RequirementSource, LoweringError> {
    let url = VerbatimUrl::parse_path(&path, project_dir).with_given(path.clone());
    let path_buf = PathBuf::from(&path);
    let path_buf = path_buf
        .absolutize_from(project_dir)
        .map_err(|err| LoweringError::AbsolutizeError(path, err))?
        .to_path_buf();
    Ok(RequirementSource::Path {
        path: path_buf,
        url,
        editable,
    })
}

/// Given an extra in a project that may contain references to the project itself, flatten it into
/// a list of requirements.
///
/// For example:
/// ```toml
/// [project]
/// name = "my-project"
/// version = "0.0.1"
/// dependencies = [
///     "tomli",
/// ]
///
/// [project.optional-dependencies]
/// test = [
///     "pep517",
/// ]
/// dev = [
///     "my-project[test]",
/// ]
/// ```
fn flatten_extra(
    project_name: &PackageName,
    requirements: &[Requirement],
    extras: &IndexMap<ExtraName, Vec<Requirement>>,
) -> Vec<Requirement> {
    fn inner(
        project_name: &PackageName,
        requirements: &[Requirement],
        extras: &IndexMap<ExtraName, Vec<Requirement>>,
        seen: &mut FxHashSet<ExtraName>,
    ) -> Vec<Requirement> {
        let mut flattened = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            if requirement.name == *project_name {
                for extra in &requirement.extras {
                    // Avoid infinite recursion on mutually recursive extras.
                    if !seen.insert(extra.clone()) {
                        continue;
                    }

                    // Flatten the extra requirements.
                    for (other_extra, extra_requirements) in extras {
                        if other_extra == extra {
                            flattened.extend(inner(project_name, extra_requirements, extras, seen));
                        }
                    }
                }
            } else {
                flattened.push(requirement.clone());
            }
        }
        flattened
    }

    inner(
        project_name,
        requirements,
        extras,
        &mut FxHashSet::default(),
    )
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
mod serde_from_and_to_string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub(super) fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use anyhow::Context;
    use indoc::indoc;
    use insta::assert_snapshot;
    use uv_configuration::PreviewMode;

    use uv_fs::Simplified;

    use crate::{ExtrasSpecification, RequirementsSpecification};

    fn from_source(
        contents: &str,
        path: impl AsRef<Path>,
        extras: &ExtrasSpecification,
    ) -> anyhow::Result<RequirementsSpecification> {
        let path = uv_fs::absolutize_path(path.as_ref())?;
        RequirementsSpecification::parse_direct_pyproject_toml(
            contents,
            extras,
            path.as_ref(),
            PreviewMode::Enabled,
        )
        .with_context(|| format!("Failed to parse `{}`", path.user_display()))
    }

    fn format_err(input: &str) -> String {
        let err = from_source(input, "pyproject.toml", &ExtrasSpecification::None).unwrap_err();
        let mut causes = err.chain();
        let mut message = String::new();
        message.push_str(&format!("error: {}\n", causes.next().unwrap()));
        for err in causes {
            message.push_str(&format!("  Caused by: {err}\n"));
        }
        message
    }

    #[test]
    fn conflict_project_and_sources() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm @ git+https://github.com/tqdm/tqdm",
            ]

            [tool.uv.sources]
            tqdm = { url = "https://files.pythonhosted.org/packages/a5/d6/502a859bac4ad5e274255576cd3e15ca273cdb91731bc39fb840dd422ee9/tqdm-4.66.0-py3-none-any.whl" }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
        "###);
    }

    #[test]
    fn too_many_git_specs() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]

            [tool.uv.sources]
            tqdm = { git = "https://github.com/tqdm/tqdm", rev = "baaaaaab", tag = "v1.0.0" }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: Can only specify one of rev, tag, or branch
        "###);
    }

    #[test]
    fn too_many_git_typo() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]

            [tool.uv.sources]
            tqdm = { git = "https://github.com/tqdm/tqdm", ref = "baaaaaab" }
        "#};

        // TODO(konsti): This should tell you the set of valid fields
        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: TOML parse error at line 9, column 8
          |
        9 | tqdm = { git = "https://github.com/tqdm/tqdm", ref = "baaaaaab" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[test]
    fn you_cant_mix_those() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]

            [tool.uv.sources]
            tqdm = { path = "tqdm", index = "torch" }
        "#};

        // TODO(konsti): This should tell you the set of valid fields
        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: TOML parse error at line 9, column 8
          |
        9 | tqdm = { path = "tqdm", index = "torch" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[test]
    fn missing_constraint() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: Must specify a version constraint
        "###);
    }

    #[test]
    fn invalid_syntax() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm ==4.66.0",
            ]

            [tool.uv.sources]
            tqdm = { url = invalid url to tqdm-4.66.0-py3-none-any.whl" }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: TOML parse error at line 9, column 16
          |
        9 | tqdm = { url = invalid url to tqdm-4.66.0-py3-none-any.whl" }
          |                ^
        invalid string
        expected `"`, `'`

        "###);
    }

    #[test]
    fn invalid_url() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm ==4.66.0",
            ]

            [tool.uv.sources]
            tqdm = { url = "§invalid#+#*Ä" }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: TOML parse error at line 9, column 8
          |
        9 | tqdm = { url = "§invalid#+#*Ä" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[test]
    fn workspace_and_url_spec() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm @ git+https://github.com/tqdm/tqdm",
            ]

            [tool.uv.sources]
            tqdm = { workspace = true }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
        "###);
    }

    #[test]
    fn missing_workspace_package() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm ==4.66.0",
            ]

            [tool.uv.sources]
            tqdm = { workspace = true }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: Package is not included as workspace package in `tool.uv.workspace`
        "###);
    }

    #[test]
    fn cant_be_dynamic() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dynamic = [
                "dependencies"
            ]

            [tool.uv.sources]
            tqdm = { workspace = true }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: pyproject.toml section is declared as dynamic, but must be static: `project.dependencies`
        "###);
    }

    #[test]
    fn missing_project_section() {
        let input = indoc! {"
            [tool.uv.sources]
            tqdm = { workspace = true }
        "};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: Must specify a `[project]` section alongside `[tool.uv.sources]`
        "###);
    }
}
