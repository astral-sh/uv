use std::collections::BTreeMap;
use std::path::Path;

use uv_auth::CredentialsCache;
use uv_cache::Cache;
use uv_configuration::NoSources;
use uv_distribution_types::{IndexLocations, Requirement};
use uv_normalize::PackageName;
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Sources, ToolUvSources};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, ProjectWorkspace, WorkspaceCache};

use crate::RequiresDist;
use crate::metadata::{GitWorkspaceMember, LoweredRequirement, MetadataError};

/// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
/// dependencies.
pub(crate) async fn lower_requires_dist(
    metadata: uv_pypi_types::RequiresDist,
    install_path: &Path,
    git_member: Option<&GitWorkspaceMember<'_>>,
    locations: &IndexLocations,
    sources: NoSources,
    editable: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    credentials_cache: &CredentialsCache,
) -> Result<RequiresDist, MetadataError> {
    let discovery = DiscoveryOptions {
        stop_discovery_at: git_member.map(|git_member| {
            git_member
                .fetch_root
                .parent()
                .expect("git checkout has a parent")
                .to_path_buf()
        }),
        members: if sources.is_none() {
            MemberDiscovery::default()
        } else {
            MemberDiscovery::None
        },
    };
    let Some(project_workspace) =
        ProjectWorkspace::from_maybe_project_root(install_path, &discovery, cache, workspace_cache)
            .await?
    else {
        return from_metadata23_with_source_context(metadata, git_member);
    };

    from_project_workspace(
        metadata,
        &project_workspace,
        git_member,
        locations,
        &sources,
        editable,
        cache,
        workspace_cache,
        credentials_cache,
    )
    .await
}

fn from_metadata23_with_source_context(
    metadata: uv_pypi_types::RequiresDist,
    git_member: Option<&GitWorkspaceMember<'_>>,
) -> Result<RequiresDist, MetadataError> {
    let requires_dist = Box::into_iter(metadata.requires_dist)
        .map(|requirement| {
            let requirement_name = requirement.name.clone();
            LoweredRequirement::preserve_git_source(requirement, git_member)
                .map(LoweredRequirement::into_inner)
                .map_err(|err| MetadataError::LoweringError(requirement_name, Box::new(err)))
        })
        .collect::<Result<Box<_>, _>>()?;

    Ok(RequiresDist {
        name: metadata.name,
        requires_dist,
        provides_extra: metadata.provides_extra,
        dependency_groups: BTreeMap::default(),
        dynamic: metadata.dynamic,
    })
}

