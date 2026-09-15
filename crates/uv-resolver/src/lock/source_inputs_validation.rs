use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use tracing::debug;

use uv_configuration::{BuildOptions, Constraints, Excludes, Overrides};
use uv_distribution::{DistributionDatabase, Metadata};
use uv_distribution_types::{
    ArchiveHashPolicy, DependencyMetadata, Dist, Identifier, Requirement, RequirementSource,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Tags;
use uv_pypi_types::PyProjectToml;
use uv_types::{BuildContext, HashStrategy, RequestedRequirements};
use uv_workspace::WorkspaceMember;

use crate::{InMemoryIndex, MetadataResponse, ResolverEnvironment, SourceDiscovery, SourceInput};

use super::source_inputs::SourceInputs;
use super::{Lock, LockError, LockErrorKind, PackageId, Source};

impl Lock {
    /// Replay source discovery from current roots, independently of the selected dependency graph.
    ///
    /// Local metadata is refreshed even when its package is absent from the selected dependency
    /// graph. Remote metadata is reused only after the current traversal reaches its recorded source.
    pub(super) async fn validate_source_inputs<Context: BuildContext>(
        &self,
        root: &Path,
        roots: &[Requirement],
        member_names: &BTreeMap<PackageName, WorkspaceMember>,
        constraints: &Constraints,
        overrides: &Overrides,
        excludes: &Excludes,
        dependency_metadata: &DependencyMetadata,
        tags: &Tags,
        markers: &MarkerEnvironment,
        build_options: &BuildOptions,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, Context>,
        allow_missing_package_metadata: bool,
    ) -> Result<Option<(Vec<RequestedRequirements>, Vec<SourceInput>)>, LockError> {
        let Some(expected) = self.source_inputs.as_ref() else {
            return Ok(None);
        };
        let mut metadata_cache = FxHashMap::default();
        let mut roots = roots.to_vec();

        // Member roots precede group and script declarations. Only those original member requests
        // expand all extras; another declaration of the same source retains its requested extras.
        let mut expanded_members = FxHashSet::default();
        for requirement in &mut roots {
            let Some(member) = member_names.get(&requirement.name) else {
                continue;
            };
            let RequirementSource::Directory { install_path, .. } = &requirement.source else {
                continue;
            };
            if install_path.as_ref() != member.root()
                || !requirement.extras.is_empty()
                || !requirement.groups.is_empty()
                || !requirement.marker.is_true()
                || requirement.origin.is_some()
                || !expanded_members.insert(requirement.name.clone())
            {
                continue;
            }
            let Some(metadata) = self
                .source_input_metadata(
                    requirement,
                    root,
                    dependency_metadata,
                    overrides.has_scoped_package(&requirement.name)
                        || excludes.has_scoped_package(&requirement.name),
                    tags,
                    markers,
                    build_options,
                    hasher,
                    index,
                    database,
                )
                .await?
            else {
                return Ok(None);
            };
            if metadata.name != requirement.name {
                return Ok(None);
            }
            let mut extras = metadata.provides_extra.clone();
            extras.sort_unstable();
            requirement.extras = extras;
            metadata_cache.insert(
                (requirement.name.clone(), requirement.source.clone()),
                metadata,
            );
        }

        let env = ResolverEnvironment::universal(Vec::new());
        let mut discovery = SourceDiscovery::new(
            &roots,
            constraints,
            overrides,
            excludes,
            dependency_metadata,
            hasher,
            &env,
        );
        let mut lookaheads = Vec::new();
        while let Some(requirement) = discovery.next_requirement() {
            let key = (requirement.name.clone(), requirement.source.clone());
            let metadata = if let Some(metadata) = metadata_cache.get(&key) {
                metadata.clone()
            } else {
                let Some(metadata) = self
                    .source_input_metadata(
                        &requirement,
                        root,
                        dependency_metadata,
                        overrides.has_scoped_package(&requirement.name)
                            || excludes.has_scoped_package(&requirement.name),
                        tags,
                        markers,
                        build_options,
                        discovery.hasher(),
                        index,
                        database,
                    )
                    .await?
                else {
                    return Ok(None);
                };
                metadata_cache.insert(key, metadata.clone());
                metadata
            };

            if metadata.name != requirement.name {
                return Ok(None);
            }

            lookaheads.push(
                discovery
                    .visit(requirement, metadata)
                    .map_err(LockErrorKind::SourceInputHash)?,
            );
        }

        let (inputs, _) = discovery.into_parts();
        if !allow_missing_package_metadata {
            // Selected package declarations are validated against their package entries. Additional
            // mutable providers retain fingerprints so their declarations are checked as well.
            let actual = SourceInputs::from_inputs(&inputs, self, root)?;
            let actual = actual
                .packages
                .iter()
                .filter(|input| input.is_local_fingerprint())
                .collect::<Vec<_>>();
            let expected = expected
                .packages
                .iter()
                .filter(|input| input.is_local_fingerprint())
                .collect::<Vec<_>>();
            if actual.len() != expected.len() {
                debug!("The set of local source discovery inputs changed");
                return Ok(None);
            }
            for input in actual {
                let mut matching = None;
                for expected in &expected {
                    if expected.matches_requirement(&input.requirement, root)? {
                        matching = Some(*expected);
                        break;
                    }
                }
                if !matching.is_some_and(|expected| {
                    input.version == expected.version && input.metadata == expected.metadata
                }) {
                    debug!(
                        "Local source discovery input changed for `{}`",
                        input.requirement
                    );
                    return Ok(None);
                }
            }
        }
        Ok(Some((lookaheads, inputs)))
    }

    /// Load current local metadata or the recorded metadata for an exact remote source.
    async fn source_input_metadata<Context: BuildContext>(
        &self,
        requirement: &Requirement,
        root: &Path,
        dependency_metadata: &DependencyMetadata,
        needs_version: bool,
        tags: &Tags,
        markers: &MarkerEnvironment,
        build_options: &BuildOptions,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<Option<Metadata>, LockError> {
        match &requirement.source {
            RequirementSource::Registry { .. } => return Ok(None),
            RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. } => {
                if let Some(inputs) = &self.source_inputs {
                    for input in &inputs.packages {
                        if input.matches_requirement(requirement, root)? {
                            if let RequirementSource::Url { .. } = &requirement.source
                                && !database.client().unmanaged.connectivity().is_offline()
                            {
                                for package in self.packages_for_name(&requirement.name) {
                                    if package
                                        .id
                                        .source
                                        .satisfies_requirement_source(&requirement.source, root)?
                                    {
                                        return Self::package_metadata(
                                            package,
                                            root,
                                            tags,
                                            markers,
                                            build_options,
                                            hasher,
                                            index,
                                            database,
                                        )
                                        .await
                                        .map(Some);
                                    }
                                }
                            }
                            return input.metadata(self, root);
                        }
                    }
                }
                debug!("No recorded source discovery metadata for `{requirement}`");
                return Ok(None);
            }
            RequirementSource::Path { .. } | RequirementSource::Directory { .. } => {}
        }

        let Some(url) = requirement.source.to_verbatim_parsed_url() else {
            return Ok(None);
        };
        let dist = Dist::from_url(requirement.name.clone(), url)
            .map_err(LockErrorKind::InvalidSourceRequirement)?;
        let package_id = PackageId {
            name: requirement.name.clone(),
            version: None,
            source: Source::from_dist(&dist, root)?,
        };
        if let RequirementSource::Directory { install_path, .. } = &requirement.source {
            let mut recorded_version = None;
            if !needs_version
                && !dependency_metadata
                    .values()
                    .any(|metadata| metadata.name == requirement.name)
                && let Some(inputs) = &self.source_inputs
            {
                for input in &inputs.packages {
                    if input.matches_requirement(requirement, root)? {
                        recorded_version = Some(&input.version);
                        break;
                    }
                }
            }
            if let Some(metadata) = Self::source_input_static_metadata(
                install_path,
                &package_id,
                dependency_metadata,
                recorded_version,
                database,
            )
            .await?
            {
                return Ok(Some(metadata));
            }
        }

        // Selected local artifacts retain the archive-hash checks recorded on their package entry.
        if let Some(package) = self.packages.iter().find(|package| {
            package.id.name == package_id.name && package.id.source == package_id.source
        }) {
            return Self::package_metadata(
                package,
                root,
                tags,
                markers,
                build_options,
                hasher,
                index,
                database,
            )
            .await
            .map(Some);
        }

        let id = dist.distribution_id();
        let hashes = hasher.metadata_policy(&dist);
        if let Some(response) = index.distributions().get(&id)
            && let MetadataResponse::Found(archive) = response.as_ref()
            && ArchiveHashPolicy::from(hashes.validation).matches(archive.hashes.as_slice())
        {
            return Ok(Some(archive.metadata.clone()));
        }
        let archive = database
            .get_or_build_wheel_metadata(&dist, hashes)
            .await
            .map_err(|err| LockErrorKind::Resolution {
                id: package_id,
                err,
            })?;
        let metadata = archive.metadata.clone();
        index
            .distributions()
            .done(id, Arc::new(MetadataResponse::Found(archive)));
        Ok(Some(metadata))
    }

    /// Read complete static source metadata without requiring permission to run a build backend.
    async fn source_input_static_metadata<Context: BuildContext>(
        source_tree: &Path,
        package_id: &PackageId,
        dependency_metadata: &DependencyMetadata,
        recorded_version: Option<&Version>,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<Option<Metadata>, LockError> {
        let path = source_tree.join("pyproject.toml");
        let contents = match fs_err::tokio::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(LockErrorKind::UnreadablePyprojectToml { path, err }.into()),
        };
        let pyproject_toml =
            PyProjectToml::from_toml(&contents, path.user_display()).map_err(|err| {
                LockErrorKind::InvalidPyprojectToml {
                    path: path.clone(),
                    err,
                }
            })?;
        let Some(project) = pyproject_toml.project.as_ref() else {
            return Ok(None);
        };
        // A dynamic version does not affect static dependency discovery unless a scoped policy
        // uses it. In that case the caller withholds the recorded version and metadata is rebuilt.
        let version = project.version.clone().or_else(|| {
            project
                .dynamic
                .as_ref()
                .filter(|dynamic| dynamic.iter().any(|field| field == "version"))
                .and(recorded_version)
                .cloned()
        });
        let Some(version) = version else {
            return Ok(None);
        };
        // The distribution database applies explicit metadata overrides before inspecting a source.
        if dependency_metadata
            .get(&package_id.name, Some(&version))
            .is_some()
        {
            return Ok(None);
        }
        let requires_python = match pyproject_toml.requires_python() {
            Ok(requires_python) => requires_python,
            Err(
                uv_pypi_types::MetadataError::FieldNotFound("project")
                | uv_pypi_types::MetadataError::DynamicField("requires-python"),
            ) => return Ok(None),
            Err(err) => {
                return Err(LockErrorKind::InvalidPyprojectToml { path, err }.into());
            }
        };
        let metadata = database
            .requires_dist(source_tree, &pyproject_toml)
            .await
            .map_err(|err| LockErrorKind::Resolution {
                id: package_id.clone(),
                err,
            })?;
        Ok(metadata.map(|metadata| {
            Metadata {
                name: metadata.name,
                version,
                requires_dist: metadata.requires_dist,
                requires_python,
                provides_extra: metadata.provides_extra,
                dependency_groups: metadata.dependency_groups,
                dynamic: metadata.dynamic,
            }
            .with_force_relative(true)
        }))
    }
}
