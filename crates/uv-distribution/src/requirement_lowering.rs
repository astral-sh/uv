use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use path_absolutize::Absolutize;
use thiserror::Error;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{VerbatimUrl, VersionOrUrl};
use pypi_types::{Requirement, RequirementSource, VerbatimParsedUrl};
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::PackageName;
use uv_warnings::warn_user_once;

use crate::pyproject::Source;
use crate::Workspace;

/// An error parsing and merging `tool.uv.sources` with
/// `project.{dependencies,optional-dependencies}`.
#[derive(Debug, Error)]
pub enum LoweringError {
    #[error("Package is not included as workspace package in `tool.uv.workspace`")]
    UndeclaredWorkspacePackage,
    #[error("Can only specify one of: `rev`, `tag`, or `branch`")]
    MoreThanOneGitRef,
    #[error("Unable to combine options in `tool.uv.sources`")]
    InvalidEntry,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    InvalidVerbatimUrl(#[from] pep508_rs::VerbatimUrlError),
    #[error("Can't combine URLs from both `project.dependencies` and `tool.uv.sources`")]
    ConflictingUrls,
    #[error("Could not normalize path: `{}`", _0.user_display())]
    Absolutize(PathBuf, #[source] io::Error),
    #[error("Fragments are not allowed in URLs: `{0}`")]
    ForbiddenFragment(Url),
    #[error("`workspace = false` is not yet supported")]
    WorkspaceFalse,
    #[error("`tool.uv.sources` is a preview feature; use `--preview` or set `UV_PREVIEW=1` to enable it")]
    MissingPreview,
}

/// Combine `project.dependencies` or `project.optional-dependencies` with `tool.uv.sources`.
pub(crate) fn lower_requirement(
    requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &BTreeMap<PackageName, Source>,
    workspace: &Workspace,
    preview: PreviewMode,
) -> Result<Requirement, LoweringError> {
    let source = project_sources
        .get(&requirement.name)
        .or(workspace.sources().get(&requirement.name))
        .cloned();

    let workspace_package_declared =
        // We require that when you use a package that's part of the workspace, ...
        !workspace.packages().contains_key(&requirement.name)
        // ... it must be declared as a workspace dependency (`workspace = true`), ...
        || matches!(
            source,
            Some(Source::Workspace {
                // By using toml, we technically support `workspace = false`.
                workspace: true,
                ..
            })
        )
        // ... except for recursive self-inclusion (extras that activate other extras), e.g.
        // `framework[machine_learning]` depends on `framework[cuda]`.
        || &requirement.name == project_name;
    if !workspace_package_declared {
        return Err(LoweringError::UndeclaredWorkspacePackage);
    }

    let Some(source) = source else {
        let has_sources = !project_sources.is_empty() || !workspace.sources().is_empty();
        // Support recursive editable inclusions.
        if has_sources && requirement.version_or_url.is_none() && &requirement.name != project_name
        {
            warn_user_once!(
                "Missing version constraint (e.g., a lower bound) for `{}`",
                requirement.name
            );
        }
        return Ok(Requirement::from(requirement));
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
                precise: None,
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
            path_source(path, project_dir, editable.unwrap_or(false))?
        }
        Source::Registry { index } => match requirement.version_or_url {
            None => {
                warn_user_once!(
                    "Missing version constraint (e.g., a lower bound) for `{}`",
                    requirement.name
                );
                RequirementSource::Registry {
                    specifier: VersionSpecifiers::empty(),
                    index: Some(index),
                }
            }
            Some(VersionOrUrl::VersionSpecifier(version)) => RequirementSource::Registry {
                specifier: version,
                index: Some(index),
            },
            Some(VersionOrUrl::Url(_)) => return Err(LoweringError::ConflictingUrls),
        },
        Source::Workspace {
            workspace: is_workspace,
            editable,
        } => {
            if !is_workspace {
                return Err(LoweringError::WorkspaceFalse);
            }
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            let path = workspace
                .packages()
                .get(&requirement.name)
                .ok_or(LoweringError::UndeclaredWorkspacePackage)?
                .clone();
            path_source(path.root(), workspace.root(), editable.unwrap_or(true))?
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
        origin: requirement.origin,
    })
}

/// Convert a path string to a path section.
fn path_source(
    path: impl AsRef<Path>,
    project_dir: &Path,
    editable: bool,
) -> Result<RequirementSource, LoweringError> {
    let url = VerbatimUrl::parse_path(path.as_ref(), project_dir)?
        .with_given(path.as_ref().to_string_lossy().to_string());
    let path_buf = path.as_ref().to_path_buf();
    let path_buf = path_buf
        .absolutize_from(project_dir)
        .map_err(|err| LoweringError::Absolutize(path.as_ref().to_path_buf(), err))?
        .to_path_buf();
    //if !editable {
    //    // TODO(konsti): Support this. Currently we support `{ workspace = true }`, but we don't
    //    //  support `{ workspace = true, editable = false }` since we only collect editables.
    //    return Err(LoweringError::NonEditableWorkspaceDependency);
    //}
    Ok(RequirementSource::Path {
        path: path_buf,
        url,
        editable,
    })
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::str::FromStr;

    use indoc::indoc;
    use insta::assert_snapshot;

    use pypi_types::Metadata23;
    use uv_configuration::PreviewMode;
    use uv_normalize::PackageName;

    use crate::metadata::Metadata;
    use crate::pyproject::PyProjectToml;
    use crate::ProjectWorkspace;

    async fn metadata_from_pyproject_toml(contents: &str) -> anyhow::Result<Metadata> {
        let pyproject_toml: PyProjectToml = toml::from_str(contents)?;
        let path = Path::new("pyproject.toml");
        let project_name = PackageName::from_str("foo").unwrap();
        let project_workspace =
            ProjectWorkspace::from_project(path, &pyproject_toml, project_name, Some(path)).await?;
        let metadata = Metadata23::parse_pyproject_toml(contents)?;
        Ok(Metadata::from_project_workspace(
            metadata,
            &project_workspace,
            PreviewMode::Enabled,
        )?)
    }

    async fn format_err(input: &str) -> String {
        let err = metadata_from_pyproject_toml(input).await.unwrap_err();
        let mut causes = err.chain();
        let mut message = String::new();
        message.push_str(&format!("error: {}\n", causes.next().unwrap()));
        for err in causes {
            message.push_str(&format!("  Caused by: {err}\n"));
        }
        message
    }

    #[tokio::test]
    async fn conflict_project_and_sources() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: Failed to parse entry for: `tqdm`
          Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
        "###);
    }

