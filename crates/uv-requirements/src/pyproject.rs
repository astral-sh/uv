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

use distribution_types::{ParsedUrlError, UvRequirement, UvRequirements, UvSource};
use pep508_rs::{Requirement, VerbatimUrl, VersionOrUrl};
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::{ExtraName, PackageName};

use crate::ExtrasSpecification;

#[derive(Debug, Error)]
pub(crate) enum Pep621Error {
    #[error(transparent)]
    Pep508(#[from] pep508_rs::Pep508Error),
    #[error("You need to specify a `[project]` section to use `[tool.uv.sources]`")]
    MissingProjectSection,
    #[error("pyproject.toml section is declared as dynamic, but must be static: `{0}`")]
    CantBeDynamic(&'static str),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] UvSourcesLoweringError),
}

/// An error parsing and merging `tool.uv.sources` with
/// `project.{dependencies,optional-dependencies}`.
#[derive(Debug, Error)]
pub(crate) enum UvSourcesLoweringError {
    #[error("Invalid URL structure")]
    DirectUrl(#[from] Box<ParsedUrlError>),
    #[error("Unsupported path (can't convert to URL): `{}`", _0.user_display())]
    PathToUrl(PathBuf),
    #[error("The package is not included as workspace package in `tool.uv.workspace`")]
    UndeclaredWorkspacePackage,
    #[error("You need to specify a version constraint")]
    UnconstrainedVersion,
    #[error("You can only use one of rev, tag or branch")]
    MoreThanOneGitRef,
    #[error("You can't combine these options in `tool.uv.sources`")]
    InvalidEntry,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error("You can't combine a url in `project` with `tool.uv.sources`")]
    ConflictingUrls,
    /// Note: Infallible on unix and windows.
    #[error("Could not normalize path: `{0}`")]
    AbsolutizeError(String, #[source] io::Error),
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PyProjectToml {
    /// Project metadata
    pub(crate) project: Option<Project>,
    /// Uv additions
    pub(crate) tool: Option<Tool>,
}

/// PEP 621 project metadata (`project`).
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for extracting static requirements.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Project {
    /// The name of the project
    pub(crate) name: PackageName,
    /// Project dependencies
    pub(crate) dependencies: Option<Vec<String>>,
    /// Optional dependencies
    pub(crate) optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    pub(crate) dynamic: Option<Vec<String>>,
}

/// `tool`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    pub(crate) uv: Option<Uv>,
}

/// `tool.uv`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Uv {
    pub(crate) sources: Option<HashMap<PackageName, Source>>,
    pub(crate) workspace: Option<UvWorkspace>,
}

/// `tool.uv.workspace`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct UvWorkspace {
    pub(crate) members: Option<Vec<SerdePattern>>,
    pub(crate) exclude: Option<Vec<SerdePattern>>,
}

/// (De)serialize globs as strings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct SerdePattern(#[serde(with = "serde_from_and_to_string")] pub(crate) Pattern);

impl Deref for SerdePattern {
    type Target = Pattern;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A `tool.uv.sources` value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum Source {
    Git {
        git: String,
        subdirectory: Option<String>,
        // Only one of the three may be used, we validate this later for a better error message.
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
    },
    Url {
        url: String,
        subdirectory: Option<String>,
    },
    Path {
        path: String,
        /// `false` by default.
        editable: Option<bool>,
    },
    Registry {
        // TODO(konstin): The string is more-or-less a placeholder
        index: String,
    },
    Workspace {
        workspace: bool,
        /// `true` by default.
        editable: Option<bool>,
    },
    /// Show a better error message for invalid combinations of options.
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
pub(crate) struct UvMetadata {
    /// The name of the project.
    pub(crate) name: PackageName,
    /// The requirements extracted from the project.
    pub(crate) requirements: Vec<UvRequirement>,
    /// The extras used to collect requirements.
    pub(crate) used_extras: FxHashSet<ExtraName>,
}

impl UvMetadata {
    /// Extract the static [`UvMetadata`] from a [`Project`] and [`ExtrasSpecification`], if
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

        let uv_requirements = lower_requirements(
            &project.dependencies.unwrap_or_default(),
            &project.optional_dependencies.unwrap_or_default(),
            &project.name,
            project_dir,
            &project_sources.unwrap_or_default(),
            workspace_sources,
            workspace_packages,
        )?;

        // Parse out the project requirements.
        let mut requirements = uv_requirements.dependencies;

        // Include any optional dependencies specified in `extras`.
        let mut used_extras = FxHashSet::default();
        if !extras.is_empty() {
            // Include the optional dependencies if the extras are requested.
            for (extra, optional_requirements) in &uv_requirements.optional_dependencies {
                if extras.contains(extra) {
                    used_extras.insert(extra.clone());
                    requirements.extend(flatten_extra(
                        &project.name,
                        optional_requirements,
                        &uv_requirements.optional_dependencies,
                    ));
                }
            }
        }

        Ok(Some(Self {
            name: project.name,
            requirements,
            used_extras,
        }))
    }
}

pub(crate) fn lower_requirements(
    dependencies: &[String],
    optional_dependencies: &IndexMap<ExtraName, Vec<String>>,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &HashMap<PackageName, Source>,
    workspace_sources: &HashMap<PackageName, Source>,
    workspace_packages: &HashMap<PackageName, String>,
) -> Result<UvRequirements, Pep621Error> {
    let dependencies = dependencies
        .iter()
        .map(|dependency| {
            let requirement = Requirement::from_str(dependency)?;
            let name = requirement.name.clone();
            lower_requirement(
                requirement,
                project_name,
                project_dir,
                project_sources,
                workspace_sources,
                workspace_packages,
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
                    let requirement = Requirement::from_str(dependency)?;
                    let name = requirement.name.clone();
                    lower_requirement(
                        requirement,
                        project_name,
                        project_dir,
                        project_sources,
                        workspace_sources,
                        workspace_packages,
                    )
                    .map_err(|err| Pep621Error::LoweringError(name, err))
                })
                .collect::<Result<_, Pep621Error>>()?;
            Ok((extra_name.clone(), dependencies))
        })
        .collect::<Result<_, Pep621Error>>()?;
    Ok(UvRequirements {
        dependencies,
        optional_dependencies,
    })
}

/// Combine `project.dependencies`/`project.optional-dependencies` with `tool.uv.sources`.
pub(crate) fn lower_requirement(
    requirement: Requirement,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &HashMap<PackageName, Source>,
    workspace_sources: &HashMap<PackageName, Source>,
    workspace_packages: &HashMap<PackageName, String>,
) -> Result<UvRequirement, UvSourcesLoweringError> {
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
        return Err(UvSourcesLoweringError::UndeclaredWorkspacePackage);
    }

