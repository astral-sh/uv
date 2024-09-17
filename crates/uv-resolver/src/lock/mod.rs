use either::Either;
use itertools::Itertools;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::convert::Infallible;
use std::fmt::{Debug, Display};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;
use toml_edit::{value, Array, ArrayOfTables, InlineTable, Item, Table, Value};
use url::Url;

use cache_key::RepositoryUrl;
use distribution_filename::{DistExtension, ExtensionError, SourceDistExtension, WheelFilename};
use distribution_types::{
    BuiltDist, DirectUrlBuiltDist, DirectUrlSourceDist, DirectorySourceDist, Dist,
    DistributionMetadata, FileLocation, FlatIndexLocation, GitSourceDist, HashPolicy,
    IndexLocations, IndexUrl, Name, PathBuiltDist, PathSourceDist, RegistryBuiltDist,
    RegistryBuiltWheel, RegistrySourceDist, RemoteSource, Resolution, ResolvedDist, ToUrlError,
    UrlString,
};
use pep440_rs::Version;
use pep508_rs::{split_scheme, MarkerEnvironment, MarkerTree, VerbatimUrl, VerbatimUrlError};
use platform_tags::{TagCompatibility, TagPriority, Tags};
use pypi_types::{
    redact_git_credentials, HashDigest, ParsedArchiveUrl, ParsedGitUrl, Requirement,
    RequirementSource, ResolverMarkerEnvironment,
};
use uv_configuration::{BuildOptions, DevSpecification, ExtrasSpecification, InstallOptions};
use uv_distribution::DistributionDatabase;
use uv_fs::{relative_to, PortablePath, PortablePathBuf};
use uv_git::{GitReference, GitSha, RepositoryReference, ResolvedRepositoryReference};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_types::BuildContext;
use uv_workspace::{InstallTarget, Workspace};

pub use crate::lock::requirements_txt::RequirementsTxtExport;
pub use crate::lock::tree::TreeDisplay;
use crate::requires_python::SimplifiedMarkerTree;
use crate::resolution::{AnnotatedDist, ResolutionGraphNode};
use crate::{ExcludeNewer, PrereleaseMode, RequiresPython, ResolutionGraph, ResolutionMode};

mod requirements_txt;
mod tree;

/// The current version of the lockfile format.
const VERSION: u32 = 1;

static LINUX_MARKERS: LazyLock<MarkerTree> = LazyLock::new(|| {
    MarkerTree::from_str(
        "platform_system == 'Linux' and os_name == 'posix' and sys_platform == 'linux'",
    )
    .unwrap()
});
static WINDOWS_MARKERS: LazyLock<MarkerTree> = LazyLock::new(|| {
    MarkerTree::from_str(
        "platform_system == 'Windows' and os_name == 'nt' and sys_platform == 'win32'",
    )
    .unwrap()
});
static MAC_MARKERS: LazyLock<MarkerTree> = LazyLock::new(|| {
    MarkerTree::from_str(
        "platform_system == 'Darwin' and os_name == 'posix' and sys_platform == 'darwin'",
    )
    .unwrap()
});

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "LockWire")]
pub struct Lock {
    version: u32,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    fork_markers: Vec<MarkerTree>,
    /// The list of supported environments specified by the user.
    supported_environments: Vec<MarkerTree>,
    /// The range of supported Python versions.
    requires_python: RequiresPython,
    /// We discard the lockfile if these options don't match.
    options: ResolverOptions,
    /// The actual locked version and their metadata.
    packages: Vec<Package>,
    /// A map from package ID to index in `packages`.
    ///
    /// This can be used to quickly lookup the full package for any ID
    /// in this lock. For example, the dependencies for each package are
    /// listed as package IDs. This map can be used to find the full
    /// package for each such dependency.
    ///
    /// It is guaranteed that every package in this lock has an entry in
    /// this map, and that every dependency for every package has an ID
    /// that exists in this map. That is, there are no dependencies that don't
    /// have a corresponding locked package entry in the same lockfile.
    by_id: FxHashMap<PackageId, usize>,
    /// The input requirements to the resolution.
    manifest: ResolverManifest,
}

impl Lock {
    /// Initialize a [`Lock`] from a [`ResolutionGraph`].
    pub fn from_resolution_graph(graph: &ResolutionGraph, root: &Path) -> Result<Self, LockError> {
        let mut packages = BTreeMap::new();
        let requires_python = graph.requires_python.clone();

        // Lock all base packages.
        for node_index in graph.petgraph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &graph.petgraph[node_index] else {
                continue;
            };
            if !dist.is_base() {
                continue;
            }
            let fork_markers = graph
                .fork_markers(dist.name(), &dist.version, dist.dist.version_or_url().url())
                .cloned()
                .unwrap_or_default();
            let mut package = Package::from_annotated_dist(dist, fork_markers, root)?;
            Self::remove_unreachable_wheels(graph, &requires_python, node_index, &mut package);

            // Add all dependencies
            for edge in graph.petgraph.edges(node_index) {
                let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                else {
                    continue;
                };
                let marker = edge.weight().clone();
                package.add_dependency(&requires_python, dependency_dist, marker, root)?;
            }

            let id = package.id.clone();
            if let Some(locked_dist) = packages.insert(id, package) {
                return Err(LockErrorKind::DuplicatePackage {
                    id: locked_dist.id.clone(),
                }
                .into());
            }
        }