    #[tokio::test]
    async fn too_many_git_specs() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: Failed to parse entry for: `tqdm`
          Caused by: Can only specify one of: `rev`, `tag`, or `branch`
        "###);
    }

    #[tokio::test]
    async fn too_many_git_typo() {
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
        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = { git = "https://github.com/tqdm/tqdm", ref = "baaaaaab" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[tokio::test]
    async fn you_cant_mix_those() {
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
        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = { path = "tqdm", index = "torch" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[tokio::test]
    async fn missing_constraint() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]
        "#};
        assert!(metadata_from_pyproject_toml(input).await.is_ok());
    }

    #[tokio::test]
    async fn invalid_syntax() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 16
          |
        8 | tqdm = { url = invalid url to tqdm-4.66.0-py3-none-any.whl" }
          |                ^
        invalid string
        expected `"`, `'`

        "###);
    }

    #[tokio::test]
    async fn invalid_url() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = { url = "§invalid#+#*Ä" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^
        data did not match any variant of untagged enum Source

        "###);
    }

    #[tokio::test]
    async fn workspace_and_url_spec() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: Failed to parse entry for: `tqdm`
          Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
        "###);
    }

    #[tokio::test]
    async fn missing_workspace_package() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: Failed to parse entry for: `tqdm`
          Caused by: Package is not included as workspace package in `tool.uv.workspace`
        "###);
    }

    #[tokio::test]
    async fn cant_be_dynamic() {
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

        assert_snapshot!(format_err(input).await, @r###"
        error: The following field was marked as dynamic: dependencies
        "###);
    }

    #[tokio::test]
    async fn missing_project_section() {
        let input = indoc! {"
            [tool.uv.sources]
            tqdm = { workspace = true }
        "};

        assert_snapshot!(format_err(input).await, @r###"
        error: metadata field project not found
        "###);
    }
}
