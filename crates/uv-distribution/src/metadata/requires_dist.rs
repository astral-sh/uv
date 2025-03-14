use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::slice;

use rustc_hash::FxHashSet;

use uv_configuration::SourceStrategy;
use uv_distribution_types::IndexLocations;
use uv_normalize::{ExtraName, GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pep508::MarkerTree;
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Sources, ToolUvSources};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, ProjectWorkspace, WorkspaceCache};

use crate::metadata::{GitWorkspaceMember, LoweredRequirement, MetadataError};
use crate::Metadata;

#[derive(Debug, Clone)]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<uv_pypi_types::Requirement>,
    pub provides_extras: Vec<ExtraName>,
    pub dependency_groups: BTreeMap<GroupName, Vec<uv_pypi_types::Requirement>>,
    pub dynamic: bool,
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
            dynamic: metadata.dynamic,
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
        cache: &WorkspaceCache,
    ) -> Result<Self, MetadataError> {
        // TODO(konsti): Cache workspace discovery.
        let discovery_options = DiscoveryOptions {
            stop_discovery_at: git_member.map(|git_member| {
                git_member
                    .fetch_root
                    .parent()
                    .expect("git checkout has a parent")
                    .to_path_buf()
            }),
            members: match sources {
                SourceStrategy::Enabled => MemberDiscovery::default(),
                SourceStrategy::Disabled => MemberDiscovery::None,
            },
        };
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(install_path, &discovery_options, cache)
                .await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(metadata, &project_workspace, git_member, locations, sources)
    }

    fn from_project_workspace(
        metadata: uv_pypi_types::RequiresDist,
        project_workspace: &ProjectWorkspace,
        git_member: Option<&GitWorkspaceMember<'_>>,
        locations: &IndexLocations,
        source_strategy: SourceStrategy,
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
                                Some(&metadata.name),
                                project_workspace.project_root(),
                                project_sources,
                                project_indexes,
                                extra,
                                Some(&group),
                                locations,
                                project_workspace.workspace(),
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
                        Some(&metadata.name),
                        project_workspace.project_root(),
                        project_sources,
                        project_indexes,
                        extra.as_deref(),
                        group,
                        locations,
                        project_workspace.workspace(),
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
                    if !metadata.provides_extras.contains(extra) {
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
            dynamic: metadata.dynamic,
        }
    }
}

/// Like [`uv_pypi_types::RequiresDist`], but with any recursive (or self-referential) dependencies
/// resolved.
///
/// For example, given:
/// ```toml
/// [project]
/// name = "example"
/// version = "0.1.0"
/// requires-python = ">=3.13.0"
/// dependencies = []
///
/// [project.optional-dependencies]
/// all = [
///     "example[async]",
/// ]
/// async = [
///     "fastapi",
/// ]
/// ```
///
/// A build backend could return:
/// ```txt
/// Metadata-Version: 2.2
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: example[async]; extra == "all"
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == "async"
/// ```
///
/// Or:
/// ```txt
/// Metadata-Version: 2.4
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: fastapi; extra == 'all'
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == 'async'
/// ```
///
/// The [`FlatRequiresDist`] struct is used to flatten out the recursive dependencies, i.e., convert
/// from the former to the latter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRequiresDist(Vec<uv_pypi_types::Requirement>);