        // Lock all extras and development dependencies.
        for node_index in graph.petgraph.node_indices() {
            let ResolutionGraphNode::Dist(dist) = &graph.petgraph[node_index] else {
                continue;
            };
            if let Some(extra) = dist.extra.as_ref() {
                let id = PackageId::from_annotated_dist(dist, root)?;
                let Some(package) = packages.get_mut(&id) else {
                    return Err(LockErrorKind::MissingExtraBase {
                        id,
                        extra: extra.clone(),
                    }
                    .into());
                };
                for edge in graph.petgraph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = edge.weight().clone();
                    package.add_optional_dependency(
                        &requires_python,
                        extra.clone(),
                        dependency_dist,
                        marker,
                        root,
                    )?;
                }
            }
            if let Some(group) = dist.dev.as_ref() {
                let id = PackageId::from_annotated_dist(dist, root)?;
                let Some(package) = packages.get_mut(&id) else {
                    return Err(LockErrorKind::MissingDevBase {
                        id,
                        group: group.clone(),
                    }
                    .into());
                };
                for edge in graph.petgraph.edges(node_index) {
                    let ResolutionGraphNode::Dist(dependency_dist) = &graph.petgraph[edge.target()]
                    else {
                        continue;
                    };
                    let marker = edge.weight().clone();
                    package.add_dev_dependency(
                        &requires_python,
                        group.clone(),
                        dependency_dist,
                        marker,
                        root,
                    )?;
                }
            }
        }

        let packages = packages.into_values().collect();
        let options = ResolverOptions {
            resolution_mode: graph.options.resolution_mode,
            prerelease_mode: graph.options.prerelease_mode,
            exclude_newer: graph.options.exclude_newer,
        };
        let lock = Self::new(
            VERSION,
            packages,
            requires_python,
            options,
            ResolverManifest::default(),
            vec![],
            graph.fork_markers.clone(),
        )?;
        Ok(lock)
    }

    /// Remove wheels that can't be selected for installation due to environment markers.
    ///
    /// For example, a package included under `sys_platform == 'win32'` does not need Linux
    /// wheels.
    fn remove_unreachable_wheels(
        graph: &ResolutionGraph,
        requires_python: &RequiresPython,
        node_index: NodeIndex,
        locked_dist: &mut Package,
    ) {
        // Remove wheels that don't match `requires-python` and can't be selected for installation.
        locked_dist
            .wheels
            .retain(|wheel| requires_python.matches_wheel_tag(&wheel.filename));

        // Filter by platform tags.

        // See https://github.com/pypi/warehouse/blob/ccff64920db7965078cf1fdb50f028e640328887/warehouse/forklift/legacy.py#L100-L169
        // for a list of relevant platforms.
        let linux_tags = [
            "manylinux1_",
            "manylinux2010_",
            "manylinux2014_",
            "musllinux_",
            "manylinux_",
        ];
        let windows_tags = ["win32", "win_arm64", "win_amd64", "win_ia64"];

        locked_dist.wheels.retain(|wheel| {
            // Naively, we'd check whether `platform_system == 'Linux'` is disjoint, or
            // `os_name == 'posix'` is disjoint, or `sys_platform == 'linux'` is disjoint (each on its
            // own sufficient to exclude linux wheels), but due to
            // `(A ∩ (B ∩ C) = ∅) => ((A ∩ B = ∅) or (A ∩ C = ∅))`
            // a single disjointness check with the intersection is sufficient, so we have one
            // constant per platform.
            let platform_tags = &wheel.filename.platform_tag;
            if platform_tags.iter().all(|tag| {
                linux_tags.into_iter().any(|linux_tag| {
                    // These two linux tags are allowed by warehouse.
                    tag.starts_with(linux_tag) || tag == "linux_armv6l" || tag == "linux_armv7l"
                })
            }) {
                !graph.petgraph[node_index]
                    .marker()
                    .is_disjoint(&LINUX_MARKERS)
            } else if platform_tags
                .iter()
                .all(|tag| windows_tags.contains(&&**tag))
            {
                !graph.petgraph[node_index]
                    .marker()
                    .is_disjoint(&WINDOWS_MARKERS)
            } else if platform_tags.iter().all(|tag| tag.starts_with("macosx_")) {
                !graph.petgraph[node_index]
                    .marker()
                    .is_disjoint(&MAC_MARKERS)
            } else {
                true
            }
        });
    }

    /// Initialize a [`Lock`] from a list of [`Package`] entries.
    fn new(
        version: u32,
        mut packages: Vec<Package>,
        requires_python: RequiresPython,
        options: ResolverOptions,
        manifest: ResolverManifest,
        supported_environments: Vec<MarkerTree>,
        fork_markers: Vec<MarkerTree>,
    ) -> Result<Self, LockError> {
        // Put all dependencies for each package in a canonical order and
        // check for duplicates.
        for package in &mut packages {
            package.dependencies.sort();
            for windows in package.dependencies.windows(2) {
                let (dep1, dep2) = (&windows[0], &windows[1]);
                if dep1 == dep2 {
                    return Err(LockErrorKind::DuplicateDependency {
                        id: package.id.clone(),
                        dependency: dep1.clone(),
                    }
                    .into());
                }
            }

            // Perform the same validation for optional dependencies.
            for (extra, dependencies) in &mut package.optional_dependencies {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateOptionalDependency {
                            id: package.id.clone(),
                            extra: extra.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for (group, dependencies) in &mut package.dev_dependencies {
                dependencies.sort();
                for windows in dependencies.windows(2) {
                    let (dep1, dep2) = (&windows[0], &windows[1]);
                    if dep1 == dep2 {
                        return Err(LockErrorKind::DuplicateDevDependency {
                            id: package.id.clone(),
                            group: group.clone(),
                            dependency: dep1.clone(),
                        }
                        .into());
                    }
                }
            }
        }
        packages.sort_by(|dist1, dist2| dist1.id.cmp(&dist2.id));

        // Check for duplicate package IDs and also build up the map for
        // packages keyed by their ID.
        let mut by_id = FxHashMap::default();
        for (i, dist) in packages.iter().enumerate() {
            if by_id.insert(dist.id.clone(), i).is_some() {
                return Err(LockErrorKind::DuplicatePackage {
                    id: dist.id.clone(),
                }
                .into());
            }
        }

        // Build up a map from ID to extras.
        let mut extras_by_id = FxHashMap::default();
        for dist in &packages {
            for extra in dist.optional_dependencies.keys() {
                extras_by_id
                    .entry(dist.id.clone())
                    .or_insert_with(FxHashSet::default)
                    .insert(extra.clone());
            }
        }

        // Remove any non-existent extras (e.g., extras that were requested but don't exist).
        for dist in &mut packages {
            for dep in dist
                .dependencies
                .iter_mut()
                .chain(dist.optional_dependencies.values_mut().flatten())
                .chain(dist.dev_dependencies.values_mut().flatten())
            {
                dep.extra.retain(|extra| {
                    extras_by_id
                        .get(&dep.package_id)
                        .is_some_and(|extras| extras.contains(extra))
                });
            }
        }

        // Check that every dependency has an entry in `by_id`. If any don't,
        // it implies we somehow have a dependency with no corresponding locked
        // package.
        for dist in &packages {
            for dep in &dist.dependencies {
                if !by_id.contains_key(&dep.package_id) {
                    return Err(LockErrorKind::UnrecognizedDependency {
                        id: dist.id.clone(),
                        dependency: dep.clone(),
                    }
                    .into());
                }
            }

            // Perform the same validation for optional dependencies.
            for dependencies in dist.optional_dependencies.values() {
                for dep in dependencies {
                    if !by_id.contains_key(&dep.package_id) {
                        return Err(LockErrorKind::UnrecognizedDependency {
                            id: dist.id.clone(),
                            dependency: dep.clone(),
                        }
                        .into());
                    }
                }
            }

            // Perform the same validation for dev dependencies.
            for dependencies in dist.dev_dependencies.values() {
                for dep in dependencies {
                    if !by_id.contains_key(&dep.package_id) {
                        return Err(LockErrorKind::UnrecognizedDependency {
                            id: dist.id.clone(),
                            dependency: dep.clone(),
                        }
                        .into());
                    }
                }
            }

            // Also check that our sources are consistent with whether we have
            // hashes or not.
            if let Some(requires_hash) = dist.id.source.requires_hash() {
                for wheel in &dist.wheels {
                    if requires_hash != wheel.hash.is_some() {
                        return Err(LockErrorKind::Hash {
                            id: dist.id.clone(),
                            artifact_type: "wheel",
                            expected: requires_hash,
                        }
                        .into());
                    }
                }
            }
        }
        Ok(Self {
            version,
            fork_markers,
            supported_environments,
            requires_python,
            options,
            packages,
            by_id,
            manifest,
        })
    }

    /// Record the requirements that were used to generate this lock.
    #[must_use]
    pub fn with_manifest(mut self, manifest: ResolverManifest) -> Self {
        self.manifest = manifest;
        self
    }

    /// Record the supported environments that were used to generate this lock.
    #[must_use]
    pub fn with_supported_environments(mut self, supported_environments: Vec<MarkerTree>) -> Self {
        // We "complexify" the markers given, since the supported
        // environments given might be coming directly from what's written in
        // `pyproject.toml`, and those are assumed to be simplified (i.e.,
        // they assume `requires-python` is true). But a `Lock` always uses
        // non-simplified markers internally, so we need to re-complexify them
        // here.
        //
        // The nice thing about complexifying is that it's a no-op if the
        // markers given have already been complexified.
        self.supported_environments = supported_environments
            .into_iter()
            .map(|marker| self.requires_python.complexify_markers(marker))
            .collect();
        self
    }

    /// Returns the number of packages in the lockfile.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Returns `true` if the lockfile contains no packages.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Returns the [`Package`] entries in this lock.
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns the supported Python version range for the lockfile, if present.
    pub fn requires_python(&self) -> &RequiresPython {
        &self.requires_python
    }

    /// Returns the resolution mode used to generate this lock.
    pub fn resolution_mode(&self) -> ResolutionMode {
        self.options.resolution_mode
    }

    /// Returns the pre-release mode used to generate this lock.
    pub fn prerelease_mode(&self) -> PrereleaseMode {
        self.options.prerelease_mode
    }

    /// Returns the exclude newer setting used to generate this lock.
    pub fn exclude_newer(&self) -> Option<ExcludeNewer> {
        self.options.exclude_newer
    }

    /// Returns the supported environments that were used to generate this lock.
    pub fn supported_environments(&self) -> &[MarkerTree] {
        &self.supported_environments
    }

    /// Returns the workspace members that were used to generate this lock.
    pub fn members(&self) -> &BTreeSet<PackageName> {
        &self.manifest.members
    }

    /// Returns the supported environments that were used to generate this
    /// lock.
    ///
    /// The markers returned here are "simplified" with respect to the lock
    /// file's `requires-python` setting. This means these should only be used
    /// for direct comparison purposes with the supported environments written
    /// by a human in `pyproject.toml`. (Think of "supported environments" in
    /// `pyproject.toml` as having an implicit `and python_full_version >=
    /// '{requires-python-bound}'` attached to each one.)
    pub fn simplified_supported_environments(&self) -> Vec<MarkerTree> {
        self.supported_environments()
            .iter()
            .map(|marker| self.requires_python.simplify_markers(marker.clone()))
            .collect()
    }

    /// If this lockfile was built from a forking resolution with non-identical forks, return the
    /// markers of those forks, otherwise `None`.
    pub fn fork_markers(&self) -> &[MarkerTree] {
        self.fork_markers.as_slice()
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        project: InstallTarget<'_>,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecification,
        dev: DevSpecification<'_>,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();

        // Add the workspace packages to the queue.
        for root_name in project.packages() {
            let root = self
                .find_by_name(root_name)
                .map_err(|_| LockErrorKind::MultipleRootPackages {
                    name: root_name.clone(),
                })?
                .ok_or_else(|| LockErrorKind::MissingRootPackage {
                    name: root_name.clone(),
                })?;

            if dev.prod() {
                // Add the base package.
                queue.push_back((root, None));

                // Add any extras.
                match extras {
                    ExtrasSpecification::None => {}
                    ExtrasSpecification::All => {
                        for extra in root.optional_dependencies.keys() {
                            queue.push_back((root, Some(extra)));
                        }
                    }
                    ExtrasSpecification::Some(extras) => {
                        for extra in extras {
                            queue.push_back((root, Some(extra)));
                        }
                    }
                }
            }

            // Add any dev dependencies.
            for group in dev.iter() {
                for dep in root.dev_dependencies.get(group).into_iter().flatten() {
                    if dep.complexified_marker.evaluate(marker_env, &[]) {
                        let dep_dist = self.find_by_id(&dep.package_id);
                        if seen.insert((&dep.package_id, None)) {
                            queue.push_back((dep_dist, None));
                        }
                        for extra in &dep.extra {
                            if seen.insert((&dep.package_id, Some(extra))) {
                                queue.push_back((dep_dist, Some(extra)));
                            }
                        }
                    }
                }
            }
        }

        // Add any dependency groups that are exclusive to the workspace root (e.g., dev
        // dependencies in (legacy) non-project workspace roots).
        for group in dev.iter() {
            for dependency in project.group(group) {
                if dependency.marker.evaluate(marker_env, &[]) {
                    let root_name = &dependency.name;
                    let root = self
                        .find_by_markers(root_name, marker_env)
                        .map_err(|_| LockErrorKind::MultipleRootPackages {
                            name: root_name.clone(),
                        })?
                        .ok_or_else(|| LockErrorKind::MissingRootPackage {
                            name: root_name.clone(),
                        })?;

                    // Add the base package.
                    queue.push_back((root, None));

                    // Add any extras.
                    for extra in &dependency.extras {
                        queue.push_back((root, Some(extra)));
                    }
                }
            }
        }

        let mut map = BTreeMap::default();
        let mut hashes = BTreeMap::default();
        while let Some((dist, extra)) = queue.pop_front() {
            let deps = if let Some(extra) = extra {
                Either::Left(dist.optional_dependencies.get(extra).into_iter().flatten())
            } else {
                Either::Right(dist.dependencies.iter())
            };
            for dep in deps {
                if dep.complexified_marker.evaluate(marker_env, &[]) {
                    let dep_dist = self.find_by_id(&dep.package_id);
                    if seen.insert((&dep.package_id, None)) {
                        queue.push_back((dep_dist, None));
                    }
                    for extra in &dep.extra {
                        if seen.insert((&dep.package_id, Some(extra))) {
                            queue.push_back((dep_dist, Some(extra)));
                        }
                    }
                }
            }
            if install_options.include_package(
                &dist.id.name,
                project.project_name(),
                &self.manifest.members,
            ) {
                map.insert(
                    dist.id.name.clone(),
                    ResolvedDist::Installable(dist.to_dist(
                        project.workspace().install_path(),
                        TagPolicy::Required(tags),
                        build_options,
                    )?),
                );
                hashes.insert(dist.id.name.clone(), dist.hashes());
            }
        }
        let diagnostics = vec![];
        Ok(Resolution::new(map, hashes, diagnostics))
    }

    /// Returns the TOML representation of this lockfile.
    pub fn to_toml(&self) -> anyhow::Result<String> {
        // We construct a TOML document manually instead of going through Serde to enable
        // the use of inline tables.
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("version", value(i64::from(self.version)));

        doc.insert("requires-python", value(self.requires_python.to_string()));

        if !self.fork_markers.is_empty() {
            let fork_markers = each_element_on_its_line_array(
                self.fork_markers
                    .iter()
                    .map(|marker| SimplifiedMarkerTree::new(&self.requires_python, marker.clone()))
                    .filter_map(|marker| marker.try_to_string()),
            );
            doc.insert("resolution-markers", value(fork_markers));
        }

        if !self.supported_environments.is_empty() {
            let supported_environments = each_element_on_its_line_array(
                self.supported_environments
                    .iter()
                    .map(|marker| SimplifiedMarkerTree::new(&self.requires_python, marker.clone()))
                    .filter_map(|marker| marker.try_to_string()),
            );
            doc.insert("supported-markers", value(supported_environments));
        }

        // Write the settings that were used to generate the resolution.
        // This enables us to invalidate the lockfile if the user changes
        // their settings.
        {
            let mut options_table = Table::new();

            if self.options.resolution_mode != ResolutionMode::default() {
                options_table.insert(
                    "resolution-mode",
                    value(self.options.resolution_mode.to_string()),
                );
            }
            if self.options.prerelease_mode != PrereleaseMode::default() {
                options_table.insert(
                    "prerelease-mode",
                    value(self.options.prerelease_mode.to_string()),
                );
            }
            if let Some(exclude_newer) = self.options.exclude_newer {
                options_table.insert("exclude-newer", value(exclude_newer.to_string()));
            }

            if !options_table.is_empty() {
                doc.insert("options", Item::Table(options_table));
            }
        }

        // Write the manifest that was used to generate the resolution.
        {
            let mut manifest_table = Table::new();

            if !self.manifest.members.is_empty() {
                manifest_table.insert(
                    "members",
                    value(each_element_on_its_line_array(
                        self.manifest
                            .members
                            .iter()
                            .map(std::string::ToString::to_string),
                    )),
                );
            }

            if !self.manifest.requirements.is_empty() {
                let requirements = self
                    .manifest
                    .requirements
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let requirements = match requirements.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    requirements => each_element_on_its_line_array(requirements.iter()),
                };
                manifest_table.insert("requirements", value(requirements));
            }

            if !self.manifest.constraints.is_empty() {
                let constraints = self
                    .manifest
                    .constraints
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let constraints = match constraints.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    constraints => each_element_on_its_line_array(constraints.iter()),
                };
                manifest_table.insert("constraints", value(constraints));
            }

            if !self.manifest.overrides.is_empty() {
                let overrides = self
                    .manifest
                    .overrides
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let overrides = match overrides.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    overrides => each_element_on_its_line_array(overrides.iter()),
                };
                manifest_table.insert("overrides", value(overrides));
            }

            if !manifest_table.is_empty() {
                doc.insert("manifest", Item::Table(manifest_table));
            }
        }

        // Count the number of packages for each package name. When
        // there's only one package for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut dist_count_by_name: FxHashMap<PackageName, u64> = FxHashMap::default();
        for dist in &self.packages {
            *dist_count_by_name.entry(dist.id.name.clone()).or_default() += 1;
        }

        let mut packages = ArrayOfTables::new();
        for dist in &self.packages {
            packages.push(dist.to_toml(&self.requires_python, &dist_count_by_name)?);
        }

        doc.insert("package", Item::ArrayOfTables(packages));
        Ok(doc.to_string())
    }

    /// Returns the package with the given name. If there are multiple
    /// matching packages, then an error is returned. If there are no
    /// matching packages, then `Ok(None)` is returned.
    fn find_by_name(&self, name: &PackageName) -> Result<Option<&Package>, String> {
        let mut found_dist = None;
        for dist in &self.packages {
            if &dist.id.name == name {
                if found_dist.is_some() {
                    return Err(format!("found multiple packages matching `{name}`"));
                }
                found_dist = Some(dist);
            }
        }
        Ok(found_dist)
    }

    /// Returns the package with the given name.
    ///
    /// If there are multiple matching packages, returns the package that
    /// corresponds to the given marker tree.
    ///
    /// If there are multiple packages that are relevant to the current
    /// markers, then an error is returned.
    ///
    /// If there are no matching packages, then `Ok(None)` is returned.
    fn find_by_markers(
        &self,
        name: &PackageName,
        marker_env: &MarkerEnvironment,
    ) -> Result<Option<&Package>, String> {
        let mut found_dist = None;
        for dist in &self.packages {
            if &dist.id.name == name {
                if dist.fork_markers.is_empty()
                    || dist
                        .fork_markers
                        .iter()
                        .any(|marker| marker.evaluate(marker_env, &[]))
                {
                    if found_dist.is_some() {
                        return Err(format!("found multiple packages matching `{name}`"));
                    }
                    found_dist = Some(dist);
                }
            }
        }
        Ok(found_dist)
    }

    fn find_by_id(&self, id: &PackageId) -> &Package {
        let index = *self.by_id.get(id).expect("locked package for ID");
        let dist = self.packages.get(index).expect("valid index for package");
        dist
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub async fn satisfies<Context: BuildContext>(
        &self,
        workspace: &Workspace,
        members: &[PackageName],
        requirements: &[Requirement],
        constraints: &[Requirement],
        overrides: &[Requirement],
        indexes: Option<&IndexLocations>,
        build_options: &BuildOptions,
        tags: &Tags,
        database: &DistributionDatabase<'_, Context>,
    ) -> Result<SatisfiesResult<'_>, LockError> {
        let mut queue: VecDeque<&Package> = VecDeque::new();
        let mut seen = FxHashSet::default();

        // Validate that the lockfile was generated with the same root members.
        {
            let expected = members.iter().cloned().collect::<BTreeSet<_>>();
            let actual = &self.manifest.members;
            if expected != *actual {
                return Ok(SatisfiesResult::MismatchedMembers(expected, actual));
            }
        }

        // Validate that the member sources have not changed.
        {
            // E.g., that they've switched from virtual to non-virtual or vice versa.
            for (name, member) in workspace.packages() {
                let expected = !member.pyproject_toml().is_package();
                let actual = self
                    .find_by_name(name)
                    .ok()
                    .flatten()
                    .map(|package| matches!(package.id.source, Source::Virtual(_)));
                if actual.map_or(true, |actual| actual != expected) {
                    return Ok(SatisfiesResult::MismatchedSources(name.clone(), expected));
                }
            }

            // E.g., that the version has changed.
            for (name, member) in workspace.packages() {
                let Some(expected) = member
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .and_then(|project| project.version.as_ref())
                else {
                    continue;
                };
                let actual = self
                    .find_by_name(name)
                    .ok()
                    .flatten()
                    .map(|package| &package.id.version);
                if actual.map_or(true, |actual| actual != expected) {
                    return Ok(SatisfiesResult::MismatchedVersion(
                        name.clone(),
                        expected.clone(),
                        actual.cloned(),
                    ));
                }
            }
        }

        // Validate that the lockfile was generated with the same requirements.
        {
            let expected: BTreeSet<_> = requirements
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .requirements
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedConstraints(expected, actual));
            }
        }

        // Validate that the lockfile was generated with the same constraints.
        {
            let expected: BTreeSet<_> = constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .constraints
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedConstraints(expected, actual));
            }
        }

        // Validate that the lockfile was generated with the same overrides.
        {
            let expected: BTreeSet<_> = overrides
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            let actual: BTreeSet<_> = self
                .manifest
                .overrides
                .iter()
                .cloned()
                .map(|requirement| normalize_requirement(requirement, workspace))
                .collect::<Result<_, _>>()?;
            if expected != actual {
                return Ok(SatisfiesResult::MismatchedOverrides(expected, actual));
            }
        }

        // Collect the set of available indexes (both `--index-url` and `--find-links` entries).
        let remotes = indexes.map(|locations| {
            locations
                .indexes()
                .filter_map(|index_url| match index_url {
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        Some(UrlString::from(index_url.redacted()))
                    }
                    IndexUrl::Path(_) => None,
                })
                .chain(
                    locations
                        .flat_index()
                        .filter_map(|index_url| match index_url {
                            FlatIndexLocation::Url(_) => {
                                Some(UrlString::from(index_url.redacted()))
                            }
                            FlatIndexLocation::Path(_) => None,
                        }),
                )
                .collect::<BTreeSet<_>>()
        });

        let locals = indexes.map(|locations| {
            locations
                .indexes()
                .filter_map(|index_url| match index_url {
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => None,
                    IndexUrl::Path(index_url) => {
                        let path = index_url.to_file_path().ok()?;
                        let path = relative_to(&path, workspace.install_path())
                            .or_else(|_| std::path::absolute(path))
                            .ok()?;
                        Some(path)
                    }
                })
                .chain(
                    locations
                        .flat_index()
                        .filter_map(|index_url| match index_url {
                            FlatIndexLocation::Url(_) => None,
                            FlatIndexLocation::Path(index_url) => {
                                let path = index_url.to_file_path().ok()?;
                                let path = relative_to(&path, workspace.install_path())
                                    .or_else(|_| std::path::absolute(path))
                                    .ok()?;
                                Some(path)
                            }
                        }),
                )
                .collect::<BTreeSet<_>>()
        });

        // Add the workspace packages to the queue.
        for root_name in workspace.packages().keys() {
            let root = self
                .find_by_name(root_name)
                .expect("found too many packages matching root");

            let Some(root) = root else {
                // The package is not in the lockfile, so it can't be satisfied.
                return Ok(SatisfiesResult::MissingRoot(root_name.clone()));
            };

            // Add the base package.
            queue.push_back(root);
        }

        while let Some(package) = queue.pop_front() {
            // If the lockfile references an index that was not provided, we can't validate it.
            if let Source::Registry(index) = &package.id.source {
                match index {
                    RegistrySource::Url(url) => {
                        if remotes
                            .as_ref()
                            .is_some_and(|remotes| !remotes.contains(url))
                        {
                            return Ok(SatisfiesResult::MissingRemoteIndex(
                                &package.id.name,
                                &package.id.version,
                                url,
                            ));
                        }
                    }
                    RegistrySource::Path(path) => {
                        if locals.as_ref().is_some_and(|locals| !locals.contains(path)) {
                            return Ok(SatisfiesResult::MissingLocalIndex(
                                &package.id.name,
                                &package.id.version,
                                path,
                            ));
                        }
                    }
                };
            }

            // If the package is immutable, we don't need to validate it (or its dependencies).
            if package.id.source.is_immutable() {
                continue;
            }

            // Get the metadata for the distribution.
            let dist = package.to_dist(
                workspace.install_path(),
                TagPolicy::Preferred(tags),
                build_options,
            )?;

            let Ok(archive) = database
                .get_or_build_wheel_metadata(&dist, HashPolicy::None)
                .await
            else {
                return Ok(SatisfiesResult::MissingMetadata(
                    &package.id.name,
                    &package.id.version,
                ));
            };

            // Validate the `requires-dist` metadata.
            {
                let expected: BTreeSet<_> = archive
                    .metadata
                    .requires_dist
                    .into_iter()
                    .map(|requirement| normalize_requirement(requirement, workspace))
                    .collect::<Result<_, _>>()?;
                let actual: BTreeSet<_> = package
                    .metadata
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(|requirement| normalize_requirement(requirement, workspace))
                    .collect::<Result<_, _>>()?;

                if expected != actual {
                    return Ok(SatisfiesResult::MismatchedRequiresDist(
                        &package.id.name,
                        &package.id.version,
                        expected,
                        actual,
                    ));
                }
            }

            // Validate the `dev-dependencies` metadata.
            {
                let expected: BTreeMap<GroupName, BTreeSet<Requirement>> = archive
                    .metadata
                    .dev_dependencies
                    .into_iter()
                    .map(|(group, requirements)| {
                        Ok::<_, LockError>((
                            group,
                            requirements
                                .into_iter()
                                .map(|requirement| normalize_requirement(requirement, workspace))
                                .collect::<Result<_, _>>()?,
                        ))
                    })
                    .collect::<Result<_, _>>()?;
                let actual: BTreeMap<GroupName, BTreeSet<Requirement>> = package
                    .metadata
                    .requires_dev
                    .iter()
                    .map(|(group, requirements)| {
                        Ok::<_, LockError>((
                            group.clone(),
                            requirements
                                .iter()
                                .cloned()
                                .map(|requirement| normalize_requirement(requirement, workspace))
                                .collect::<Result<_, _>>()?,
                        ))
                    })
                    .collect::<Result<_, _>>()?;

                if expected != actual {
                    return Ok(SatisfiesResult::MismatchedDevDependencies(
                        &package.id.name,
                        &package.id.version,
                        expected,
                        actual,
                    ));
                }
            }

            // Recurse.
            // TODO(charlie): Do we care about extras here, or any other fields on the `Dependency`?
            // Should we instead recurse on `requires_dist`?
            for dep in &package.dependencies {
                if seen.insert(&dep.package_id) {
                    let dep_dist = self.find_by_id(&dep.package_id);
                    queue.push_back(dep_dist);
                }
            }

            for dependencies in package.optional_dependencies.values() {
                for dep in dependencies {
                    if seen.insert(&dep.package_id) {
                        let dep_dist = self.find_by_id(&dep.package_id);
                        queue.push_back(dep_dist);
                    }
                }
            }

            for dependencies in package.dev_dependencies.values() {
                for dep in dependencies {
                    if seen.insert(&dep.package_id) {
                        let dep_dist = self.find_by_id(&dep.package_id);
                        queue.push_back(dep_dist);
                    }
                }
            }
        }

        Ok(SatisfiesResult::Satisfied)
    }
}

