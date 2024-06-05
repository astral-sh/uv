mod lowering;

use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;

use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{HashDigest, Metadata23};
use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, PackageName};

use crate::metadata::lowering::{lower_requirement, LoweringError};
use crate::{ProjectWorkspace, WorkspaceError};

#[derive(Debug, Error)]
pub enum MetadataLoweringError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] LoweringError),
}

#[derive(Debug, Clone)]
pub struct Metadata {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<pypi_types::Requirement>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

impl Metadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: Metadata23) -> Self {
        Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(pypi_types::Requirement::from)
                .collect(),
            requires_python: metadata.requires_python,
            provides_extras: metadata.provides_extras,
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: Metadata23,
        project_root: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataLoweringError> {
        // TODO(konsti): Limit discovery for Git checkouts to Git root.
        // TODO(konsti): Cache workspace discovery.
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(project_root, None).await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(metadata, &project_workspace, preview_mode)
    }

    fn from_project_workspace(
        metadata: Metadata23,
        project_workspace: &ProjectWorkspace,
        preview_mode: PreviewMode,
    ) -> Result<Metadata, MetadataLoweringError> {
        let empty = BTreeMap::default();
        let sources = project_workspace
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .unwrap_or(&empty);

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
                .map_err(|err| MetadataLoweringError::LoweringError(requirement_name.clone(), err))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist,
            requires_python: metadata.requires_python,
            provides_extras: metadata.provides_extras,
        })
    }
}

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The [`Metadata`] for the underlying distribution.
    pub metadata: Metadata,
    /// The hashes of the source or built archive.
    pub hashes: Vec<HashDigest>,
}

impl ArchiveMetadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: Metadata23) -> Self {
        Self {
            metadata: Metadata::from_metadata23(metadata),
            hashes: vec![],
        }
    }
}

impl From<Metadata> for ArchiveMetadata {
    fn from(metadata: Metadata) -> Self {
        Self {
            metadata,
            hashes: vec![],
        }
    }
}

#[cfg(test)]
mod test {
    use anyhow::Context;
    use std::path::Path;

    use indoc::indoc;
    use insta::assert_snapshot;

    use pypi_types::Metadata23;
    use uv_configuration::PreviewMode;

    use crate::metadata::Metadata;
    use crate::pyproject::PyProjectToml;
    use crate::ProjectWorkspace;

    async fn metadata_from_pyproject_toml(contents: &str) -> anyhow::Result<Metadata> {
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