async fn from_project_workspace(
    metadata: uv_pypi_types::RequiresDist,
    project_workspace: &ProjectWorkspace,
    git_member: Option<&GitWorkspaceMember<'_>>,
    locations: &IndexLocations,
    no_sources: &NoSources,
    editable: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    credentials_cache: &CredentialsCache,
) -> Result<RequiresDist, MetadataError> {
    // Collect any `tool.uv.index` entries.
    let empty = vec![];
    let project_indexes = project_workspace
        .current_project()
        .pyproject_toml()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.index.as_deref())
        .unwrap_or(&empty);

    // Collect any `tool.uv.sources` and `tool.uv.dev_dependencies` from `pyproject.toml`.
    let empty = BTreeMap::default();
    let project_sources = project_workspace
        .current_project()
        .pyproject_toml()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.sources.as_ref())
        .map(ToolUvSources::inner)
        .unwrap_or(&empty);

    let dependency_groups = FlatDependencyGroups::from_pyproject_toml(
        project_workspace.current_project().root(),
        project_workspace.current_project().pyproject_toml(),
    )?;

    // Now that we've resolved the dependency groups, we can validate that each source references
    // a valid extra or group, if present.
    validate_sources(project_sources, &metadata, &dependency_groups)?;

    // Lower the dependency groups.
    let mut lowered_dependency_groups = BTreeMap::new();
    for (name, flat_group) in dependency_groups {
        let mut requirements = Vec::new();
        for requirement in flat_group.requirements {
            if no_sources.for_package(&requirement.name) {
                requirements.push(Requirement::from(requirement));
                continue;
            }

            let requirement_name = requirement.name.clone();
            requirements.extend(
                LoweredRequirement::from_requirement(
                    requirement,
                    Some(&metadata.name),
                    project_workspace.project_root(),
                    project_sources,
                    project_indexes,
                    None,
                    Some(&name),
                    locations,
                    project_workspace.workspace(),
                    git_member,
                    editable,
                    cache,
                    workspace_cache,
                    credentials_cache,
                )
                .await
                .map(|requirement| {
                    requirement
                        .map(LoweredRequirement::into_inner)
                        .map_err(|err| {
                            MetadataError::GroupLoweringError(
                                name.clone(),
                                requirement_name.clone(),
                                Box::new(err),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
            );
        }
        lowered_dependency_groups.insert(name, requirements.into_boxed_slice());
    }

    // Lower the requirements.
    let mut requires_dist = Vec::new();
    for requirement in Box::into_iter(metadata.requires_dist) {
        if no_sources.for_package(&requirement.name) {
            requires_dist.push(Requirement::from(requirement));
            continue;
        }

        let requirement_name = requirement.name.clone();
        let extra = requirement.marker.top_level_extra_name();
        requires_dist.extend(
            LoweredRequirement::from_requirement(
                requirement,
                Some(&metadata.name),
                project_workspace.project_root(),
                project_sources,
                project_indexes,
                extra.as_deref(),
                None,
                locations,
                project_workspace.workspace(),
                git_member,
                editable,
                cache,
                workspace_cache,
                credentials_cache,
            )
            .await
            .map(|requirement| {
                requirement
                    .map(LoweredRequirement::into_inner)
                    .map_err(|err| {
                        MetadataError::LoweringError(requirement_name.clone(), Box::new(err))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        );
    }

    Ok(RequiresDist {
        name: metadata.name,
        requires_dist: requires_dist.into_boxed_slice(),
        dependency_groups: lowered_dependency_groups,
        provides_extra: metadata.provides_extra,
        dynamic: metadata.dynamic,
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
                if !metadata.provides_extra.contains(extra) {
                    return Err(MetadataError::MissingSourceExtra(
                        name.clone(),
                        extra.clone(),
                    ));
                }

                // If there is no such requirement with the extra, error.
                if !metadata.requires_dist.iter().any(|requirement| {
                    requirement.name == *name
                        && requirement.marker.top_level_extra_name().as_deref() == Some(extra)
                }) {
                    return Err(MetadataError::IncompleteSourceExtra(
                        name.clone(),
                        extra.clone(),
                    ));
                }
            }

            if let Some(group) = source.group() {
                // If the group doesn't exist at all, error.
                let Some(flat_group) = dependency_groups.get(group) else {
                    return Err(MetadataError::MissingSourceGroup(
                        name.clone(),
                        group.clone(),
                    ));
                };

                // If there is no such requirement with the group, error.
                if !flat_group
                    .requirements
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

#[cfg(test)]
mod test {
    use std::fmt::Write;
    use std::path::Path;

    use indoc::indoc;
    use insta::assert_snapshot;
    use tempfile::TempDir;

    use uv_auth::CredentialsCache;
    use uv_cache::Cache;
    use uv_configuration::NoSources;
    use uv_distribution_types::IndexLocations;
    use uv_workspace::{DiscoveryOptions, ProjectWorkspace, WorkspaceCache};

    use crate::RequiresDist;

    async fn requires_dist_from_pyproject_toml(
        temp_dir: &Path,
        contents: &str,
    ) -> anyhow::Result<RequiresDist> {
        let workspace_cache = WorkspaceCache::default();
        fs_err::create_dir_all(temp_dir)?;
        fs_err::write(temp_dir.join("pyproject.toml"), contents)?;
        let cache = Cache::from_path(temp_dir.join(".uv_cache"));
        let project_workspace = ProjectWorkspace::discover(
            temp_dir,
            &DiscoveryOptions {
                stop_discovery_at: Some(temp_dir.to_path_buf()),
                ..DiscoveryOptions::default()
            },
            &cache,
            &workspace_cache,
        )
        .await?;
        let pyproject_toml = uv_pypi_types::PyProjectToml::from_toml(contents, "pyproject.toml")?;
        let requires_dist = uv_pypi_types::RequiresDist::from_pyproject_toml(pyproject_toml)?;
        Ok(super::from_project_workspace(
            requires_dist,
            &project_workspace,
            None,
            &IndexLocations::default(),
            &NoSources::default(),
            true,
            &cache,
            &workspace_cache,
            &CredentialsCache::new(),
        )
        .await?)
    }

    async fn format_err(input: &str) -> String {
        let temp_dir = TempDir::new().unwrap();
        let err = requires_dist_from_pyproject_toml(temp_dir.path(), input)
            .await
            .unwrap_err();
        let mut causes = err.chain();
        let mut message = String::new();
        let _ = writeln!(message, "error: {}", causes.next().unwrap());
        for err in causes {
            let _ = writeln!(message, "  Caused by: {err}");
        }
        message
            .replace(&temp_dir.path().display().to_string(), "[PATH]")
            .replace('\\', "/")
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

        assert_snapshot!(format_err(input).await, @"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 8
          |
        8 | tqdm = true
          |        ^^^^
        invalid type: boolean `true`, expected a single source (as a map) or list of sources
        ");
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 8
          |
        8 | tqdm = { git = "https://github.com/tqdm/tqdm", rev = "baaaaaab", tag = "v1.0.0" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        expected at most one of `rev`, `tag`, or `branch`
        "#);
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 48
          |
        8 | tqdm = { git = "https://github.com/tqdm/tqdm", ref = "baaaaaab" }
          |                                                ^^^
        unknown field `ref`, expected one of `git`, `subdirectory`, `rev`, `tag`, `branch`, `lfs`, `url`, `path`, `editable`, `package`, `index`, `workspace`, `marker`, `extra`, `group`
        "#);
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 7, column 8
          |
        7 | tqdm = { git = "https://github.com/tqdm/tqdm", extra = "torch", group = "dev" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        cannot specify both `extra` and `group`
        "#);
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 8
          |
        8 | tqdm = { path = "tqdm", index = "torch" }
          |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        cannot specify both `path` and `index`
        "#);
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
        let temp_dir = TempDir::new().unwrap();
        assert!(
            requires_dist_from_pyproject_toml(temp_dir.path(), input)
                .await
                .is_ok()
        );
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 16
          |
        8 | tqdm = { url = invalid url to tqdm-4.66.0-py3-none-any.whl" }
          |                ^
        missing opening quote, expected `"`
        "#);
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

        assert_snapshot!(format_err(input).await, @r#"
        error: Failed to parse: `[PATH]/pyproject.toml`
          Caused by: TOML parse error at line 8, column 16
          |
        8 | tqdm = { url = "§invalid#+#*Ä" }
          |                ^^^^^^^^^^^^^^^^^
        relative URL without a base: "§invalid#+#*Ä"
        "#);
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

        assert_snapshot!(format_err(input).await, @"
        error: Failed to parse entry: `tqdm`
          Caused by: `tqdm` references a workspace in `tool.uv.sources` (e.g., `tqdm = { workspace = true }`), but is not a workspace member
        ");
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

        assert_snapshot!(format_err(input).await, @"
        error: Failed to parse entry: `tqdm`
          Caused by: `tqdm` references a workspace in `tool.uv.sources` (e.g., `tqdm = { workspace = true }`), but is not a workspace member
        ");
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

        assert_snapshot!(format_err(input).await, @"error: The following field was marked as dynamic: dependencies");
    }

    #[tokio::test]
    async fn missing_project_section() {
        let input = indoc! {"
            [tool.uv.sources]
            tqdm = { workspace = true }
        "};

        assert_snapshot!(format_err(input).await, @"error: No `project` table found in: [PATH]/pyproject.toml");
    }
}
