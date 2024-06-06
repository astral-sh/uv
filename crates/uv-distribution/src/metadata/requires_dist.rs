use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use std::path::Path;

use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::metadata::lowering::lower_requirement;
use crate::metadata::MetadataError;
use crate::{Metadata, ProjectWorkspace};

/// The name of the global `dev-dependencies` group.
///
/// Internally, we model dependency groups as a generic concept; but externally, we only expose the
/// `dev-dependencies` group.
pub static DEV_DEPENDENCIES: Lazy<GroupName> =
    Lazy::new(|| GroupName::new("dev".to_string()).unwrap());

#[derive(Debug, Clone)]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<pypi_types::Requirement>,
    pub provides_extras: Vec<ExtraName>,
    pub dev_dependencies: BTreeMap<GroupName, Vec<pypi_types::Requirement>>,
}

impl RequiresDist {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: pypi_types::RequiresDist) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(pypi_types::Requirement::from)
                .collect(),
            provides_extras: metadata.provides_extras,
            dev_dependencies: BTreeMap::default(),
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: pypi_types::RequiresDist,
        project_root: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataError> {
        // TODO(konsti): Limit discovery for Git checkouts to Git root.
        // TODO(konsti): Cache workspace discovery.
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(project_root, None).await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(metadata, &project_workspace, preview_mode)
    }

    pub fn from_project_workspace(
        metadata: pypi_types::RequiresDist,
        project_workspace: &ProjectWorkspace,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataError> {
        // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
        let empty = BTreeMap::default();
        let sources = project_workspace
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .unwrap_or(&empty);

        let dev_dependencies = {
            let dev_dependencies = project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.dev_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .map(|requirement| {
                    let requirement_name = requirement.name.clone();
                    lower_requirement(
                        requirement,
                        &metadata.name,
                        project_workspace.project_root(),
                        sources,
                        project_workspace.workspace(),
                        preview_mode,
                    )
                    .map_err(|err| MetadataError::LoweringError(requirement_name.clone(), err))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if dev_dependencies.is_empty() {
                BTreeMap::default()
            } else {
                BTreeMap::from([(DEV_DEPENDENCIES.clone(), dev_dependencies)])
            }
        };

        let requires_dist = metadata
            .requires_dist
            .into_iter()
            .map(|requirement| {
                let requirement_name = requirement.name.clone();
                lower_requirement(
                    requirement,
                    &metadata.name,
                    project_workspace.project_root(),
                    sources,
                    project_workspace.workspace(),
                    preview_mode,
                )
                .map_err(|err| MetadataError::LoweringError(requirement_name.clone(), err))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            name: metadata.name,
            requires_dist,
            dev_dependencies,
            provides_extras: metadata.provides_extras,
        })
    }
}

impl From<Metadata> for RequiresDist {
    fn from(metadata: Metadata) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata.requires_dist,
            provides_extras: metadata.provides_extras,
            dev_dependencies: metadata.dev_dependencies,
        }
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use anyhow::Context;
    use indoc::indoc;
    use insta::assert_snapshot;

    use uv_configuration::PreviewMode;

    use crate::pyproject::PyProjectToml;
    use crate::{ProjectWorkspace, RequiresDist};

    async fn requires_dist_from_pyproject_toml(contents: &str) -> anyhow::Result<RequiresDist> {
        let pyproject_toml: PyProjectToml = toml::from_str(contents)?;
        let path = Path::new("pyproject.toml");
        let project_workspace = ProjectWorkspace::from_project(
            path,
            pyproject_toml
                .project
                .as_ref()
                .context("metadata field project not found")?,
            &pyproject_toml,
            Some(path),
        )
        .await?;
        let requires_dist = pypi_types::RequiresDist::parse_pyproject_toml(contents)?;
        Ok(RequiresDist::from_project_workspace(
            requires_dist,
            &project_workspace,
            PreviewMode::Enabled,
        )?)
    }

    async fn format_err(input: &str) -> String {
        let err = requires_dist_from_pyproject_toml(input).await.unwrap_err();
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
        assert!(requires_dist_from_pyproject_toml(input).await.is_ok());
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