    let Some(source) = source else {
        // Support recursive editable inclusions. TODO(konsti): This is a workspace feature.
        return if requirement.version_or_url.is_none() && &requirement.name != project_name {
            Err(UvSourcesLoweringError::UnconstrainedVersion)
        } else {
            Ok(UvRequirement::from_requirement(requirement).map_err(Box::new)?)
        };
    };

    let source = match source {
        Source::Git {
            git,
            subdirectory,
            rev,
            tag,
            branch,
        } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(UvSourcesLoweringError::ConflictingUrls);
            }
            // TODO(konsti): We know better than this enum
            let reference = match (rev, tag, branch) {
                (None, None, None) => GitReference::DefaultBranch,
                (Some(rev), None, None) => {
                    if rev.len() == 40 {
                        GitReference::FullCommit(rev)
                    } else {
                        GitReference::BranchOrTagOrCommit(rev)
                    }
                }
                (None, Some(tag), None) => GitReference::BranchOrTag(tag),
                (None, None, Some(branch)) => GitReference::BranchOrTag(branch),
                _ => return Err(UvSourcesLoweringError::MoreThanOneGitRef),
            };

            // TODO(konsti): Wrong verbatim url
            let url = VerbatimUrl::from_url(Url::parse(&git)?).with_given(git);
            let repository = url.to_url().clone();
            UvSource::Git {
                url,
                repository,
                reference,
                subdirectory: subdirectory.map(PathBuf::from),
            }
        }
        Source::Url { url, subdirectory } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(UvSourcesLoweringError::ConflictingUrls);
            }
            let url = VerbatimUrl::from_url(Url::parse(&url)?).with_given(url);
            UvSource::Url {
                url,
                subdirectory: subdirectory.map(PathBuf::from),
            }
        }
        Source::Path { path, editable } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(UvSourcesLoweringError::ConflictingUrls);
            }
            path_source(path, project_dir, editable)?
        }
        Source::Registry { index } => match requirement.version_or_url {
            None => return Err(UvSourcesLoweringError::UnconstrainedVersion),
            Some(VersionOrUrl::VersionSpecifier(version)) => UvSource::Registry {
                version,
                index: Some(index),
            },
            Some(VersionOrUrl::Url(_)) => return Err(UvSourcesLoweringError::ConflictingUrls),
        },
        Source::Workspace {
            workspace,
            editable,
        } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(UvSourcesLoweringError::ConflictingUrls);
            }
            if !workspace {
                todo!()
            }
            let path = workspace_packages
                .get(&requirement.name)
                .ok_or(UvSourcesLoweringError::UndeclaredWorkspacePackage)?
                .clone();
            path_source(path, project_dir, editable)?
        }
        Source::CatchAll { .. } => {
            // This is better than a serde error about not matching any enum variant
            return Err(UvSourcesLoweringError::InvalidEntry);
        }
    };
    Ok(UvRequirement {
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
) -> Result<UvSource, UvSourcesLoweringError> {
    let path_buf = PathBuf::from(&path);
    let path_buf = path_buf
        .absolutize_from(project_dir)
        .map_err(|err| UvSourcesLoweringError::AbsolutizeError(path.clone(), err))?
        .to_path_buf();
    let url = VerbatimUrl::from_url(
        Url::from_file_path(&path_buf)
            .map_err(|()| UvSourcesLoweringError::PathToUrl(path_buf.clone()))?,
    )
    .with_given(path);
    Ok(UvSource::Path {
        path: path_buf,
        url,
        editable,
    })
}

/// Given an extra in a project that may contain references to the project
/// itself, flatten it into a list of requirements.
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
    requirements: &[UvRequirement],
    extras: &IndexMap<ExtraName, Vec<UvRequirement>>,
) -> Vec<UvRequirement> {
    fn inner(
        project_name: &PackageName,
        requirements: &[UvRequirement],
        extras: &IndexMap<ExtraName, Vec<UvRequirement>>,
        seen: &mut FxHashSet<ExtraName>,
    ) -> Vec<UvRequirement> {
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

    use uv_fs::Simplified;

    use crate::{ExtrasSpecification, RequirementsSpecification};

    fn from_source(
        contents: &str,
        path: impl AsRef<Path>,
        extras: &ExtrasSpecification,
    ) -> anyhow::Result<RequirementsSpecification> {
        let path = uv_fs::absolutize_path(path.as_ref())?;
        RequirementsSpecification::parse_direct_pyproject_toml(&contents, extras, path.as_ref())
            .with_context(|| format!("Failed to parse `{}`", path.user_display()))
    }

    fn format_err(input: &str) -> String {
        let err = from_source(input, "pyproject.toml", &ExtrasSpecification::None).unwrap_err();
        let mut causes = err.chain();
        let mut message = String::new();
        message.push_str(&format!("error: {}\n", causes.next().unwrap()));
        for err in causes {
            message.push_str(&format!("  Caused by: {}\n", err));
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
          Caused by: You can't combine a url in `project` with `tool.uv.sources`
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
          Caused by: You can only use one of rev, tag or branch
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
          Caused by: You need to specify a version constraint
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
          Caused by: Failed to parse entry for: `tqdm`
          Caused by: relative URL without a base
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
          Caused by: You can't combine a url in `project` with `tool.uv.sources`
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
          Caused by: The package is not included as workspace package in `tool.uv.workspace`
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
        let input = indoc! {r#"
            [tool.uv.sources]
            tqdm = { workspace = true }
        "#};

        assert_snapshot!(format_err(input), @r###"
        error: Failed to parse `pyproject.toml`
          Caused by: You need to specify a `[project]` section to use `[tool.uv.sources]`
        "###);
    }
}