#[derive(Debug, Copy, Clone)]
enum TagPolicy<'tags> {
    /// Exclusively consider wheels that match the specified platform tags.
    Required(&'tags Tags),
    /// Prefer wheels that match the specified platform tags, but fall back to incompatible wheels
    /// if  necessary.
    Preferred(&'tags Tags),
}

impl<'tags> TagPolicy<'tags> {
    /// Returns the platform tags to consider.
    fn tags(&self) -> &'tags Tags {
        match self {
            TagPolicy::Required(tags) | TagPolicy::Preferred(tags) => tags,
        }
    }
}

/// The result of checking if a lockfile satisfies a set of requirements.
#[derive(Debug)]
pub enum SatisfiesResult<'lock> {
    /// The lockfile satisfies the requirements.
    Satisfied,
    /// The lockfile uses a different set of workspace members.
    MismatchedMembers(BTreeSet<PackageName>, &'lock BTreeSet<PackageName>),
    /// The lockfile uses a different set of sources for its workspace members.
    MismatchedSources(PackageName, bool),
    /// The lockfile uses a different set of version for its workspace members.
    MismatchedVersion(PackageName, Version, Option<Version>),
    /// The lockfile uses a different set of requirements.
    MismatchedRequirements(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of constraints.
    MismatchedConstraints(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile uses a different set of overrides.
    MismatchedOverrides(BTreeSet<Requirement>, BTreeSet<Requirement>),
    /// The lockfile is missing a workspace member.
    MissingRoot(PackageName),
    /// The lockfile referenced a remote index that was not provided
    MissingRemoteIndex(&'lock PackageName, &'lock Version, &'lock UrlString),
    /// The lockfile referenced a local index that was not provided
    MissingLocalIndex(&'lock PackageName, &'lock Version, &'lock PathBuf),
    /// The resolver failed to generate metadata for a given package.
    MissingMetadata(&'lock PackageName, &'lock Version),
    /// A package in the lockfile contains different `requires-dist` metadata than expected.
    MismatchedRequiresDist(
        &'lock PackageName,
        &'lock Version,
        BTreeSet<Requirement>,
        BTreeSet<Requirement>,
    ),
    /// A package in the lockfile contains different `dev-dependencies` metadata than expected.
    MismatchedDevDependencies(
        &'lock PackageName,
        &'lock Version,
        BTreeMap<GroupName, BTreeSet<Requirement>>,
        BTreeMap<GroupName, BTreeSet<Requirement>>,
    ),
}

/// We discard the lockfile if these options match.
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
struct ResolverOptions {
    /// The [`ResolutionMode`] used to generate this lock.
    #[serde(default)]
    resolution_mode: ResolutionMode,
    /// The [`PrereleaseMode`] used to generate this lock.
    #[serde(default)]
    prerelease_mode: PrereleaseMode,
    /// The [`ExcludeNewer`] used to generate this lock.
    exclude_newer: Option<ExcludeNewer>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ResolverManifest {
    /// The workspace members included in the lockfile.
    #[serde(default)]
    members: BTreeSet<PackageName>,
    /// The requirements provided to the resolver, exclusive of the workspace members.
    #[serde(default)]
    requirements: BTreeSet<Requirement>,
    /// The constraints provided to the resolver.
    #[serde(default)]
    constraints: BTreeSet<Requirement>,
    /// The overrides provided to the resolver.
    #[serde(default)]
    overrides: BTreeSet<Requirement>,
}

impl ResolverManifest {
    /// Initialize a [`ResolverManifest`] with the given members, requirements, constraints, and
    /// overrides.
    pub fn new(
        members: impl IntoIterator<Item = PackageName>,
        requirements: impl IntoIterator<Item = Requirement>,
        constraints: impl IntoIterator<Item = Requirement>,
        overrides: impl IntoIterator<Item = Requirement>,
    ) -> Self {
        Self {
            members: members.into_iter().collect(),
            requirements: requirements.into_iter().collect(),
            constraints: constraints.into_iter().collect(),
            overrides: overrides.into_iter().collect(),
        }
    }

    /// Convert the manifest to a relative form using the given workspace.
    pub fn relative_to(self, workspace: &Workspace) -> Result<Self, io::Error> {
        Ok(Self {
            members: self.members,
            requirements: self
                .requirements
                .into_iter()
                .map(|requirement| requirement.relative_to(workspace.install_path()))
                .collect::<Result<BTreeSet<_>, _>>()?,
            constraints: self
                .constraints
                .into_iter()
                .map(|requirement| requirement.relative_to(workspace.install_path()))
                .collect::<Result<BTreeSet<_>, _>>()?,
            overrides: self
                .overrides
                .into_iter()
                .map(|requirement| requirement.relative_to(workspace.install_path()))
                .collect::<Result<BTreeSet<_>, _>>()?,
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct LockWire {
    version: u32,
    requires_python: RequiresPython,
    /// If this lockfile was built from a forking resolution with non-identical forks, store the
    /// forks in the lockfile so we can recreate them in subsequent resolutions.
    #[serde(rename = "resolution-markers", default)]
    fork_markers: Vec<SimplifiedMarkerTree>,
    #[serde(rename = "supported-markers", default)]
    supported_environments: Vec<SimplifiedMarkerTree>,
    /// We discard the lockfile if these options match.
    #[serde(default)]
    options: ResolverOptions,
    #[serde(default)]
    manifest: ResolverManifest,
    #[serde(rename = "package", alias = "distribution", default)]
    packages: Vec<PackageWire>,
}

impl TryFrom<LockWire> for Lock {
    type Error = LockError;

    fn try_from(wire: LockWire) -> Result<Lock, LockError> {
        // Count the number of sources for each package name. When
        // there's only one source for a particular package name (the
        // overwhelmingly common case), we can omit some data (like source and
        // version) on dependency edges since it is strictly redundant.
        let mut unambiguous_package_ids: FxHashMap<PackageName, PackageId> = FxHashMap::default();
        let mut ambiguous = FxHashSet::default();
        for dist in &wire.packages {
            if ambiguous.contains(&dist.id.name) {
                continue;
            }
            if unambiguous_package_ids.remove(&dist.id.name).is_some() {
                ambiguous.insert(dist.id.name.clone());
                continue;
            }
            unambiguous_package_ids.insert(dist.id.name.clone(), dist.id.clone());
        }

        let packages = wire
            .packages
            .into_iter()
            .map(|dist| dist.unwire(&wire.requires_python, &unambiguous_package_ids))
            .collect::<Result<Vec<_>, _>>()?;
        let supported_environments = wire
            .supported_environments
            .into_iter()
            .map(|simplified_marker| simplified_marker.into_marker(&wire.requires_python))
            .collect();
        let fork_markers = wire
            .fork_markers
            .into_iter()
            .map(|simplified_marker| simplified_marker.into_marker(&wire.requires_python))
            .collect();
        let lock = Lock::new(
            wire.version,
            packages,
            wire.requires_python,
            wire.options,
            wire.manifest,
            supported_environments,
            fork_markers,
        )?;

        /*
        // TODO: Use the below in tests to validate we don't produce a
        // trivially incorrect lock file.
        let mut name_to_markers: BTreeMap<&PackageName, Vec<(&Version, &MarkerTree)>> =
            BTreeMap::new();
        for package in &lock.packages {
            for dep in &package.dependencies {
                name_to_markers
                    .entry(&dep.package_id.name)
                    .or_default()
                    .push((&dep.package_id.version, &dep.marker));
            }
        }
        for (name, marker_trees) in name_to_markers {
            for (i, (version1, marker1)) in marker_trees.iter().enumerate() {
                for (version2, marker2) in &marker_trees[i + 1..] {
                    if version1 == version2 {
                        continue;
                    }
                    if !marker1.is_disjoint(marker2) {
                        assert!(
                            false,
                            "[{marker1:?}] (for version {version1}) is not disjoint with \
                             [{marker2:?}] (for version {version2}) \
                             for package `{name}`",
                        );
                    }
                }
            }
        }
        */

        Ok(lock)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub(crate) id: PackageId,
    sdist: Option<SourceDist>,
    wheels: Vec<Wheel>,
    /// If there are multiple versions or sources for the same package name, we add the markers of
    /// the fork(s) that contained this version or source, so we can set the correct preferences in
    /// the next resolution.
    ///
    /// Named `resolution-markers` in `uv.lock`.
    fork_markers: Vec<MarkerTree>,
    /// The resolved dependencies of the package.
    dependencies: Vec<Dependency>,
    /// The resolved optional dependencies of the package.
    optional_dependencies: BTreeMap<ExtraName, Vec<Dependency>>,
    /// The resolved development dependencies of the package.
    dev_dependencies: BTreeMap<GroupName, Vec<Dependency>>,
    /// The exact requirements from the package metadata.
    metadata: PackageMetadata,
}

impl Package {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        fork_markers: Vec<MarkerTree>,
        root: &Path,
    ) -> Result<Self, LockError> {
        let id = PackageId::from_annotated_dist(annotated_dist, root)?;
        let sdist = SourceDist::from_annotated_dist(&id, annotated_dist)?;
        let wheels = Wheel::from_annotated_dist(annotated_dist)?;
        let requires_dist = if id.source.is_immutable() {
            BTreeSet::default()
        } else {
            annotated_dist
                .metadata
                .as_ref()
                .expect("metadata is present")
                .requires_dist
                .iter()
                .cloned()
                .map(|requirement| requirement.relative_to(root))
                .collect::<Result<_, _>>()
                .map_err(LockErrorKind::RequirementRelativePath)?
        };
        let requires_dev = if id.source.is_immutable() {
            BTreeMap::default()
        } else {
            annotated_dist
                .metadata
                .as_ref()
                .expect("metadata is present")
                .dev_dependencies
                .iter()
                .map(|(group, requirements)| {
                    let requirements = requirements
                        .iter()
                        .cloned()
                        .map(|requirement| requirement.relative_to(root))
                        .collect::<Result<_, _>>()
                        .map_err(LockErrorKind::RequirementRelativePath)?;
                    Ok::<_, LockError>((group.clone(), requirements))
                })
                .collect::<Result<_, _>>()?
        };
        Ok(Package {
            id,
            sdist,
            wheels,
            fork_markers,
            dependencies: vec![],
            optional_dependencies: BTreeMap::default(),
            dev_dependencies: BTreeMap::default(),
            metadata: PackageMetadata {
                requires_dist,
                requires_dev,
            },
        })
    }

    /// Add the [`AnnotatedDist`] as a dependency of the [`Package`].
    fn add_dependency(
        &mut self,
        requires_python: &RequiresPython,
        annotated_dist: &AnnotatedDist,
        marker: MarkerTree,
        root: &Path,
    ) -> Result<(), LockError> {
        let new_dep =
            Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        for existing_dep in &mut self.dependencies {
            if existing_dep.package_id == new_dep.package_id
                // It's important that we do a comparison on
                // *simplified* markers here. In particular, when
                // we write markers out to the lock file, we use
                // "simplified" markers, or markers that are simplified
                // *given* that `requires-python` is satisfied. So if
                // we don't do equality based on what the simplified
                // marker is, we might wind up not merging dependencies
                // that ought to be merged and thus writing out extra
                // entries.
                //
                // For example, if `requires-python = '>=3.8'` and we
                // have `foo==1` and
                // `foo==1 ; python_version >= '3.8'` dependencies,
                // then they don't have equivalent complexified
                // markers, but their simplified markers are identical.
                //
                // NOTE: It does seem like perhaps this should
                // be implemented semantically/algebraically on
                // `MarkerTree` itself, but it wasn't totally clear
                // how to do that. I think `pep508` would need to
                // grow a concept of "requires python" and provide an
                // operation specifically for that.
                && existing_dep.simplified_marker == new_dep.simplified_marker
            {
                existing_dep.extra.extend(new_dep.extra);
                return Ok(());
            }
        }

        self.dependencies.push(new_dep);
        Ok(())
    }

    /// Add the [`AnnotatedDist`] as an optional dependency of the [`Package`].
    fn add_optional_dependency(
        &mut self,
        requires_python: &RequiresPython,
        extra: ExtraName,
        annotated_dist: &AnnotatedDist,
        marker: MarkerTree,
        root: &Path,
    ) -> Result<(), LockError> {
        let dep = Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        let optional_deps = self.optional_dependencies.entry(extra).or_default();
        for existing_dep in &mut *optional_deps {
            if existing_dep.package_id == dep.package_id
                // See note in add_dependency for why we use
                // simplified markers here.
                && existing_dep.simplified_marker == dep.simplified_marker
            {
                existing_dep.extra.extend(dep.extra);
                return Ok(());
            }
        }

        optional_deps.push(dep);
        Ok(())
    }

    /// Add the [`AnnotatedDist`] as a development dependency of the [`Package`].
    fn add_dev_dependency(
        &mut self,
        requires_python: &RequiresPython,
        dev: GroupName,
        annotated_dist: &AnnotatedDist,
        marker: MarkerTree,
        root: &Path,
    ) -> Result<(), LockError> {
        let dep = Dependency::from_annotated_dist(requires_python, annotated_dist, marker, root)?;
        let dev_deps = self.dev_dependencies.entry(dev).or_default();
        for existing_dep in &mut *dev_deps {
            if existing_dep.package_id == dep.package_id
                // See note in add_dependency for why we use
                // simplified markers here.
                && existing_dep.simplified_marker == dep.simplified_marker
            {
                existing_dep.extra.extend(dep.extra);
                return Ok(());
            }
        }

        dev_deps.push(dep);
        Ok(())
    }

    /// Convert the [`Package`] to a [`Dist`] that can be used in installation.
    fn to_dist(
        &self,
        workspace_root: &Path,
        tag_policy: TagPolicy<'_>,
        build_options: &BuildOptions,
    ) -> Result<Dist, LockError> {
        let no_binary = build_options.no_binary_package(&self.id.name);
        let no_build = build_options.no_build_package(&self.id.name);

        if !no_binary {
            if let Some(best_wheel_index) = self.find_best_wheel(tag_policy) {
                return match &self.id.source {
                    Source::Registry(source) => {
                        let wheels = self
                            .wheels
                            .iter()
                            .map(|wheel| wheel.to_registry_dist(source, workspace_root))
                            .collect::<Result<_, LockError>>()?;
                        let reg_built_dist = RegistryBuiltDist {
                            wheels,
                            best_wheel_index,
                            sdist: None,
                        };
                        Ok(Dist::Built(BuiltDist::Registry(reg_built_dist)))
                    }
                    Source::Path(path) => {
                        let filename: WheelFilename =
                            self.wheels[best_wheel_index].filename.clone();
                        let path_dist = PathBuiltDist {
                            filename,
                            url: verbatim_url(workspace_root.join(path), &self.id)?,
                            install_path: workspace_root.join(path),
                        };
                        let built_dist = BuiltDist::Path(path_dist);
                        Ok(Dist::Built(built_dist))
                    }
                    Source::Direct(url, direct) => {
                        let filename: WheelFilename =
                            self.wheels[best_wheel_index].filename.clone();
                        let url = Url::from(ParsedArchiveUrl {
                            url: url.to_url(),
                            subdirectory: direct.subdirectory.as_ref().map(PathBuf::from),
                            ext: DistExtension::Wheel,
                        });
                        let direct_dist = DirectUrlBuiltDist {
                            filename,
                            location: url.clone(),
                            url: VerbatimUrl::from_url(url),
                        };
                        let built_dist = BuiltDist::DirectUrl(direct_dist);
                        Ok(Dist::Built(built_dist))
                    }
                    Source::Git(_, _) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "Git",
                    }
                    .into()),
                    Source::Directory(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "directory",
                    }
                    .into()),
                    Source::Editable(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "editable",
                    }
                    .into()),
                    Source::Virtual(_) => Err(LockErrorKind::InvalidWheelSource {
                        id: self.id.clone(),
                        source_type: "virtual",
                    }
                    .into()),
                };
            };
        }

        if !no_build {
            if let Some(sdist) = self.to_source_dist(workspace_root)? {
                return Ok(Dist::Source(sdist));
            }
        }

        match (no_binary, no_build) {
            (true, true) => Err(LockErrorKind::NoBinaryNoBuild {
                id: self.id.clone(),
            }
            .into()),
            (true, false) if self.id.source.is_wheel() => Err(LockErrorKind::NoBinaryWheelOnly {
                id: self.id.clone(),
            }
            .into()),
            (true, false) => Err(LockErrorKind::NoBinary {
                id: self.id.clone(),
            }
            .into()),
            (false, true) => Err(LockErrorKind::NoBuild {
                id: self.id.clone(),
            }
            .into()),
            (false, false) if self.id.source.is_wheel() => {
                Err(LockErrorKind::IncompatibleWheelOnly {
                    id: self.id.clone(),
                }
                .into())
            }
            (false, false) => Err(LockErrorKind::NeitherSourceDistNorWheel {
                id: self.id.clone(),
            }
            .into()),
        }
    }

    /// Convert the source of this [`Package`] to a [`SourceDist`] that can be used in installation.
    ///
    /// Returns `Ok(None)` if the source cannot be converted because `self.sdist` is `None`. This is required
    /// for registry sources.
    fn to_source_dist(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<distribution_types::SourceDist>, LockError> {
        let sdist = match &self.id.source {
            Source::Path(path) => {
                // A direct path source can also be a wheel, so validate the extension.
                let DistExtension::Source(ext) = DistExtension::from_path(path)? else {
                    return Ok(None);
                };
                let path_dist = PathSourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    ext,
                };
                distribution_types::SourceDist::Path(path_dist)
            }
            Source::Directory(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    editable: false,
                    r#virtual: false,
                };
                distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Editable(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    editable: true,
                    r#virtual: false,
                };
                distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Virtual(path) => {
                let dir_dist = DirectorySourceDist {
                    name: self.id.name.clone(),
                    url: verbatim_url(workspace_root.join(path), &self.id)?,
                    install_path: workspace_root.join(path),
                    editable: false,
                    r#virtual: true,
                };
                distribution_types::SourceDist::Directory(dir_dist)
            }
            Source::Git(url, git) => {
                // Remove the fragment and query from the URL; they're already present in the
                // `GitSource`.
                let mut url = url.to_url();
                url.set_fragment(None);
                url.set_query(None);

                // Reconstruct the `GitUrl` from the `GitSource`.
                let git_url = uv_git::GitUrl::from_commit(
                    url,
                    GitReference::from(git.kind.clone()),
                    git.precise,
                );

                // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                let url = Url::from(ParsedGitUrl {
                    url: git_url.clone(),
                    subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                });

                let git_dist = GitSourceDist {
                    name: self.id.name.clone(),
                    url: VerbatimUrl::from_url(url),
                    git: Box::new(git_url),
                    subdirectory: git.subdirectory.as_ref().map(PathBuf::from),
                };
                distribution_types::SourceDist::Git(git_dist)
            }
            Source::Direct(url, direct) => {
                // A direct URL source can also be a wheel, so validate the extension.
                let DistExtension::Source(ext) = DistExtension::from_path(url.as_ref())? else {
                    return Ok(None);
                };
                let subdirectory = direct.subdirectory.as_ref().map(PathBuf::from);
                let url = Url::from(ParsedArchiveUrl {
                    url: url.to_url(),
                    subdirectory: subdirectory.clone(),
                    ext: DistExtension::Source(ext),
                });
                let direct_dist = DirectUrlSourceDist {
                    name: self.id.name.clone(),
                    location: url.clone(),
                    subdirectory: subdirectory.clone(),
                    ext,
                    url: VerbatimUrl::from_url(url),
                };
                distribution_types::SourceDist::DirectUrl(direct_dist)
            }
            Source::Registry(RegistrySource::Url(url)) => {
                let Some(ref sdist) = self.sdist else {
                    return Ok(None);
                };

                let file_url = sdist.url().ok_or_else(|| LockErrorKind::MissingUrl {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                })?;
                let filename = sdist
                    .filename()
                    .ok_or_else(|| LockErrorKind::MissingFilename {
                        id: self.id.clone(),
                    })?;
                let ext = SourceDistExtension::from_path(filename.as_ref())?;
                let file = Box::new(distribution_types::File {
                    dist_info_metadata: false,
                    filename: filename.to_string(),
                    hashes: sdist
                        .hash()
                        .map(|hash| vec![hash.0.clone()])
                        .unwrap_or_default(),
                    requires_python: None,
                    size: sdist.size(),
                    upload_time_utc_ms: None,
                    url: FileLocation::AbsoluteUrl(file_url.clone()),
                    yanked: None,
                });
                let index = IndexUrl::from(VerbatimUrl::from_url(url.to_url()));

                let reg_dist = RegistrySourceDist {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                    file,
                    ext,
                    index,
                    wheels: vec![],
                };
                distribution_types::SourceDist::Registry(reg_dist)
            }
            Source::Registry(RegistrySource::Path(path)) => {
                let Some(ref sdist) = self.sdist else {
                    return Ok(None);
                };

                let file_path = sdist.path().ok_or_else(|| LockErrorKind::MissingPath {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                })?;
                let file_url = Url::from_file_path(workspace_root.join(path).join(file_path))
                    .map_err(|()| LockErrorKind::PathToUrl)?;
                let filename = sdist
                    .filename()
                    .ok_or_else(|| LockErrorKind::MissingFilename {
                        id: self.id.clone(),
                    })?;
                let ext = SourceDistExtension::from_path(filename.as_ref())?;
                let file = Box::new(distribution_types::File {
                    dist_info_metadata: false,
                    filename: filename.to_string(),
                    hashes: sdist
                        .hash()
                        .map(|hash| vec![hash.0.clone()])
                        .unwrap_or_default(),
                    requires_python: None,
                    size: sdist.size(),
                    upload_time_utc_ms: None,
                    url: FileLocation::AbsoluteUrl(UrlString::from(file_url)),
                    yanked: None,
                });
                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(workspace_root.join(path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );

                let reg_dist = RegistrySourceDist {
                    name: self.id.name.clone(),
                    version: self.id.version.clone(),
                    file,
                    ext,
                    index,
                    wheels: vec![],
                };
                distribution_types::SourceDist::Registry(reg_dist)
            }
        };

        Ok(Some(sdist))
    }

    fn to_toml(
        &self,
        requires_python: &RequiresPython,
        dist_count_by_name: &FxHashMap<PackageName, u64>,
    ) -> anyhow::Result<Table> {
        let mut table = Table::new();

        self.id.to_toml(None, &mut table);

        if !self.fork_markers.is_empty() {
            let wheels = each_element_on_its_line_array(
                self.fork_markers
                    .iter()
                    .map(|marker| SimplifiedMarkerTree::new(requires_python, marker.clone()))
                    .filter_map(|marker| marker.try_to_string()),
            );
            table.insert("resolution-markers", value(wheels));
        }

        if !self.dependencies.is_empty() {
            let deps = each_element_on_its_line_array(self.dependencies.iter().map(|dep| {
                dep.to_toml(requires_python, dist_count_by_name)
                    .into_inline_table()
            }));
            table.insert("dependencies", value(deps));
        }

        if !self.optional_dependencies.is_empty() {
            let mut optional_deps = Table::new();
            for (extra, deps) in &self.optional_dependencies {
                let deps = each_element_on_its_line_array(deps.iter().map(|dep| {
                    dep.to_toml(requires_python, dist_count_by_name)
                        .into_inline_table()
                }));
                if !deps.is_empty() {
                    optional_deps.insert(extra.as_ref(), value(deps));
                }
            }
            if !optional_deps.is_empty() {
                table.insert("optional-dependencies", Item::Table(optional_deps));
            }
        }

        if !self.dev_dependencies.is_empty() {
            let mut dev_dependencies = Table::new();
            for (extra, deps) in &self.dev_dependencies {
                let deps = each_element_on_its_line_array(deps.iter().map(|dep| {
                    dep.to_toml(requires_python, dist_count_by_name)
                        .into_inline_table()
                }));
                if !deps.is_empty() {
                    dev_dependencies.insert(extra.as_ref(), value(deps));
                }
            }
            if !dev_dependencies.is_empty() {
                table.insert("dev-dependencies", Item::Table(dev_dependencies));
            }
        }

        if let Some(ref sdist) = self.sdist {
            table.insert("sdist", value(sdist.to_toml()?));
        }

        if !self.wheels.is_empty() {
            let wheels = each_element_on_its_line_array(
                self.wheels
                    .iter()
                    .map(Wheel::to_toml)
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .into_iter(),
            );
            table.insert("wheels", value(wheels));
        }

        // Write the package metadata, if non-empty.
        {
            let mut metadata_table = Table::new();

            if !self.metadata.requires_dist.is_empty() {
                let requires_dist = self
                    .metadata
                    .requires_dist
                    .iter()
                    .map(|requirement| {
                        serde::Serialize::serialize(
                            &requirement,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let requires_dist = match requires_dist.as_slice() {
                    [] => Array::new(),
                    [requirement] => Array::from_iter([requirement]),
                    requires_dist => each_element_on_its_line_array(requires_dist.iter()),
                };
                metadata_table.insert("requires-dist", value(requires_dist));
            }

            if !self.metadata.requires_dev.is_empty() {
                let mut requires_dev = Table::new();
                for (extra, deps) in &self.metadata.requires_dev {
                    let deps = deps
                        .iter()
                        .map(|requirement| {
                            serde::Serialize::serialize(
                                &requirement,
                                toml_edit::ser::ValueSerializer::new(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let deps = match deps.as_slice() {
                        [] => Array::new(),
                        [requirement] => Array::from_iter([requirement]),
                        deps => each_element_on_its_line_array(deps.iter()),
                    };
                    if !deps.is_empty() {
                        requires_dev.insert(extra.as_ref(), value(deps));
                    }
                }
                if !requires_dev.is_empty() {
                    metadata_table.insert("requires-dev", Item::Table(requires_dev));
                }
            }

            if !metadata_table.is_empty() {
                table.insert("metadata", Item::Table(metadata_table));
            }
        }

        Ok(table)
    }

    fn find_best_wheel(&self, tag_policy: TagPolicy<'_>) -> Option<usize> {
        let mut best: Option<(TagPriority, usize)> = None;
        for (i, wheel) in self.wheels.iter().enumerate() {
            let TagCompatibility::Compatible(priority) =
                wheel.filename.compatibility(tag_policy.tags())
            else {
                continue;
            };
            match best {
                None => {
                    best = Some((priority, i));
                }
                Some((best_priority, _)) => {
                    if priority > best_priority {
                        best = Some((priority, i));
                    }
                }
            }
        }

        let best = best.map(|(_, i)| i);
        match tag_policy {
            TagPolicy::Required(_) => best,
            TagPolicy::Preferred(_) => best.or_else(|| self.wheels.first().map(|_| 0)),
        }
    }

    /// Returns the [`PackageName`] of the package.
    pub fn name(&self) -> &PackageName {
        &self.id.name
    }

    /// Returns the [`Version`] of the package.
    pub fn version(&self) -> &Version {
        &self.id.version
    }

    /// Return the fork markers for this package, if any.
    pub fn fork_markers(&self) -> &[MarkerTree] {
        self.fork_markers.as_slice()
    }

    /// Returns all the hashes associated with this [`Package`].
    fn hashes(&self) -> Vec<HashDigest> {
        let mut hashes = Vec::new();
        if let Some(ref sdist) = self.sdist {
            if let Some(hash) = sdist.hash() {
                hashes.push(hash.0.clone());
            }
        }
        for wheel in &self.wheels {
            hashes.extend(wheel.hash.as_ref().map(|h| h.0.clone()));
        }
        hashes
    }

    /// Returns the [`ResolvedRepositoryReference`] for the package, if it is a Git source.
    pub fn as_git_ref(&self) -> Option<ResolvedRepositoryReference> {
        match &self.id.source {
            Source::Git(url, git) => Some(ResolvedRepositoryReference {
                reference: RepositoryReference {
                    url: RepositoryUrl::new(&url.to_url()),
                    reference: GitReference::from(git.kind.clone()),
                },
                sha: git.precise,
            }),
            _ => None,
        }
    }
}

/// Attempts to construct a `VerbatimUrl` from the given `Path`.
fn verbatim_url(path: PathBuf, id: &PackageId) -> Result<VerbatimUrl, LockError> {
    let url = VerbatimUrl::from_absolute_path(path).map_err(|err| LockErrorKind::VerbatimUrl {
        id: id.clone(),
        err,
    })?;

    Ok(url)
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageWire {
    #[serde(flatten)]
    id: PackageId,
    #[serde(default)]
    metadata: PackageMetadata,
    #[serde(default)]
    sdist: Option<SourceDist>,
    #[serde(default)]
    wheels: Vec<Wheel>,
    #[serde(default, rename = "resolution-markers")]
    fork_markers: Vec<SimplifiedMarkerTree>,
    #[serde(default)]
    dependencies: Vec<DependencyWire>,
    #[serde(default)]
    optional_dependencies: BTreeMap<ExtraName, Vec<DependencyWire>>,
    #[serde(default)]
    dev_dependencies: BTreeMap<GroupName, Vec<DependencyWire>>,
}

#[derive(Clone, Default, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    #[serde(default)]
    requires_dist: BTreeSet<Requirement>,
    #[serde(default)]
    requires_dev: BTreeMap<GroupName, BTreeSet<Requirement>>,
}

impl PackageWire {
    fn unwire(
        self,
        requires_python: &RequiresPython,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<Package, LockError> {
        let unwire_deps = |deps: Vec<DependencyWire>| -> Result<Vec<Dependency>, LockError> {
            deps.into_iter()
                .map(|dep| dep.unwire(requires_python, unambiguous_package_ids))
                .collect()
        };
        Ok(Package {
            id: self.id,
            metadata: self.metadata,
            sdist: self.sdist,
            wheels: self.wheels,
            fork_markers: self
                .fork_markers
                .into_iter()
                .map(|simplified_marker| simplified_marker.into_marker(requires_python))
                .collect(),
            dependencies: unwire_deps(self.dependencies)?,
            optional_dependencies: self
                .optional_dependencies
                .into_iter()
                .map(|(extra, deps)| Ok((extra, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
            dev_dependencies: self
                .dev_dependencies
                .into_iter()
                .map(|(group, deps)| Ok((group, unwire_deps(deps)?)))
                .collect::<Result<_, LockError>>()?,
        })
    }
}

/// Inside the lockfile, we match a dependency entry to a package entry through a key made up
/// of the name, the version and the source url.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
pub(crate) struct PackageId {
    pub(crate) name: PackageName,
    pub(crate) version: Version,
    source: Source,
}

impl PackageId {
    fn from_annotated_dist(
        annotated_dist: &AnnotatedDist,
        root: &Path,
    ) -> Result<PackageId, LockError> {
        let name = annotated_dist.name.clone();
        let version = annotated_dist.version.clone();
        let source = Source::from_resolved_dist(&annotated_dist.dist, root)?;
        Ok(Self {
            name,
            version,
            source,
        })
    }

    /// Writes this package ID inline into the table given.
    ///
    /// When a map is given, and if the package name in this ID is unambiguous
    /// (i.e., it has a count of 1 in the map), then the `version` and `source`
    /// fields are omitted. In all other cases, including when a map is not
    /// given, the `version` and `source` fields are written.
    fn to_toml(&self, dist_count_by_name: Option<&FxHashMap<PackageName, u64>>, table: &mut Table) {
        let count = dist_count_by_name.and_then(|map| map.get(&self.name).copied());
        table.insert("name", value(self.name.to_string()));
        if count.map(|count| count > 1).unwrap_or(true) {
            table.insert("version", value(self.version.to_string()));
            self.source.to_toml(table);
        }
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}=={} @ {}", self.name, self.version, self.source)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct PackageIdForDependency {
    name: PackageName,
    version: Option<Version>,
    source: Option<Source>,
}

impl PackageIdForDependency {
    fn unwire(
        self,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<PackageId, LockError> {
        let unambiguous_package_id = unambiguous_package_ids.get(&self.name);
        let version = self.version.map(Ok::<_, LockError>).unwrap_or_else(|| {
            let Some(dist_id) = unambiguous_package_id else {
                return Err(LockErrorKind::MissingDependencyVersion {
                    name: self.name.clone(),
                }
                .into());
            };
            Ok(dist_id.version.clone())
        })?;
        let source = self.source.map(Ok::<_, LockError>).unwrap_or_else(|| {
            let Some(package_id) = unambiguous_package_id else {
                return Err(LockErrorKind::MissingDependencySource {
                    name: self.name.clone(),
                }
                .into());
            };
            Ok(package_id.source.clone())
        })?;
        Ok(PackageId {
            name: self.name,
            version,
            source,
        })
    }
}

impl From<PackageId> for PackageIdForDependency {
    fn from(id: PackageId) -> PackageIdForDependency {
        PackageIdForDependency {
            name: id.name,
            version: Some(id.version),
            source: Some(id.source),
        }
    }
}

/// A unique identifier to differentiate between different sources for the same version of a
/// package.
///
/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of sources.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "SourceWire")]
enum Source {
    /// A registry or `--find-links` index.
    Registry(RegistrySource),
    /// A Git repository.
    Git(UrlString, GitSource),
    /// A direct HTTP(S) URL.
    Direct(UrlString, DirectSource),
    /// A path to a local source or built archive.
    Path(PathBuf),
    /// A path to a local directory.
    Directory(PathBuf),
    /// A path to a local directory that should be installed as editable.
    Editable(PathBuf),
    /// A path to a local directory that should not be built or installed.
    Virtual(PathBuf),
}

impl Source {
    fn from_resolved_dist(resolved_dist: &ResolvedDist, root: &Path) -> Result<Source, LockError> {
        match *resolved_dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed(_) => unreachable!(),
            ResolvedDist::Installable(ref dist) => Source::from_dist(dist, root),
        }
    }

    fn from_dist(dist: &Dist, root: &Path) -> Result<Source, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Source::from_built_dist(built_dist, root),
            Dist::Source(ref source_dist) => Source::from_source_dist(source_dist, root),
        }
    }

    fn from_built_dist(built_dist: &BuiltDist, root: &Path) -> Result<Source, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Source::from_registry_built_dist(reg_dist, root),
            BuiltDist::DirectUrl(ref direct_dist) => {
                Ok(Source::from_direct_built_dist(direct_dist))
            }
            BuiltDist::Path(ref path_dist) => Source::from_path_built_dist(path_dist, root),
        }
    }

    fn from_source_dist(
        source_dist: &distribution_types::SourceDist,
        root: &Path,
    ) -> Result<Source, LockError> {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                Source::from_registry_source_dist(reg_dist, root)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                Ok(Source::from_direct_source_dist(direct_dist))
            }
            distribution_types::SourceDist::Git(ref git_dist) => {
                Ok(Source::from_git_dist(git_dist))
            }
            distribution_types::SourceDist::Path(ref path_dist) => {
                Source::from_path_source_dist(path_dist, root)
            }
            distribution_types::SourceDist::Directory(ref directory) => {
                Source::from_directory_source_dist(directory, root)
            }
        }
    }

    fn from_registry_built_dist(
        reg_dist: &RegistryBuiltDist,
        root: &Path,
    ) -> Result<Source, LockError> {
        Source::from_index_url(&reg_dist.best_wheel().index, root)
    }

    fn from_registry_source_dist(
        reg_dist: &RegistrySourceDist,
        root: &Path,
    ) -> Result<Source, LockError> {
        Source::from_index_url(&reg_dist.index, root)
    }

    fn from_direct_built_dist(direct_dist: &DirectUrlBuiltDist) -> Source {
        Source::Direct(
            normalize_url(direct_dist.url.to_url()),
            DirectSource { subdirectory: None },
        )
    }

    fn from_direct_source_dist(direct_dist: &DirectUrlSourceDist) -> Source {
        Source::Direct(
            normalize_url(direct_dist.url.to_url()),
            DirectSource {
                subdirectory: direct_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            },
        )
    }

    fn from_path_built_dist(path_dist: &PathBuiltDist, root: &Path) -> Result<Source, LockError> {
        let path = relative_to(&path_dist.install_path, root)
            .or_else(|_| std::path::absolute(&path_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        Ok(Source::Path(path))
    }

    fn from_path_source_dist(path_dist: &PathSourceDist, root: &Path) -> Result<Source, LockError> {
        let path = relative_to(&path_dist.install_path, root)
            .or_else(|_| std::path::absolute(&path_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        Ok(Source::Path(path))
    }

    fn from_directory_source_dist(
        directory_dist: &DirectorySourceDist,
        root: &Path,
    ) -> Result<Source, LockError> {
        let path = relative_to(&directory_dist.install_path, root)
            .or_else(|_| std::path::absolute(&directory_dist.install_path))
            .map_err(LockErrorKind::DistributionRelativePath)?;
        if directory_dist.editable {
            Ok(Source::Editable(path))
        } else if directory_dist.r#virtual {
            Ok(Source::Virtual(path))
        } else {
            Ok(Source::Directory(path))
        }
    }

    fn from_index_url(index_url: &IndexUrl, root: &Path) -> Result<Source, LockError> {
        match index_url {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                // Remove any sensitive credentials from the index URL.
                let redacted = index_url.redacted();
                let source = RegistrySource::Url(UrlString::from(redacted.as_ref()));
                Ok(Source::Registry(source))
            }
            IndexUrl::Path(url) => {
                let path = url.to_file_path().map_err(|()| LockErrorKind::UrlToPath)?;
                let path = relative_to(&path, root)
                    .or_else(|_| std::path::absolute(&path))
                    .map_err(LockErrorKind::IndexRelativePath)?;
                let source = RegistrySource::Path(path);
                Ok(Source::Registry(source))
            }
        }
    }

    fn from_git_dist(git_dist: &GitSourceDist) -> Source {
        Source::Git(
            UrlString::from(locked_git_url(git_dist)),
            GitSource {
                kind: GitSourceKind::from(git_dist.git.reference().clone()),
                precise: git_dist.git.precise().unwrap_or_else(|| {
                    panic!("Git distribution is missing a precise hash: {git_dist}")
                }),
                subdirectory: git_dist
                    .subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(ToString::to_string),
            },
        )
    }

    /// Returns `true` if the source should be considered immutable.
    ///
    /// We assume that registry sources are immutable. In other words, we expect that once a
    /// package-version is published to a registry, its metadata will not change.
    ///
    /// We also assume that Git sources are immutable, since a Git source encodes a specific commit.
    fn is_immutable(&self) -> bool {
        matches!(self, Self::Registry(..) | Self::Git(_, _))
    }

    /// Returns `true` if the source is that of a wheel.
    fn is_wheel(&self) -> bool {
        match &self {
            Source::Path(path) => {
                matches!(
                    DistExtension::from_path(path).ok(),
                    Some(DistExtension::Wheel)
                )
            }
            Source::Direct(url, _) => {
                matches!(
                    DistExtension::from_path(url.as_ref()).ok(),
                    Some(DistExtension::Wheel)
                )
            }
            Source::Directory(..) => false,
            Source::Editable(..) => false,
            Source::Virtual(..) => false,
            Source::Git(..) => false,
            Source::Registry(..) => false,
        }
    }

    fn to_toml(&self, table: &mut Table) {
        let mut source_table = InlineTable::new();
        match *self {
            Source::Registry(ref source) => match source {
                RegistrySource::Url(url) => {
                    source_table.insert("registry", Value::from(url.as_ref()));
                }
                RegistrySource::Path(path) => {
                    source_table.insert(
                        "registry",
                        Value::from(PortablePath::from(path).to_string()),
                    );
                }
            },
            Source::Git(ref url, _) => {
                source_table.insert("git", Value::from(url.as_ref()));
            }
            Source::Direct(ref url, DirectSource { ref subdirectory }) => {
                source_table.insert("url", Value::from(url.as_ref()));
                if let Some(ref subdirectory) = *subdirectory {
                    source_table.insert("subdirectory", Value::from(subdirectory));
                }
            }
            Source::Path(ref path) => {
                source_table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
            Source::Directory(ref path) => {
                source_table.insert(
                    "directory",
                    Value::from(PortablePath::from(path).to_string()),
                );
            }
            Source::Editable(ref path) => {
                source_table.insert(
                    "editable",
                    Value::from(PortablePath::from(path).to_string()),
                );
            }
            Source::Virtual(ref path) => {
                source_table.insert("virtual", Value::from(PortablePath::from(path).to_string()));
            }
        }
        table.insert("source", value(source_table));
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Source::Registry(RegistrySource::Url(url))
            | Source::Git(url, _)
            | Source::Direct(url, _) => {
                write!(f, "{}+{}", self.name(), url)
            }
            Source::Registry(RegistrySource::Path(path))
            | Source::Path(path)
            | Source::Directory(path)
            | Source::Editable(path)
            | Source::Virtual(path) => {
                write!(f, "{}+{}", self.name(), PortablePath::from(path))
            }
        }
    }
}

impl Source {
    fn name(&self) -> &str {
        match *self {
            Self::Registry(..) => "registry",
            Self::Git(..) => "git",
            Self::Direct(..) => "direct",
            Self::Path(..) => "path",
            Self::Directory(..) => "directory",
            Self::Editable(..) => "editable",
            Self::Virtual(..) => "virtual",
        }
    }

    /// Returns `Some(true)` to indicate that the source kind _must_ include a
    /// hash.
    ///
    /// Returns `Some(false)` to indicate that the source kind _must not_
    /// include a hash.
    ///
    /// Returns `None` to indicate that the source kind _may_ include a hash.
    fn requires_hash(&self) -> Option<bool> {
        match *self {
            Self::Registry(..) => None,
            Self::Direct(..) | Self::Path(..) => Some(true),
            Self::Git(..) | Self::Directory(..) | Self::Editable(..) | Self::Virtual(..) => {
                Some(false)
            }
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
enum SourceWire {
    Registry {
        registry: RegistrySource,
    },
    Git {
        git: String,
    },
    Direct {
        url: UrlString,
        #[serde(default)]
        subdirectory: Option<String>,
    },
    Path {
        path: PortablePathBuf,
    },
    Directory {
        directory: PortablePathBuf,
    },
    Editable {
        editable: PortablePathBuf,
    },
    Virtual {
        r#virtual: PortablePathBuf,
    },
}

impl TryFrom<SourceWire> for Source {
    type Error = LockError;

    fn try_from(wire: SourceWire) -> Result<Source, LockError> {
        #[allow(clippy::enum_glob_use)]
        use self::SourceWire::*;

        match wire {
            Registry { registry } => Ok(Source::Registry(registry)),
            Git { git } => {
                let url = Url::parse(&git)
                    .map_err(|err| SourceParseError::InvalidUrl {
                        given: git.to_string(),
                        err,
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;

                let git_source = GitSource::from_url(&url)
                    .map_err(|err| match err {
                        GitSourceError::InvalidSha => SourceParseError::InvalidSha {
                            given: git.to_string(),
                        },
                        GitSourceError::MissingSha => SourceParseError::MissingSha {
                            given: git.to_string(),
                        },
                    })
                    .map_err(LockErrorKind::InvalidGitSourceUrl)?;

                Ok(Source::Git(UrlString::from(url), git_source))
            }
            Direct { url, subdirectory } => Ok(Source::Direct(url, DirectSource { subdirectory })),
            Path { path } => Ok(Source::Path(path.into())),
            Directory { directory } => Ok(Source::Directory(directory.into())),
            Editable { editable } => Ok(Source::Editable(editable.into())),
            Virtual { r#virtual } => Ok(Source::Virtual(r#virtual.into())),
        }
    }
}

/// The source for a registry, which could be a URL or a relative path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
enum RegistrySource {
    /// Ex) `https://pypi.org/simple`
    Url(UrlString),
    /// Ex) `../path/to/local/index`
    Path(PathBuf),
}

impl std::fmt::Display for RegistrySource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RegistrySource::Url(url) => write!(f, "{url}"),
            RegistrySource::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

impl<'de> serde::de::Deserialize<'de> for RegistrySource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RegistrySource;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a valid URL or a file path")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if split_scheme(value).is_some() {
                    Ok(
                        serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
                            value,
                        ))
                        .map(RegistrySource::Url)?,
                    )
                } else {
                    Ok(
                        serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
                            value,
                        ))
                        .map(RegistrySource::Path)?,
                    )
                }
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DirectSource {
    subdirectory: Option<String>,
}

/// NOTE: Care should be taken when adding variants to this enum. Namely, new
/// variants should be added without changing the relative ordering of other
/// variants. Otherwise, this could cause the lockfile to have a different
/// canonical ordering of package entries.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct GitSource {
    precise: GitSha,
    subdirectory: Option<String>,
    kind: GitSourceKind,
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
enum GitSourceError {
    InvalidSha,
    MissingSha,
}

impl GitSource {
    /// Extracts a Git source reference from the query pairs and the hash
    /// fragment in the given URL.
    fn from_url(url: &Url) -> Result<GitSource, GitSourceError> {
        let mut kind = GitSourceKind::DefaultBranch;
        let mut subdirectory = None;
        for (key, val) in url.query_pairs() {
            match &*key {
                "tag" => kind = GitSourceKind::Tag(val.into_owned()),
                "branch" => kind = GitSourceKind::Branch(val.into_owned()),
                "rev" => kind = GitSourceKind::Rev(val.into_owned()),
                "subdirectory" => subdirectory = Some(val.into_owned()),
                _ => continue,
            };
        }
        let precise = GitSha::from_str(url.fragment().ok_or(GitSourceError::MissingSha)?)
            .map_err(|_| GitSourceError::InvalidSha)?;

        Ok(GitSource {
            precise,
            subdirectory,
            kind,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
enum GitSourceKind {
    Tag(String),
    Branch(String),
    Rev(String),
    DefaultBranch,
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
struct SourceDistMetadata {
    /// A hash of the source distribution.
    hash: Option<Hash>,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    size: Option<u64>,
}

/// A URL or file path where the source dist that was
/// locked against was found. The location does not need to exist in the
/// future, so this should be treated as only a hint to where to look
/// and/or recording where the source dist file originally came from.
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(try_from = "SourceDistWire")]
enum SourceDist {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Path {
        path: PathBuf,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
}

impl SourceDist {
    fn filename(&self) -> Option<Cow<str>> {
        match self {
            SourceDist::Url { url, .. } => url.filename().ok(),
            SourceDist::Path { path, .. } => {
                path.file_name().map(|filename| filename.to_string_lossy())
            }
        }
    }

    fn url(&self) -> Option<&UrlString> {
        match &self {
            SourceDist::Url { url, .. } => Some(url),
            SourceDist::Path { .. } => None,
        }
    }

    fn path(&self) -> Option<&Path> {
        match &self {
            SourceDist::Url { .. } => None,
            SourceDist::Path { path, .. } => Some(path),
        }
    }

    fn hash(&self) -> Option<&Hash> {
        match &self {
            SourceDist::Url { metadata, .. } => metadata.hash.as_ref(),
            SourceDist::Path { metadata, .. } => metadata.hash.as_ref(),
        }
    }

    fn size(&self) -> Option<u64> {
        match &self {
            SourceDist::Url { metadata, .. } => metadata.size,
            SourceDist::Path { metadata, .. } => metadata.size,
        }
    }
}

impl SourceDist {
    fn from_annotated_dist(
        id: &PackageId,
        annotated_dist: &AnnotatedDist,
    ) -> Result<Option<SourceDist>, LockError> {
        match annotated_dist.dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed(_) => unreachable!(),
            ResolvedDist::Installable(ref dist) => {
                SourceDist::from_dist(id, dist, &annotated_dist.hashes, annotated_dist.index())
            }
        }
    }

    fn from_dist(
        id: &PackageId,
        dist: &Dist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Option<SourceDist>, LockError> {
        match *dist {
            Dist::Built(BuiltDist::Registry(ref built_dist)) => {
                let Some(sdist) = built_dist.sdist.as_ref() else {
                    return Ok(None);
                };
                SourceDist::from_registry_dist(sdist, index)
            }
            Dist::Built(_) => Ok(None),
            Dist::Source(ref source_dist) => {
                SourceDist::from_source_dist(id, source_dist, hashes, index)
            }
        }
    }

    fn from_source_dist(
        id: &PackageId,
        source_dist: &distribution_types::SourceDist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Option<SourceDist>, LockError> {
        match *source_dist {
            distribution_types::SourceDist::Registry(ref reg_dist) => {
                SourceDist::from_registry_dist(reg_dist, index)
            }
            distribution_types::SourceDist::DirectUrl(ref direct_dist) => {
                SourceDist::from_direct_dist(id, direct_dist, hashes).map(Some)
            }
            // An actual sdist entry in the lockfile is only required when
            // it's from a registry or a direct URL. Otherwise, it's strictly
            // redundant with the information in all other kinds of `source`.
            distribution_types::SourceDist::Git(_)
            | distribution_types::SourceDist::Path(_)
            | distribution_types::SourceDist::Directory(_) => Ok(None),
        }
    }

    fn from_registry_dist(
        reg_dist: &RegistrySourceDist,
        index: Option<&IndexUrl>,
    ) -> Result<Option<SourceDist>, LockError> {
        // Reject distributions from registries that don't match the index URL, as can occur with
        // `--find-links`.
        if !index.is_some_and(|index| *index == reg_dist.index) {
            return Ok(None);
        }

        match &reg_dist.index {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                let url = normalize_file_location(&reg_dist.file.url)
                    .map_err(LockErrorKind::InvalidFileUrl)
                    .map_err(LockError::from)?;
                let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
                let size = reg_dist.file.size;
                Ok(Some(SourceDist::Url {
                    url,
                    metadata: SourceDistMetadata { hash, size },
                }))
            }
            IndexUrl::Path(path) => {
                let index_path = path.to_file_path().map_err(|()| LockErrorKind::UrlToPath)?;
                let reg_dist_path = reg_dist
                    .file
                    .url
                    .to_url()
                    .map_err(LockErrorKind::InvalidFileUrl)?
                    .to_file_path()
                    .map_err(|()| LockErrorKind::UrlToPath)?;
                let path = relative_to(&reg_dist_path, index_path)
                    .or_else(|_| std::path::absolute(&reg_dist_path))
                    .map_err(LockErrorKind::DistributionRelativePath)?;
                let hash = reg_dist.file.hashes.iter().max().cloned().map(Hash::from);
                let size = reg_dist.file.size;
                Ok(Some(SourceDist::Path {
                    path,
                    metadata: SourceDistMetadata { hash, size },
                }))
            }
        }
    }

    fn from_direct_dist(
        id: &PackageId,
        direct_dist: &DirectUrlSourceDist,
        hashes: &[HashDigest],
    ) -> Result<SourceDist, LockError> {
        let Some(hash) = hashes.iter().max().cloned().map(Hash::from) else {
            let kind = LockErrorKind::Hash {
                id: id.clone(),
                artifact_type: "direct URL source distribution",
                expected: true,
            };
            return Err(kind.into());
        };
        Ok(SourceDist::Url {
            url: normalize_url(direct_dist.url.to_url()),
            metadata: SourceDistMetadata {
                hash: Some(hash),
                size: None,
            },
        })
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
enum SourceDistWire {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
    Path {
        path: PortablePathBuf,
        #[serde(flatten)]
        metadata: SourceDistMetadata,
    },
}

impl SourceDist {
    /// Returns the TOML representation of this source distribution.
    fn to_toml(&self) -> anyhow::Result<InlineTable> {
        let mut table = InlineTable::new();
        match &self {
            SourceDist::Url { url, .. } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            SourceDist::Path { path, .. } => {
                table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
        }
        if let Some(hash) = self.hash() {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size() {
            table.insert("size", Value::from(i64::try_from(size)?));
        }
        Ok(table)
    }
}

impl TryFrom<SourceDistWire> for SourceDist {
    type Error = Infallible;

    fn try_from(wire: SourceDistWire) -> Result<SourceDist, Infallible> {
        match wire {
            SourceDistWire::Url { url, metadata } => Ok(SourceDist::Url { url, metadata }),
            SourceDistWire::Path { path, metadata } => Ok(SourceDist::Path {
                path: path.into(),
                metadata,
            }),
        }
    }
}

impl From<GitReference> for GitSourceKind {
    fn from(value: GitReference) -> Self {
        match value {
            GitReference::Branch(branch) => GitSourceKind::Branch(branch.to_string()),
            GitReference::Tag(tag) => GitSourceKind::Tag(tag.to_string()),
            GitReference::ShortCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::BranchOrTag(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::BranchOrTagOrCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::NamedRef(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::FullCommit(rev) => GitSourceKind::Rev(rev.to_string()),
            GitReference::DefaultBranch => GitSourceKind::DefaultBranch,
        }
    }
}

impl From<GitSourceKind> for GitReference {
    fn from(value: GitSourceKind) -> Self {
        match value {
            GitSourceKind::Branch(branch) => GitReference::Branch(branch),
            GitSourceKind::Tag(tag) => GitReference::Tag(tag),
            GitSourceKind::Rev(rev) => GitReference::from_rev(rev),
            GitSourceKind::DefaultBranch => GitReference::DefaultBranch,
        }
    }
}

/// Construct the lockfile-compatible [`URL`] for a [`GitSourceDist`].
fn locked_git_url(git_dist: &GitSourceDist) -> Url {
    let mut url = git_dist.git.repository().clone();

    // Redact the credentials.
    redact_git_credentials(&mut url);

    // Clear out any existing state.
    url.set_fragment(None);
    url.set_query(None);

    // Put the subdirectory in the query.
    if let Some(subdirectory) = git_dist.subdirectory.as_deref().and_then(Path::to_str) {
        url.query_pairs_mut()
            .append_pair("subdirectory", subdirectory);
    }

    // Put the requested reference in the query.
    match git_dist.git.reference() {
        GitReference::Branch(branch) => {
            url.query_pairs_mut()
                .append_pair("branch", branch.to_string().as_str());
        }
        GitReference::Tag(tag) => {
            url.query_pairs_mut()
                .append_pair("tag", tag.to_string().as_str());
        }
        GitReference::ShortCommit(rev)
        | GitReference::BranchOrTag(rev)
        | GitReference::BranchOrTagOrCommit(rev)
        | GitReference::NamedRef(rev)
        | GitReference::FullCommit(rev) => {
            url.query_pairs_mut()
                .append_pair("rev", rev.to_string().as_str());
        }
        GitReference::DefaultBranch => {}
    }

    // Put the precise commit in the fragment.
    url.set_fragment(
        git_dist
            .git
            .precise()
            .as_ref()
            .map(GitSha::to_string)
            .as_deref(),
    );

    url
}

/// Inspired by: <https://discuss.python.org/t/lock-files-again-but-this-time-w-sdists/46593>
#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(try_from = "WheelWire")]
struct Wheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    url: WheelWireSource,
    /// A hash of the built distribution.
    ///
    /// This is only present for wheels that come from registries and direct
    /// URLs. Wheels from git or path dependencies do not have hashes
    /// associated with them.
    hash: Option<Hash>,
    /// The size of the built distribution in bytes.
    ///
    /// This is only present for wheels that come from registries.
    size: Option<u64>,
    /// The filename of the wheel.
    ///
    /// This isn't part of the wire format since it's redundant with the
    /// URL. But we do use it for various things, and thus compute it at
    /// deserialization time. Not being able to extract a wheel filename from a
    /// wheel URL is thus a deserialization error.
    filename: WheelFilename,
}

impl Wheel {
    fn from_annotated_dist(annotated_dist: &AnnotatedDist) -> Result<Vec<Wheel>, LockError> {
        match annotated_dist.dist {
            // We pass empty installed packages for locking.
            ResolvedDist::Installed(_) => unreachable!(),
            ResolvedDist::Installable(ref dist) => {
                Wheel::from_dist(dist, &annotated_dist.hashes, annotated_dist.index())
            }
        }
    }

    fn from_dist(
        dist: &Dist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Wheel>, LockError> {
        match *dist {
            Dist::Built(ref built_dist) => Wheel::from_built_dist(built_dist, hashes, index),
            Dist::Source(distribution_types::SourceDist::Registry(ref source_dist)) => source_dist
                .wheels
                .iter()
                .filter(|wheel| {
                    // Reject distributions from registries that don't match the index URL, as can occur with
                    // `--find-links`.
                    index.is_some_and(|index| *index == wheel.index)
                })
                .map(Wheel::from_registry_wheel)
                .collect(),
            Dist::Source(_) => Ok(vec![]),
        }
    }

    fn from_built_dist(
        built_dist: &BuiltDist,
        hashes: &[HashDigest],
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Wheel>, LockError> {
        match *built_dist {
            BuiltDist::Registry(ref reg_dist) => Wheel::from_registry_dist(reg_dist, index),
            BuiltDist::DirectUrl(ref direct_dist) => {
                Ok(vec![Wheel::from_direct_dist(direct_dist, hashes)])
            }
            BuiltDist::Path(ref path_dist) => Ok(vec![Wheel::from_path_dist(path_dist, hashes)]),
        }
    }

    fn from_registry_dist(
        reg_dist: &RegistryBuiltDist,
        index: Option<&IndexUrl>,
    ) -> Result<Vec<Wheel>, LockError> {
        reg_dist
            .wheels
            .iter()
            .filter(|wheel| {
                // Reject distributions from registries that don't match the index URL, as can occur with
                // `--find-links`.
                index.is_some_and(|index| *index == wheel.index)
            })
            .map(Wheel::from_registry_wheel)
            .collect()
    }

    fn from_registry_wheel(wheel: &RegistryBuiltWheel) -> Result<Wheel, LockError> {
        let filename = wheel.filename.clone();
        match &wheel.index {
            IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                let url = normalize_file_location(&wheel.file.url)
                    .map_err(LockErrorKind::InvalidFileUrl)
                    .map_err(LockError::from)?;
                let hash = wheel.file.hashes.iter().max().cloned().map(Hash::from);
                let size = wheel.file.size;
                Ok(Wheel {
                    url: WheelWireSource::Url { url },
                    hash,
                    size,
                    filename,
                })
            }
            IndexUrl::Path(path) => {
                let index_path = path.to_file_path().map_err(|()| LockErrorKind::UrlToPath)?;
                let wheel_path = wheel
                    .file
                    .url
                    .to_url()
                    .map_err(LockErrorKind::InvalidFileUrl)?
                    .to_file_path()
                    .map_err(|()| LockErrorKind::UrlToPath)?;
                let path = relative_to(&wheel_path, index_path)
                    .or_else(|_| std::path::absolute(&wheel_path))
                    .map_err(LockErrorKind::DistributionRelativePath)?;
                Ok(Wheel {
                    url: WheelWireSource::Path { path },
                    hash: None,
                    size: None,
                    filename,
                })
            }
        }
    }

    fn from_direct_dist(direct_dist: &DirectUrlBuiltDist, hashes: &[HashDigest]) -> Wheel {
        Wheel {
            url: WheelWireSource::Url {
                url: normalize_url(direct_dist.url.to_url()),
            },
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            filename: direct_dist.filename.clone(),
        }
    }

    fn from_path_dist(path_dist: &PathBuiltDist, hashes: &[HashDigest]) -> Wheel {
        Wheel {
            url: WheelWireSource::Filename {
                filename: path_dist.filename.clone(),
            },
            hash: hashes.iter().max().cloned().map(Hash::from),
            size: None,
            filename: path_dist.filename.clone(),
        }
    }

    fn to_registry_dist(
        &self,
        source: &RegistrySource,
        root: &Path,
    ) -> Result<RegistryBuiltWheel, LockError> {
        let filename: WheelFilename = self.filename.clone();

        match source {
            RegistrySource::Url(index_url) => {
                let file_url = match &self.url {
                    WheelWireSource::Url { url } => url,
                    WheelWireSource::Path { .. } | WheelWireSource::Filename { .. } => {
                        return Err(LockErrorKind::MissingUrl {
                            name: filename.name,
                            version: filename.version,
                        }
                        .into())
                    }
                };
                let file = Box::new(distribution_types::File {
                    dist_info_metadata: false,
                    filename: filename.to_string(),
                    hashes: self.hash.iter().map(|h| h.0.clone()).collect(),
                    requires_python: None,
                    size: self.size,
                    upload_time_utc_ms: None,
                    url: FileLocation::AbsoluteUrl(file_url.clone()),
                    yanked: None,
                });
                let index = IndexUrl::from(VerbatimUrl::from_url(index_url.to_url()));
                Ok(RegistryBuiltWheel {
                    filename,
                    file,
                    index,
                })
            }
            RegistrySource::Path(index_path) => {
                let file_path = match &self.url {
                    WheelWireSource::Path { path } => path,
                    WheelWireSource::Url { .. } | WheelWireSource::Filename { .. } => {
                        return Err(LockErrorKind::MissingPath {
                            name: filename.name,
                            version: filename.version,
                        }
                        .into())
                    }
                };
                let file_url = Url::from_file_path(root.join(index_path).join(file_path))
                    .map_err(|()| LockErrorKind::PathToUrl)?;
                let file = Box::new(distribution_types::File {
                    dist_info_metadata: false,
                    filename: filename.to_string(),
                    hashes: self.hash.iter().map(|h| h.0.clone()).collect(),
                    requires_python: None,
                    size: self.size,
                    upload_time_utc_ms: None,
                    url: FileLocation::AbsoluteUrl(UrlString::from(file_url)),
                    yanked: None,
                });
                let index = IndexUrl::from(
                    VerbatimUrl::from_absolute_path(root.join(index_path))
                        .map_err(LockErrorKind::RegistryVerbatimUrl)?,
                );
                Ok(RegistryBuiltWheel {
                    filename,
                    file,
                    index,
                })
            }
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
struct WheelWire {
    #[serde(flatten)]
    url: WheelWireSource,
    /// A hash of the built distribution.
    ///
    /// This is only present for wheels that come from registries and direct
    /// URLs. Wheels from git or path dependencies do not have hashes
    /// associated with them.
    hash: Option<Hash>,
    /// The size of the built distribution in bytes.
    ///
    /// This is only present for wheels that come from registries.
    size: Option<u64>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum WheelWireSource {
    /// Used for all wheels that come from remote sources.
    Url {
        /// A URL where the wheel that was locked against was found. The location
        /// does not need to exist in the future, so this should be treated as
        /// only a hint to where to look and/or recording where the wheel file
        /// originally came from.
        url: UrlString,
    },
    /// Used for wheels that come from local registries (like `--find-links`).
    Path {
        /// The path to the wheel, relative to the index.
        path: PathBuf,
    },
    /// Used for path wheels.
    ///
    /// We only store the filename for path wheel, since we can't store a relative path in the url
    Filename {
        /// We duplicate the filename since a lot of code relies on having the filename on the
        /// wheel entry.
        filename: WheelFilename,
    },
}

impl Wheel {
    /// Returns the TOML representation of this wheel.
    fn to_toml(&self) -> anyhow::Result<InlineTable> {
        let mut table = InlineTable::new();
        match &self.url {
            WheelWireSource::Url { url } => {
                table.insert("url", Value::from(url.as_ref()));
            }
            WheelWireSource::Path { path } => {
                table.insert("path", Value::from(PortablePath::from(path).to_string()));
            }
            WheelWireSource::Filename { filename } => {
                table.insert("filename", Value::from(filename.to_string()));
            }
        }
        if let Some(ref hash) = self.hash {
            table.insert("hash", Value::from(hash.to_string()));
        }
        if let Some(size) = self.size {
            table.insert("size", Value::from(i64::try_from(size)?));
        }
        Ok(table)
    }
}

impl TryFrom<WheelWire> for Wheel {
    type Error = String;

    fn try_from(wire: WheelWire) -> Result<Wheel, String> {
        let filename = match &wire.url {
            WheelWireSource::Url { url } => {
                let filename = url.filename().map_err(|err| err.to_string())?;
                filename.parse::<WheelFilename>().map_err(|err| {
                    format!("failed to parse `{filename}` as wheel filename: {err}")
                })?
            }
            WheelWireSource::Path { path } => {
                let filename = path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .ok_or_else(|| {
                        format!("path `{}` has no filename component", path.display())
                    })?;
                filename.parse::<WheelFilename>().map_err(|err| {
                    format!("failed to parse `{filename}` as wheel filename: {err}")
                })?
            }
            WheelWireSource::Filename { filename } => filename.clone(),
        };

        Ok(Wheel {
            url: wire.url,
            hash: wire.hash,
            size: wire.size,
            filename,
        })
    }
}

/// A single dependency of a package in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
struct Dependency {
    package_id: PackageId,
    extra: BTreeSet<ExtraName>,
    /// A marker simplified by assuming `requires-python` is satisfied.
    /// So if `requires-python = '>=3.8'`, then
    /// `python_version >= '3.8' and python_version < '3.12'`
    /// gets simplfiied to `python_version < '3.12'`.
    ///
    /// Generally speaking, this marker should not be exposed to
    /// anything outside this module unless it's for a specialized use
    /// case. But specifically, it should never be used to evaluate
    /// against a marker environment or for disjointness checks or any
    /// other kind of marker algebra.
    ///
    /// It exists because there are some cases where we do actually
    /// want to compare markers in their "simplified" form. For
    /// example, when collapsing the extras on duplicate dependencies.
    /// Even if a dependency has different complexified markers,
    /// they might have identical markers once simplified. And since
    /// `requires-python` applies to the entire lock file, it's
    /// acceptable to do comparisons on the simplified form.
    simplified_marker: SimplifiedMarkerTree,
    /// The "complexified" marker is a marker that can stand on its
    /// own independent of `requires-python`. It can be safely used
    /// for any kind of marker algebra.
    complexified_marker: MarkerTree,
}

impl Dependency {
    fn new(
        requires_python: &RequiresPython,
        package_id: PackageId,
        extra: BTreeSet<ExtraName>,
        complexified_marker: MarkerTree,
    ) -> Dependency {
        let simplified_marker =
            SimplifiedMarkerTree::new(requires_python, complexified_marker.clone());
        Dependency {
            package_id,
            extra,
            simplified_marker,
            complexified_marker,
        }
    }

    fn from_annotated_dist(
        requires_python: &RequiresPython,
        annotated_dist: &AnnotatedDist,
        complexified_marker: MarkerTree,
        root: &Path,
    ) -> Result<Dependency, LockError> {
        let package_id = PackageId::from_annotated_dist(annotated_dist, root)?;
        let extra = annotated_dist.extra.iter().cloned().collect();
        Ok(Dependency::new(
            requires_python,
            package_id,
            extra,
            complexified_marker,
        ))
    }

    /// Returns the TOML representation of this dependency.
    fn to_toml(
        &self,
        _requires_python: &RequiresPython,
        dist_count_by_name: &FxHashMap<PackageName, u64>,
    ) -> Table {
        let mut table = Table::new();
        self.package_id
            .to_toml(Some(dist_count_by_name), &mut table);
        if !self.extra.is_empty() {
            let extra_array = self
                .extra
                .iter()
                .map(ToString::to_string)
                .collect::<Array>();
            table.insert("extra", value(extra_array));
        }
        if let Some(marker) = self.simplified_marker.try_to_string() {
            table.insert("marker", value(marker));
        }

        table
    }
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.extra.is_empty() {
            write!(
                f,
                "{}=={} @ {}",
                self.package_id.name, self.package_id.version, self.package_id.source
            )
        } else {
            write!(
                f,
                "{}[{}]=={} @ {}",
                self.package_id.name,
                self.extra.iter().join(","),
                self.package_id.version,
                self.package_id.source
            )
        }
    }
}

/// A single dependency of a package in a lockfile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
struct DependencyWire {
    #[serde(flatten)]
    package_id: PackageIdForDependency,
    #[serde(default)]
    extra: BTreeSet<ExtraName>,
    #[serde(default)]
    marker: SimplifiedMarkerTree,
}

impl DependencyWire {
    fn unwire(
        self,
        requires_python: &RequiresPython,
        unambiguous_package_ids: &FxHashMap<PackageName, PackageId>,
    ) -> Result<Dependency, LockError> {
        let complexified_marker = self.marker.clone().into_marker(requires_python);
        Ok(Dependency {
            package_id: self.package_id.unwire(unambiguous_package_ids)?,
            extra: self.extra,
            simplified_marker: self.marker,
            complexified_marker,
        })
    }
}

/// A single hash for a distribution artifact in a lockfile.
///
/// A hash is encoded as a single TOML string in the format
/// `{algorithm}:{digest}`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Hash(HashDigest);

impl From<HashDigest> for Hash {
    fn from(hd: HashDigest) -> Hash {
        Hash(hd)
    }
}

impl std::str::FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Hash, HashParseError> {
        let (algorithm, digest) = s.split_once(':').ok_or(HashParseError(
            "expected '{algorithm}:{digest}', but found no ':' in hash digest",
        ))?;
        let algorithm = algorithm
            .parse()
            .map_err(|_| HashParseError("unrecognized hash algorithm"))?;
        Ok(Hash(HashDigest {
            algorithm,
            digest: digest.into(),
        }))
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.0.algorithm, self.0.digest)
    }
}

impl<'de> serde::Deserialize<'de> for Hash {
    fn deserialize<D>(d: D) -> Result<Hash, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        string.parse().map_err(serde::de::Error::custom)
    }
}

/// Convert a [`FileLocation`] into a normalized [`UrlString`].
fn normalize_file_location(location: &FileLocation) -> Result<UrlString, ToUrlError> {
    match location {
        FileLocation::AbsoluteUrl(ref absolute) => Ok(absolute.as_base_url()),
        FileLocation::RelativeUrl(_, _) => Ok(normalize_url(location.to_url()?)),
    }
}

/// Convert a [`Url`] into a normalized [`UrlString`].
fn normalize_url(mut url: Url) -> UrlString {
    url.set_fragment(None);
    UrlString::from(url)
}

/// Normalize a [`Requirement`], which could come from a lockfile, a `pyproject.toml`, etc.
///
/// Performs the following steps:
///
/// 1. Removes any sensitive credentials.
/// 2. Ensures that the lock and install paths are appropriately framed with respect to the
///    current [`Workspace`].
/// 3. Removes the `origin` field, which is only used in `requirements.txt`.
fn normalize_requirement(
    requirement: Requirement,
    workspace: &Workspace,
) -> Result<Requirement, LockError> {
    match requirement.source {
        RequirementSource::Git {
            mut repository,
            reference,
            precise,
            subdirectory,
            url,
        } => {
            // Redact the credentials.
            redact_git_credentials(&mut repository);

            // Redact the PEP 508 URL.
            let mut url = url.to_url();
            redact_git_credentials(&mut url);
            let url = VerbatimUrl::from_url(url);

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                marker: requirement.marker,
                source: RequirementSource::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory,
                    url,
                },
                origin: None,
            })
        }
        RequirementSource::Path {
            install_path,
            ext,
            url: _,
        } => {
            let install_path = uv_fs::normalize_path(&workspace.install_path().join(&install_path));
            let url = VerbatimUrl::from_absolute_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                marker: requirement.marker,
                source: RequirementSource::Path {
                    install_path,
                    ext,
                    url,
                },
                origin: None,
            })
        }
        RequirementSource::Directory {
            install_path,
            editable,
            r#virtual,
            url: _,
        } => {
            let install_path = uv_fs::normalize_path(&workspace.install_path().join(&install_path));
            let url = VerbatimUrl::from_absolute_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            Ok(Requirement {
                name: requirement.name,
                extras: requirement.extras,
                marker: requirement.marker,
                source: RequirementSource::Directory {
                    install_path,
                    editable,
                    r#virtual,
                    url,
                },
                origin: None,
            })
        }
        _ => Ok(Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            source: requirement.source,
            origin: None,
        }),
    }
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct LockError(Box<LockErrorKind>);

impl<E> From<E> for LockError
where
    LockErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        LockError(Box::new(LockErrorKind::from(err)))
    }
}

/// An error that occurs when generating a `Lock` data structure.
///
/// These errors are sometimes the result of possible programming bugs.
/// For example, if there are two or more duplicative distributions given
/// to `Lock::new`, then an error is returned. It's likely that the fault
/// is with the caller somewhere in such cases.
#[derive(Debug, thiserror::Error)]
enum LockErrorKind {
    /// An error that occurs when multiple packages with the same
    /// ID were found.
    #[error("found duplicate package `{id}`")]
    DuplicatePackage {
        /// The ID of the conflicting package.
        id: PackageId,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers.
    #[error("for package `{id}`, found duplicate dependency `{dependency}`")]
    DuplicateDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers, as part of the
    /// that package's optional dependencies.
    #[error("for package `{id}[{extra}]`, found duplicate dependency `{dependency}`")]
    DuplicateOptionalDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The name of the optional dependency group.
        extra: ExtraName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when there are multiple dependencies for the
    /// same package that have identical identifiers, as part of the
    /// that package's development dependencies.
    #[error("for package `{id}:{group}`, found duplicate dependency `{dependency}`")]
    DuplicateDevDependency {
        /// The ID of the package for which a duplicate dependency was
        /// found.
        id: PackageId,
        /// The name of the dev dependency group.
        group: GroupName,
        /// The ID of the conflicting dependency.
        dependency: Dependency,
    },
    /// An error that occurs when the URL to a file for a wheel or
    /// source dist could not be converted to a structured `url::Url`.
    #[error("failed to parse wheel or source distribution URL")]
    InvalidFileUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in its error message.
        #[source]
        ToUrlError,
    ),
    /// An error that occurs when the extension can't be determined
    /// for a given wheel or source distribution.
    #[error("failed to parse file extension; expected one of: {0}")]
    MissingExtension(#[from] ExtensionError),
    /// Failed to parse a git source URL.
    #[error("failed to parse source git URL")]
    InvalidGitSourceUrl(
        /// The underlying error that occurred. This includes the
        /// errant URL in the message.
        #[source]
        SourceParseError,
    ),
    /// An error that occurs when there's an unrecognized dependency.
    ///
    /// That is, a dependency for a package that isn't in the lockfile.
    #[error("for package `{id}`, found dependency `{dependency}` with no locked package")]
    UnrecognizedDependency {
        /// The ID of the package that has an unrecognized dependency.
        id: PackageId,
        /// The ID of the dependency that doesn't have a corresponding package
        /// entry.
        dependency: Dependency,
    },
    /// An error that occurs when a hash is expected (or not) for a particular
    /// artifact, but one was not found (or was).
    #[error("since the package `{id}` comes from a {source} dependency, a hash was {expected} but one was not found for {artifact_type}", source = id.source.name(), expected = if *expected { "expected" } else { "not expected" })]
    Hash {
        /// The ID of the package that has a missing hash.
        id: PackageId,
        /// The specific type of artifact, e.g., "source package"
        /// or "wheel".
        artifact_type: &'static str,
        /// When true, a hash is expected to be present.
        expected: bool,
    },
    /// An error that occurs when a package is included with an extra name,
    /// but no corresponding base package (i.e., without the extra) exists.
    #[error("found package `{id}` with extra `{extra}` but no base package")]
    MissingExtraBase {
        /// The ID of the package that has a missing base.
        id: PackageId,
        /// The extra name that was found.
        extra: ExtraName,
    },
    /// An error that occurs when a package is included with a development
    /// dependency group, but no corresponding base package (i.e., without
    /// the group) exists.
    #[error(
        "found package `{id}` with development dependency group `{group}` but no base package"
    )]
    MissingDevBase {
        /// The ID of the package that has a missing base.
        id: PackageId,
        /// The development dependency group that was found.
        group: GroupName,
    },
    /// An error that occurs from an invalid lockfile where a wheel comes from a non-wheel source
    /// such as a directory.
    #[error("wheels cannot come from {source_type} sources")]
    InvalidWheelSource {
        /// The ID of the distribution that has a missing base.
        id: PackageId,
        /// The kind of the invalid source.
        source_type: &'static str,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a remote
    /// registry, but is missing a URL.
    #[error("found registry distribution {name}=={version} without a valid URL")]
    MissingUrl {
        /// The name of the distribution that is missing a URL.
        name: PackageName,
        /// The version of the distribution that is missing a URL.
        version: Version,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a local registry,
    /// but is missing a path.
    #[error("found registry distribution {name}=={version} without a valid path")]
    MissingPath {
        /// The name of the distribution that is missing a path.
        name: PackageName,
        /// The version of the distribution that is missing a path.
        version: Version,
    },
    /// An error that occurs when a distribution indicates that it is sourced from a registry, but
    /// is missing a filename.
    #[error("found registry distribution {id} without a valid filename")]
    MissingFilename {
        /// The ID of the distribution that is missing a filename.
        id: PackageId,
    },
    /// An error that occurs when a distribution is included with neither wheels nor a source
    /// distribution.
    #[error("distribution {id} can't be installed because it doesn't have a source distribution or wheel for the current platform")]
    NeitherSourceDistNorWheel {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as both `--no-binary` and `--no-build`.
    #[error("distribution {id} can't be installed because it is marked as both `--no-binary` and `--no-build`")]
    NoBinaryNoBuild {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as `--no-binary`, but no source
    /// distribution is available.
    #[error("distribution {id} can't be installed because it is marked as `--no-binary` but has no source distribution")]
    NoBinary {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a distribution is marked as `--no-build`, but no binary
    /// distribution is available.
    #[error("distribution {id} can't be installed because it is marked as `--no-build` but has no binary distribution")]
    NoBuild {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a wheel-only distribution is incompatible with the current
    /// platform.
    #[error(
        "distribution {id} can't be installed because the binary distribution is incompatible with the current platform"
    )]
    IncompatibleWheelOnly {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when a wheel-only source is marked as `--no-binary`.
    #[error("distribution {id} can't be installed because it is marked as `--no-binary` but is itself a binary distribution")]
    NoBinaryWheelOnly {
        /// The ID of the distribution.
        id: PackageId,
    },
    /// An error that occurs when converting between URLs and paths.
    #[error("found dependency `{id}` with no locked distribution")]
    VerbatimUrl {
        /// The ID of the distribution that has a missing base.
        id: PackageId,
        /// The inner error we forward.
        #[source]
        err: VerbatimUrlError,
    },
    /// An error that occurs when parsing an existing requirement.
    #[error("could not compute relative path between workspace and distribution")]
    DistributionRelativePath(
        /// The inner error we forward.
        #[source]
        std::io::Error,
    ),
    /// An error that occurs when converting an index URL to a relative path
    #[error("could not compute relative path between workspace and index")]
    IndexRelativePath(
        /// The inner error we forward.
        #[source]
        std::io::Error,
    ),
    /// An error that occurs when an ambiguous `package.dependency` is
    /// missing a `version` field.
    #[error(
        "dependency {name} has missing `version` \
         field but has more than one matching package"
    )]
    MissingDependencyVersion {
        /// The name of the dependency that is missing a `version` field.
        name: PackageName,
    },
    /// An error that occurs when an ambiguous `package.dependency` is
    /// missing a `source` field.
    #[error(
        "dependency {name} has missing `source` \
         field but has more than one matching package"
    )]
    MissingDependencySource {
        /// The name of the dependency that is missing a `source` field.
        name: PackageName,
    },
    /// An error that occurs when parsing an existing requirement.
    #[error("could not compute relative path between workspace and requirement")]
    RequirementRelativePath(
        /// The inner error we forward.
        #[source]
        std::io::Error,
    ),
    /// An error that occurs when parsing an existing requirement.
    #[error("could not convert between URL and path")]
    RequirementVerbatimUrl(
        /// The inner error we forward.
        #[source]
        VerbatimUrlError,
    ),
    /// An error that occurs when parsing a registry's index URL.
    #[error("could not convert between URL and path")]
    RegistryVerbatimUrl(
        /// The inner error we forward.
        #[source]
        VerbatimUrlError,
    ),
    /// An error that occurs when converting a path to a URL.
    #[error("failed to convert path to URL")]
    PathToUrl,
    /// An error that occurs when converting a URL to a path
    #[error("failed to convert URL to path")]
    UrlToPath,
    /// An error that occurs when multiple packages with the same
    /// name were found when identifying the root packages.
    #[error("found multiple packages matching `{name}`")]
    MultipleRootPackages {
        /// The ID of the package.
        name: PackageName,
    },
    /// An error that occurs when a root package can't be found.
    #[error("could not find root package `{name}`")]
    MissingRootPackage {
        /// The ID of the package.
        name: PackageName,
    },
}

/// An error that occurs when a source string could not be parsed.
#[derive(Clone, Debug, thiserror::Error)]
enum SourceParseError {
    /// An error that occurs when the URL in the source is invalid.
    #[error("invalid URL in source `{given}`")]
    InvalidUrl {
        /// The source string given.
        given: String,
        /// The URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when a Git URL is missing a precise commit SHA.
    #[error("missing SHA in source `{given}`")]
    MissingSha {
        /// The source string given.
        given: String,
    },
    /// An error that occurs when a Git URL has an invalid SHA.
    #[error("invalid SHA in source `{given}`")]
    InvalidSha {
        /// The source string given.
        given: String,
    },
}

/// An error that occurs when a hash digest could not be parsed.
#[derive(Clone, Debug, Eq, PartialEq)]
struct HashParseError(&'static str);

impl std::error::Error for HashParseError {}

impl std::fmt::Display for HashParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Display::fmt(self.0, f)
    }
}

/// Format an array so that each element is on its own line and has a trailing comma.
///
/// Example:
///
/// ```toml
/// dependencies = [
///     { name = "idna" },
///     { name = "sniffio" },
/// ]
/// ```
fn each_element_on_its_line_array(elements: impl Iterator<Item = impl Into<Value>>) -> Array {
    let mut array = elements
        .map(|item| {
            let mut value = item.into();
            // Each dependency is on its own line and indented.
            value.decor_mut().set_prefix("\n    ");
            value
        })
        .collect::<Array>();
    // With a trailing comma, inserting another entry doesn't change the preceding line,
    // reducing the diff noise.
    array.set_trailing_comma(true);
    // The line break between the last element's comma and the closing square bracket.
    array.set_trailing("\n");
    array
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dependency_source_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_version_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_version_unambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_version_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn missing_dependency_source_version_ambiguous() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_optional_missing() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_optional_present() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn hash_required_present() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { path = "file:///foo/bar" }
wheels = [{ url = "file:///foo/bar/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_direct_no_subdir() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_direct_has_subdir() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net", subdirectory = "wat/foo/bar" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_directory() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { directory = "path/to/dir" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn source_editable() {
        let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { editable = "path/to/dir" }
"#;
        let result: Result<Lock, _> = toml::from_str(data);
        insta::assert_debug_snapshot!(result);
    }
}