impl FlatRequiresDist {
    /// Flatten a set of requirements, resolving any self-references.
    pub fn from_requirements(
        requirements: Vec<uv_pypi_types::Requirement>,
        name: &PackageName,
    ) -> Self {
        // If there are no self-references, we can return early.
        if requirements.iter().all(|req| req.name != *name) {
            return Self(requirements);
        }

        // Memoize the top level extras, in the same order as `requirements`
        let top_level_extras: Vec<_> = requirements
            .iter()
            .map(|req| req.marker.top_level_extra_name())
            .collect();

        // Transitively process all extras that are recursively included.
        let mut flattened = requirements.clone();
        let mut seen = FxHashSet::<(ExtraName, MarkerTree)>::default();
        let mut queue: VecDeque<_> = flattened
            .iter()
            .filter(|req| req.name == *name)
            .flat_map(|req| req.extras.iter().cloned().map(|extra| (extra, req.marker)))
            .collect();
        while let Some((extra, marker)) = queue.pop_front() {
            if !seen.insert((extra.clone(), marker)) {
                continue;
            }

            // Find the requirements for the extra.
            for (requirement, top_level_extra) in requirements.iter().zip(top_level_extras.iter()) {
                if top_level_extra.as_deref() != Some(&extra) {
                    continue;
                }
                let requirement = {
                    let mut marker = marker;
                    marker.and(requirement.marker);
                    uv_pypi_types::Requirement {
                        name: requirement.name.clone(),
                        extras: requirement.extras.clone(),
                        groups: requirement.groups.clone(),
                        source: requirement.source.clone(),
                        origin: requirement.origin.clone(),
                        marker: marker.simplify_extras(slice::from_ref(&extra)),
                    }
                };
                if requirement.name == *name {
                    // Add each transitively included extra.
                    queue.extend(
                        requirement
                            .extras
                            .iter()
                            .cloned()
                            .map(|extra| (extra, requirement.marker)),
                    );
                } else {
                    // Add the requirements for that extra.
                    flattened.push(requirement);
                }
            }
        }

        // Drop all the self-references now that we've flattened them out.
        flattened.retain(|req| req.name != *name);

        // Retain any self-constraints for that extra, e.g., if `project[foo]` includes
        // `project[bar]>1.0`, as a dependency, we need to propagate `project>1.0`, in addition to
        // transitively expanding `project[bar]`.
        for req in &requirements {
            if req.name == *name {
                if !req.source.is_empty() {
                    flattened.push(uv_pypi_types::Requirement {
                        name: req.name.clone(),
                        extras: vec![],
                        groups: req.groups.clone(),
                        source: req.source.clone(),
                        origin: req.origin.clone(),
                        marker: req.marker,
                    });
                }
            }
        }

        Self(flattened)
    }

    /// Consume the [`FlatRequiresDist`] and return the inner vector.
    pub fn into_inner(self) -> Vec<uv_pypi_types::Requirement> {
        self.0
    }
}

impl IntoIterator for FlatRequiresDist {
    type Item = uv_pypi_types::Requirement;
    type IntoIter = std::vec::IntoIter<uv_pypi_types::Requirement>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::str::FromStr;

    use anyhow::Context;
    use indoc::indoc;
    use insta::assert_snapshot;

    use uv_configuration::SourceStrategy;
    use uv_distribution_types::IndexLocations;
    use uv_normalize::PackageName;
    use uv_pep508::Requirement;
    use uv_workspace::pyproject::PyProjectToml;
    use uv_workspace::{DiscoveryOptions, ProjectWorkspace, WorkspaceCache};

    use crate::metadata::requires_dist::FlatRequiresDist;
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
                stop_discovery_at: Some(path.to_path_buf()),
                ..DiscoveryOptions::default()
            },
            &WorkspaceCache::default(),
        )
        .await?;
        let requires_dist = uv_pypi_types::RequiresDist::parse_pyproject_toml(contents)?;
        Ok(RequiresDist::from_project_workspace(
            requires_dist,
            &project_workspace,
            None,
            &IndexLocations::default(),
            SourceStrategy::default(),
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
        unknown field `ref`, expected one of `git`, `subdirectory`, `rev`, `tag`, `branch`, `url`, `path`, `editable`, `package`, `index`, `workspace`, `marker`, `extra`, `group`
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
          Caused by: `tqdm` references a workspace in `tool.uv.sources` (e.g., `tqdm = { workspace = true }`), but is not a workspace member
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
          Caused by: `tqdm` references a workspace in `tool.uv.sources` (e.g., `tqdm = { workspace = true }`), but is not a workspace member
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

    #[test]
    fn test_flat_requires_dist_noop() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
        ]);

        let actual = FlatRequiresDist::from_requirements(requirements, &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_basic() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[dev]; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'test'")
                .unwrap()
                .into(),
        ]);

        let actual = FlatRequiresDist::from_requirements(requirements, &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_with_markers() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[dev]; extra == 'test' and sys_platform == 'win32'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'")
                .unwrap()
                .into(),
        ];

        let expected = FlatRequiresDist(vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'test' and sys_platform == 'win32'")
                .unwrap()
                .into(),
        ]);

        let actual = FlatRequiresDist::from_requirements(requirements, &name);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_self_constraint() {
        let name = PackageName::from_str("pkg").unwrap();
        let requirements = vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg[async]==1.0.0").unwrap().into(),
        ];

        let expected = FlatRequiresDist(vec![
            Requirement::from_str("requests>=2.0.0").unwrap().into(),
            Requirement::from_str("pytest; extra == 'test'")
                .unwrap()
                .into(),
            Requirement::from_str("black; extra == 'dev'")
                .unwrap()
                .into(),
            Requirement::from_str("pkg==1.0.0").unwrap().into(),
        ]);

        let actual = FlatRequiresDist::from_requirements(requirements, &name);

        assert_eq!(actual, expected);
    }
}
