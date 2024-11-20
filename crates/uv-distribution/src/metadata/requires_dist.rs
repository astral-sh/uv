use std::collections::BTreeMap;
use std::path::Path;

use crate::metadata::{GitWorkspaceMember, LoweredRequirement, MetadataError};
use crate::Metadata;
use uv_configuration::{LowerBound, SourceStrategy};
use uv_distribution_types::IndexLocations;
use uv_normalize::{ExtraName, GroupName, PackageName, DEV_DEPENDENCIES};
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Sources, ToolUvSources};
use uv_workspace::{DiscoveryOptions, ProjectWorkspace};

#[derive(Debug, Clone)]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<uv_pypi_types::Requirement>,
    pub provides_extras: Vec<ExtraName>,
    pub dependency_groups: BTreeMap<GroupName, Vec<uv_pypi_types::Requirement>>,
}

impl RequiresDist {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: uv_pypi_types::RequiresDist) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(uv_pypi_types::Requirement::from)
                .collect(),
            provides_extras: metadata.provides_extras,
            dependency_groups: BTreeMap::default(),
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_project_maybe_workspace(
        metadata: uv_pypi_types::RequiresDist,
        install_path: &Path,
        git_member: Option<&GitWorkspaceMember<'_>>,
        locations: &IndexLocations,
        sources: SourceStrategy,
        lower_bound: LowerBound,
    ) -> Result<Self, MetadataError> {
        // TODO(konsti): Cache workspace discovery.
        let discovery_options = if let Some(git_member) = &git_member {
            DiscoveryOptions {
                stop_discovery_at: Some(
                    git_member
                        .fetch_root
                        .parent()
                        .expect("git checkout has a parent"),
                ),
                ..Default::default()
            }
        } else {
            DiscoveryOptions::default()
        };
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(install_path, &discovery_options).await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(
            metadata,
            &project_workspace,
            git_member,
            locations,
            sources,
            lower_bound,
        )
    }

    fn from_project_workspace(
        metadata: uv_pypi_types::RequiresDist,
        project_workspace: &ProjectWorkspace,
        git_member: Option<&GitWorkspaceMember<'_>>,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
        lower_bound: LowerBound,
    ) -> Result<Self, MetadataError> {
        // Collect any `tool.uv.index` entries.
        let empty = vec![];
        let project_indexes = match source_strategy {
            SourceStrategy::Enabled => project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.index.as_deref())
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
        let empty = BTreeMap::default();
        let project_sources = match source_strategy {
            SourceStrategy::Enabled => project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .map(ToolUvSources::inner)
                .unwrap_or(&empty),
            SourceStrategy::Disabled => &empty,
        };

        // Collect the dependency groups.
        let dependency_groups = {
            // First, collect `tool.uv.dev_dependencies`
            let dev_dependencies = project_workspace
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.dev_dependencies.as_ref());

            // Then, collect `dependency-groups`
            let dependency_groups = project_workspace
                .current_project()
                .pyproject_toml()
                .dependency_groups
                .iter()
                .flatten()
                .collect::<BTreeMap<_, _>>();

            // Flatten the dependency groups.
            let mut dependency_groups =
                FlatDependencyGroups::from_dependency_groups(&dependency_groups)
                    .map_err(|err| err.with_dev_dependencies(dev_dependencies))?;

            // Add the `dev` group, if `dev-dependencies` is defined.
            if let Some(dev_dependencies) = dev_dependencies {
                dependency_groups
                    .entry(DEV_DEPENDENCIES.clone())
                    .or_insert_with(Vec::new)
                    .extend(dev_dependencies.clone());
            }

            dependency_groups
        };

        // Now that we've resolved the dependency groups, we can validate that each source references
        // a valid extra or group, if present.
        Self::validate_sources(project_sources, &metadata, &dependency_groups)?;

        // Lower the dependency groups.
        let dependency_groups = dependency_groups
            .into_iter()
            .map(|(name, requirements)| {
                let requirements = match source_strategy {
                    SourceStrategy::Enabled => requirements
                        .into_iter()
                        .flat_map(|requirement| {
                            let requirement_name = requirement.name.clone();
                            let group = name.clone();
                            let extra = None;
                            LoweredRequirement::from_requirement(
                                requirement,
                                &metadata.name,
                                project_workspace.project_root(),
                                project_sources,
                                project_indexes,
                                extra,
                                Some(&group),
                                locations,
                                project_workspace.workspace(),
                                lower_bound,
                                git_member,
                            )
                            .map(
                                move |requirement| match requirement {
                                    Ok(requirement) => Ok(requirement.into_inner()),
                                    Err(err) => Err(MetadataError::GroupLoweringError(
                                        group.clone(),
                                        requirement_name.clone(),
                                        Box::new(err),
                                    )),
                                },
                            )
                        })
                        .collect::<Result<Vec<_>, _>>(),
                    SourceStrategy::Disabled => Ok(requirements
                        .into_iter()
                        .map(uv_pypi_types::Requirement::from)
                        .collect()),
                }?;
                Ok::<(GroupName, Vec<uv_pypi_types::Requirement>), MetadataError>((
                    name,
                    requirements,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        // Lower the requirements.
        let requires_dist = metadata.requires_dist.into_iter();
        let requires_dist = match source_strategy {
            SourceStrategy::Enabled => requires_dist
                .flat_map(|requirement| {
                    let requirement_name = requirement.name.clone();
                    let extra = requirement.marker.top_level_extra_name();
                    let group = None;
                    LoweredRequirement::from_requirement(
                        requirement,
                        &metadata.name,
                        project_workspace.project_root(),
                        project_sources,
                        project_indexes,
                        extra.as_ref(),
                        group,
                        locations,
                        project_workspace.workspace(),
                        lower_bound,
                        git_member,
                    )
                    .map(move |requirement| match requirement {
                        Ok(requirement) => Ok(requirement.into_inner()),
                        Err(err) => Err(MetadataError::LoweringError(
                            requirement_name.clone(),
                            Box::new(err),
                        )),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            SourceStrategy::Disabled => requires_dist
                .into_iter()
                .map(uv_pypi_types::Requirement::from)
                .collect(),
        };

        Ok(Self {
            name: metadata.name,
            requires_dist,
            dependency_groups,
            provides_extras: metadata.provides_extras,
        })
    }

    /// Validate the sources for a given [`uv_pypi_types::RequiresDist`].
    ///
    /// If a source is requested with an `extra` or `group`, ensure that the relevant dependency is
    /// present in the relevant `project.optional-dependencies` or `dependency-groups` section.
    fn validate_sources(
        sources: &BTreeMap<PackageName, Sources>,
        metadata: &uv_pypi_types::RequiresDist,
        dependency_groups: &FlatDependencyGroups,
    ) -> Result<(), MetadataError> {
        for (name, sources) in sources {
            for source in sources.iter() {
                if let Some(extra) = source.extra() {
                    // If the extra doesn't exist at all, error.
                    if !metadata.provides_extras.contains(extra) {
                        return Err(MetadataError::MissingSourceExtra(
                            name.clone(),
                            extra.clone(),
                        ));
                    }

                    // If there is no such requirement with the extra, error.
                    if !metadata.requires_dist.iter().any(|requirement| {
                        requirement.name == *name
                            && requirement.marker.top_level_extra_name().as_ref() == Some(extra)
                    }) {
                        return Err(MetadataError::IncompleteSourceExtra(
                            name.clone(),
                            extra.clone(),
                        ));
                    }
                }

                if let Some(group) = source.group() {
                    // If the group doesn't exist at all, error.
                    let Some(dependencies) = dependency_groups.get(group) else {
                        return Err(MetadataError::MissingSourceGroup(
                            name.clone(),
                            group.clone(),
                        ));
                    };

                    // If there is no such requirement with the group, error.
                    if !dependencies
                        .iter()
                        .any(|requirement| requirement.name == *name)
                    {
                        return Err(MetadataError::IncompleteSourceGroup(
                            name.clone(),
                            group.clone(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl From<Metadata> for RequiresDist {
    fn from(metadata: Metadata) -> Self {
        Self {
            name: metadata.name,
            requires_dist: metadata.requires_dist,
            provides_extras: metadata.provides_extras,
            dependency_groups: metadata.dependency_groups,
        }
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use anyhow::Context;
    use indoc::indoc;
    use insta::assert_snapshot;
    use uv_configuration::{LowerBound, SourceStrategy};
    use uv_distribution_types::IndexLocations;
    use uv_workspace::pyproject::PyProjectToml;
    use uv_workspace::{DiscoveryOptions, ProjectWorkspace};

    use crate::RequiresDist;

    async fn requires_dist_from_pyproject_toml(contents: &str) -> anyhow::Result<RequiresDist> {
        let pyproject_toml = PyProjectToml::from_string(contents.to_string())?;
        let path = Path::new("pyproject.toml");
        let project_workspace = ProjectWorkspace::from_project(
            path,
            pyproject_toml
                .project
                .as_ref()
                .context("metadata field project not found")?,
            &pyproject_toml,
            &DiscoveryOptions {
                stop_discovery_at: Some(path),
                ..DiscoveryOptions::default()
            },
        )
        .await?;
        let requires_dist = uv_pypi_types::RequiresDist::parse_pyproject_toml(contents)?;
        Ok(RequiresDist::from_project_workspace(
            requires_dist,
            &project_workspace,
            None,
            &IndexLocations::default(),
            SourceStrategy::default(),
            LowerBound::default(),
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
        error: Failed to parse entry: `tqdm`
          Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
        "###);
    }

    #[tokio::test]
    async fn wrong_type() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = [
              "tqdm",
            ]
            [tool.uv.sources]
            tqdm = true
        "#};

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = true
          |        ^^^^
        invalid type: boolean `true`, expected a single source (as a map) or list of sources

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
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = { git = "https://github.com/tqdm/tqdm", rev = "baaaaaab", tag = "v1.0.0" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        expected at most one of `rev`, `tag`, or `branch`
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

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 48
          |
        8 | tqdm = { git = "https://github.com/tqdm/tqdm", ref = "baaaaaab" }
          |                                                ^^^
        unknown field `ref`, expected one of `git`, `subdirectory`, `rev`, `tag`, `branch`, `url`, `path`, `editable`, `index`, `workspace`, `marker`, `extra`, `group`
        "###);
    }

    #[tokio::test]
    async fn extra_and_group() {
        let input = indoc! {r#"
            [project]
            name = "foo"
            version = "0.0.0"
            dependencies = []

            [tool.uv.sources]
            tqdm = { git = "https://github.com/tqdm/tqdm", extra = "torch", group = "dev" }
        "#};

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 7, column 8
          |
        7 | tqdm = { git = "https://github.com/tqdm/tqdm", extra = "torch", group = "dev" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        cannot specify both `extra` and `group`
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

        assert_snapshot!(format_err(input).await, @r###"
        error: TOML parse error at line 8, column 8
          |
        8 | tqdm = { path = "tqdm", index = "torch" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        cannot specify both `path` and `index`
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
        error: TOML parse error at line 8, column 16
          |
        8 | tqdm = { url = "§invalid#+#*Ä" }
          |                ^^^^^^^^^^^^^^^^^
        relative URL without a base: "§invalid#+#*Ä"
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
        error: Failed to parse entry: `tqdm`
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
        error: Failed to parse entry: `tqdm`
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
