use std::collections::BTreeMap;
use std::fmt::Display;

use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{RequiresPython, UrlString};
use uv_fs::PortablePathBuf;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pypi_types::{ConflictItem, ConflictKind, ConflictSet, Conflicts, ModuleName};
use uv_workspace::Workspace;

use crate::Lock;
use crate::lock::{
    Dependency, DirectSource, PackageId, RegistrySource, Source, SourceDist, SourceDistMetadata,
    Wheel, WheelWireSource, ZstdWheel,
};

#[derive(Debug, thiserror::Error)]
enum MetadataErrorKind {
    #[error(transparent)]
    Serialize(#[from] serde_json::error::Error),
}

#[derive(Debug)]
pub struct MetadataError {
    kind: Box<MetadataErrorKind>,
}

impl std::error::Error for MetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.kind.source()
    }
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        Ok(())
    }
}

impl<E> From<E> for MetadataError
where
    MetadataErrorKind: From<E>,
{
    fn from(err: E) -> Self {
        Self {
            kind: Box::new(MetadataErrorKind::from(err)),
        }
    }
}

/// The full `uv workspace metadata` JSON object
#[derive(Debug, serde::Serialize)]
pub struct Metadata {
    /// Format information
    schema: SchemaReport,
    /// Absolute path to the workspace root
    ///
    /// Ideally absolute paths to things that are found in subdirs of this should have exactly
    /// this as a prefix so it can be stripped to get relative paths if one wants.
    workspace_root: PortablePathBuf,
    /// The version of python required by the workspace
    ///
    /// Every `marker` we emit implicitly assumes this constraint to keep things clean
    requires_python: RequiresPython,
    /// Info about conflicting packages
    conflicts: MetadataConflicts,
    /// A mapping from importable module names to the package names that provide them
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    module_owners: BTreeMap<ModuleName, Vec<PackageName>>,
    /// An index of which nodes are workspace members
    ///
    /// These entries are often what you should use as the entry-points into the `resolve` graph.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    members: Vec<MetadataWorkspaceMember>,
    /// The dependency graph
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    resolution: BTreeMap<MetadataNodeIdFlat, MetadataNode>,
}

/// The schema version for the metadata report.
#[derive(serde::Serialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    /// An unstable, experimental schema.
    #[default]
    Preview,
}

/// The schema metadata for the metadata report.
#[derive(serde::Serialize, Debug, Default)]
struct SchemaReport {
    /// The version of the schema.
    version: SchemaVersion,
}

/// Info for looking up workspace members, most information is stored in the node behind `id`
#[derive(Debug, serde::Serialize)]
struct MetadataWorkspaceMember {
    /// Package name
    name: PackageName,
    /// Absolute path to the member
    path: PortablePathBuf,
    /// Key for the package's node in the `resolve` graph
    id: MetadataNodeIdFlat,
}

/// A node in the dependency graph
///
/// There are 4 kinds of nodes:
///
/// * packages: `mypackage==1.0.0@registry+https://pypi.org/simple`
/// * extras:   `mypackage[myextra]==1.0.0@registry+https://pypi.org/simple`
/// * groups:   `mypackage:mygroup==1.0.0@registry+https://pypi.org/simple`
/// * build:    `mypackage(build)==1.0.0@registry+https://pypi.org/simple`
///
/// -----------
///
/// A package like this:
///
/// ```toml
/// [project]
/// name = "mypackage"
/// version = 1.0.0
///
/// dependencies = ["httpx"]
///
/// [project.optional-dependencies]
/// cli = ["rich"]
///
/// [dependency-groups]
/// dev = ["typing-extensions"]
///
/// [build-system]
/// requires = ["hatchling"]
/// ```
///
/// will get 4 nodes with the following edges (Version and Source omitted here for brevity):
///
/// * `mypackage`
///   * `httpx`
/// * `mypackage(build)`
///   * `hatchling`
/// * `mypackage[cli]`
///   * `mypackage`
///   * `rich`
/// * `mypackage:dev`
///   * `typing-extensions`
///
/// Note that `mypackage[cli]` has a dependency edge on `mypackage` while `mypackage:dev` does not.
/// This is because `mypackage[cli]` is fundamentally an augmentation of `mypackage` while `mypackage:dev`
/// is just a list of packages that happens to be defined by `mypackage`'s pyproject.toml.
#[derive(Debug, Clone, serde::Serialize)]
struct MetadataNode {
    /// A unique id for this node that will be used to refer to it
    #[serde(flatten)]
    id: MetadataNodeId,
    /// Dependencies of this node (the edges of The Graph)
    dependencies: Vec<MetadataDependency>,
    /// Extras
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    optional_dependencies: Vec<MetadataExtra>,
    /// Groups
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    dependency_groups: Vec<MetadataGroup>,
    /// Info about building the package
    #[serde(skip_serializing_if = "Option::is_none", default)]
    build_system: Option<MetadataBuildSystem>,
    /// The source distribution found
    #[serde(skip_serializing_if = "Option::is_none", default)]
    sdist: Option<MetadataSourceDist>,
    /// Wheels we found
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    wheels: Vec<MetadataWheel>,
}

impl MetadataNode {
    fn new(id: MetadataNodeId) -> Self {
        Self {
            id,
            dependencies: Vec::new(),
            dependency_groups: Vec::new(),
            optional_dependencies: Vec::new(),
            wheels: Vec::new(),
            build_system: None,
            sdist: None,
        }
    }

    fn from_package_id(
        workspace_root: &PortablePathBuf,
        id: &PackageId,
        kind: MetadataNodeKind,
    ) -> Self {
        Self::new(MetadataNodeId::from_package_id(workspace_root, id, kind))
    }

    fn add_dependency(&mut self, workspace_root: &PortablePathBuf, dependency: &Dependency) {
        let extras = dependency.extra();
        if extras.is_empty() {
            let id = MetadataNodeId::from_package_id(
                workspace_root,
                &dependency.package_id,
                MetadataNodeKind::Package,
            );
            self.dependencies.push(MetadataDependency {
                id: id.to_flat(),
                marker: dependency.simplified_marker.try_to_string(),
            });
            return;
        }
        for extra in extras {
            let id = MetadataNodeId::from_package_id(
                workspace_root,
                &dependency.package_id,
                MetadataNodeKind::Extra(extra.clone()),
            );
            self.dependencies.push(MetadataDependency {
                id: id.to_flat(),
                marker: dependency.simplified_marker.try_to_string(),
            });
        }
    }
}

/// The unique key for every node in the graph
///
/// (It's not entirely clear to me that two nodes can differ only by `source` but it doesn't hurt.)
#[derive(Debug, Clone, serde::Serialize)]
struct MetadataNodeId {
    /// The name of the package
    name: PackageName,
    /// The version of the package, if any could be found (source trees may have no version)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    version: Option<Version>,
    /// The source of the package (directory, registry, URL...)
    source: MetadataSource,
    /// What kind of node is this?
    kind: MetadataNodeKind,
}

/// This is intended to be an opaque unique id for referring to a node
///
/// It's human readable for convenience but parsing it or relying on it is inadvisable.
/// As currently implemented this is just a concatenation of the 4 fields in `MetadataNodeId`
/// which every node includes, so parsing it is just making more work for yourself.
type MetadataNodeIdFlat = String;

impl MetadataNodeId {
    fn from_package_id(
        workspace_root: &PortablePathBuf,
        id: &PackageId,
        kind: MetadataNodeKind,
    ) -> Self {
        let name = id.name.clone();
        let version = id.version.clone();
        let source = MetadataSource::from_source(workspace_root, id.source.clone());

        Self {
            name,
            version,
            source,
            kind,
        }
    }

    fn to_flat(&self) -> MetadataNodeIdFlat {
        self.to_string()
    }
}

impl Display for MetadataNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.version {
            Some(version) => write!(f, "{}{}=={version}@{}", self.name, self.kind, self.source),
            None => write!(f, "{}{}@{}", self.name, self.kind, self.source),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct MetadataDependency {
    id: MetadataNodeIdFlat,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    marker: Option<MetadataMarker>,
}

type MetadataMarker = String;

/// The kind a node can have in the dependency graph
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum MetadataNodeKind {
    /// The node is the package itself
    /// its edges are `project.dependencies`
    Package,
    /// The node is for building the package's sdist into a wheel
    /// its edges are `build-system.requires`
    #[expect(dead_code)]
    Build,
    /// The node is for an extra defined on the package
    /// its edges are `project.optional-dependencies.myextra`
    Extra(ExtraName),
    /// The node is for a dependency-group defined on the package
    /// its edges are `dependency-groups.mygroup`
    Group(GroupName),
}

impl Display for MetadataNodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Don't apply any special decoration, this is the default
            Self::Package => Ok(()),
            Self::Build => f.write_str("(build)"),
            Self::Extra(extra_name) => write!(f, "[{extra_name}]"),
            Self::Group(group_name) => write!(f, ":{group_name}"),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged, rename_all = "snake_case")]
enum MetadataSource {
    Registry {
        registry: MetadataRegistrySource,
    },
    Git {
        git: UrlString,
    },
    Direct {
        url: UrlString,
        subdirectory: Option<PortablePathBuf>,
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

impl Display for MetadataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Registry {
                registry: MetadataRegistrySource::Url(url),
            }
            | Self::Git { git: url }
            | Self::Direct { url, .. } => {
                write!(f, "{}+{}", self.name(), url)
            }
            Self::Registry {
                registry: MetadataRegistrySource::Path(path),
            }
            | Self::Path { path }
            | Self::Directory { directory: path }
            | Self::Editable { editable: path }
            | Self::Virtual { r#virtual: path } => {
                write!(f, "{}+{}", self.name(), path)
            }
        }
    }
}

impl MetadataSource {
    fn name(&self) -> &str {
        match self {
            Self::Registry { .. } => "registry",
            Self::Git { .. } => "git",
            Self::Direct { .. } => "direct",
            Self::Path { .. } => "path",
            Self::Directory { .. } => "directory",
            Self::Editable { .. } => "editable",
            Self::Virtual { .. } => "virtual",
        }
    }
}

impl MetadataSource {
    fn from_source(workspace_root: &PortablePathBuf, source: Source) -> Self {
        match source {
            Source::Registry(source) => match source {
                RegistrySource::Url(url) => Self::Registry {
                    registry: MetadataRegistrySource::Url(url),
                },
                RegistrySource::Path(path) => Self::Registry {
                    registry: MetadataRegistrySource::Path(normalize_workspace_relative_path(
                        workspace_root,
                        &path,
                    )),
                },
            },
            Source::Git(url, _) => Self::Git { git: url },
            Source::Direct(url, DirectSource { subdirectory }) => Self::Direct {
                url,
                subdirectory: subdirectory
                    .map(|path| normalize_workspace_relative_path(workspace_root, &path)),
            },
            Source::Path(path) => Self::Path {
                path: normalize_workspace_relative_path(workspace_root, &path),
            },
            Source::Directory(path) => Self::Directory {
                directory: normalize_workspace_relative_path(workspace_root, &path),
            },
            Source::Editable(path) => Self::Editable {
                editable: normalize_workspace_relative_path(workspace_root, &path),
            },
            Source::Virtual(path) => Self::Virtual {
                r#virtual: normalize_workspace_relative_path(workspace_root, &path),
            },
        }
    }
}

fn normalize_workspace_relative_path(
    workspace_root: &PortablePathBuf,
    maybe_rel: &std::path::Path,
) -> PortablePathBuf {
    if maybe_rel.is_absolute() {
        PortablePathBuf::from(maybe_rel)
    } else {
        PortablePathBuf::from(workspace_root.as_ref().join(maybe_rel).as_path())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum MetadataRegistrySource {
    /// Ex) `https://pypi.org/simple`
    Url(UrlString),
    /// Ex) `/path/to/local/index`
    Path(PortablePathBuf),
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged, rename_all = "snake_case")]
enum MetadataSourceDist {
    Url {
        url: UrlString,
        #[serde(flatten)]
        metadata: MetadataSourceDistMetadata,
    },
    Path {
        path: PortablePathBuf,
        #[serde(flatten)]
        metadata: MetadataSourceDistMetadata,
    },
    Metadata {
        #[serde(flatten)]
        metadata: MetadataSourceDistMetadata,
    },
}

impl MetadataSourceDist {
    fn from_sdist(workspace_root: &PortablePathBuf, sdist: &SourceDist) -> Self {
        match sdist {
            SourceDist::Url { url, metadata } => Self::Url {
                url: url.clone(),
                metadata: MetadataSourceDistMetadata::from_sdist(metadata),
            },
            SourceDist::Path { path, metadata } => Self::Path {
                path: normalize_workspace_relative_path(workspace_root, path),
                metadata: MetadataSourceDistMetadata::from_sdist(metadata),
            },
            SourceDist::Metadata { metadata } => Self::Metadata {
                metadata: MetadataSourceDistMetadata::from_sdist(metadata),
            },
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct MetadataSourceDistMetadata {
    /// A hash of the source distribution.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    hashes: BTreeMap<HashAlgorithm, Hash>,
    /// The size of the source distribution in bytes.
    ///
    /// This is only present for source distributions that come from registries.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    size: Option<u64>,
    /// The upload time of the source distribution.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    upload_time: Option<jiff::Timestamp>,
}

/// The name of a hash algorithm ("sha256", "blake2b", "md5", etc)
type HashAlgorithm = String;
/// A hex encoded digest of the file
type Hash = String;

/// Oh you wanted a hash map? No this is the hashes map, a sorted map of hashes!
///
/// We prefer matching PEP 691 (JSON-based Simple API for Python) here for future-proofing
/// and convenience of consumption.
fn hashes_map(hash: &crate::lock::Hash) -> BTreeMap<HashAlgorithm, Hash> {
    Some((hash.0.algorithm.to_string(), hash.0.digest.to_string()))
        .into_iter()
        .collect()
}

impl MetadataSourceDistMetadata {
    fn from_sdist(sdist: &SourceDistMetadata) -> Self {
        Self {
            hashes: sdist.hash.as_ref().map(hashes_map).unwrap_or_default(),
            size: sdist.size,
            upload_time: sdist.upload_time,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
struct MetadataWheel {
    /// A URL or file path (via `file://`) where the wheel that was locked
    /// against was found. The location does not need to exist in the future,
    /// so this should be treated as only a hint to where to look and/or
    /// recording where the wheel file originally came from.
    #[serde(flatten)]
    source: Option<MetadataWheelWireSource>,
    /// A hash of the built distribution.
    ///
    /// This is only present for wheels that come from registries and direct
    /// URLs. Wheels from git or path dependencies do not have hashes
    /// associated with them.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    hashes: BTreeMap<HashAlgorithm, Hash>,
    /// The size of the built distribution in bytes.
    ///
    /// This is only present for wheels that come from registries.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    size: Option<u64>,
    /// The upload time of the built distribution.
    ///
    /// This is only present for wheels that come from registries.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    upload_time: Option<jiff::Timestamp>,
    /// The filename of the wheel.
    ///
    /// This isn't part of the wire format since it's redundant with the
    /// URL. But we do use it for various things, and thus compute it at
    /// deserialization time. Not being able to extract a wheel filename from a
    /// wheel URL is thus a deserialization error.
    filename: WheelFilename,
    /// The zstandard-compressed wheel metadata, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    zstd: Option<MetadataZstdWheel>,
}

impl MetadataWheel {
    fn from_wheel(workspace_root: &PortablePathBuf, wheel: &Wheel) -> Self {
        Self {
            source: MetadataWheelWireSource::from_wheel(workspace_root, &wheel.url),
            hashes: wheel.hash.as_ref().map(hashes_map).unwrap_or_default(),
            size: wheel.size,
            upload_time: wheel.upload_time,
            filename: wheel.filename.clone(),
            zstd: wheel.zstd.as_ref().map(MetadataZstdWheel::from_wheel),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged, rename_all = "snake_case")]
enum MetadataWheelWireSource {
    Url { url: UrlString },
    Path { path: PortablePathBuf },
}

impl MetadataWheelWireSource {
    fn from_wheel(workspace_root: &PortablePathBuf, wheel: &WheelWireSource) -> Option<Self> {
        match wheel {
            WheelWireSource::Url { url } => Some(Self::Url { url: url.clone() }),
            WheelWireSource::Path { path } => Some(Self::Path {
                path: normalize_workspace_relative_path(workspace_root, path),
            }),
            // We guarantee this as a separate field so it's redundant
            WheelWireSource::Filename { .. } => None,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataZstdWheel {
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    hashes: BTreeMap<HashAlgorithm, Hash>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    size: Option<u64>,
}

impl MetadataZstdWheel {
    fn from_wheel(wheel: &ZstdWheel) -> Self {
        Self {
            hashes: wheel.hash.as_ref().map(hashes_map).unwrap_or_default(),
            size: wheel.size,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataExtra {
    name: ExtraName,
    id: MetadataNodeIdFlat,
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataGroup {
    name: GroupName,
    id: MetadataNodeIdFlat,
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataBuildSystem {
    /// The `build-backend` specified in the pyproject.toml
    build_backend: String,
    id: MetadataNodeIdFlat,
}

/// Conflicts
#[derive(Clone, Debug, serde::Serialize)]
struct MetadataConflicts {
    sets: Vec<MetadataConflictSet>,
}

impl MetadataConflicts {
    fn from_conflicts(
        members: &[MetadataWorkspaceMember],
        resolve: &BTreeMap<MetadataNodeIdFlat, MetadataNode>,
        conflicts: &Conflicts,
    ) -> Self {
        Self {
            sets: conflicts
                .iter()
                .map(|set| MetadataConflictSet::from_conflicts(members, resolve, set))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataConflictSet {
    items: Vec<MetadataConflictItem>,
}

impl MetadataConflictSet {
    fn from_conflicts(
        members: &[MetadataWorkspaceMember],
        resolve: &BTreeMap<MetadataNodeIdFlat, MetadataNode>,
        set: &ConflictSet,
    ) -> Self {
        Self {
            items: set
                .iter()
                .map(|item| MetadataConflictItem::from_conflicts(members, resolve, item))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct MetadataConflictItem {
    /// These should always be names of packages referred to in [`Metadata::members`]
    package: PackageName,
    kind: MetadataConflictKind,
    /// This should never be None (should be a validation error way earlier in uv)
    /// ...but I'd rather not error if wrong.
    id: Option<MetadataNodeIdFlat>,
}

impl MetadataConflictItem {
    fn from_conflicts(
        members: &[MetadataWorkspaceMember],
        resolve: &BTreeMap<MetadataNodeIdFlat, MetadataNode>,
        item: &ConflictItem,
    ) -> Self {
        let kind = MetadataConflictKind::from_conflicts(item.kind());
        let id = members
            .iter()
            .find(|member| &member.name == item.package())
            .and_then(|member| {
                let package_node = resolve.get(&member.id)?;
                let id = MetadataNodeId {
                    kind: kind.to_node_kind(),
                    ..package_node.id.clone()
                };
                Some(id.to_flat())
            });
        Self {
            package: item.package().clone(),
            kind,
            id,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
enum MetadataConflictKind {
    Group(GroupName),
    Extra(ExtraName),
    Project,
}

impl MetadataConflictKind {
    fn from_conflicts(item: &ConflictKind) -> Self {
        match item {
            ConflictKind::Extra(name) => Self::Extra(name.clone()),
            ConflictKind::Group(name) => Self::Group(name.clone()),
            ConflictKind::Project => Self::Project,
        }
    }

    fn to_node_kind(&self) -> MetadataNodeKind {
        match self {
            Self::Group(name) => MetadataNodeKind::Group(name.clone()),
            Self::Extra(name) => MetadataNodeKind::Extra(name.clone()),
            Self::Project => MetadataNodeKind::Package,
        }
    }
}

impl Metadata {
    /// Construct a [`PylockToml`] from a uv lockfile.
    pub fn from_lock(workspace: &Workspace, lock: &Lock) -> Result<Self, MetadataError> {
        let mut resolve = BTreeMap::new();
        let mut members = Vec::new();
        let workspace_root = PortablePathBuf::from(workspace.install_path().as_path());

        for lock_package in lock.packages() {
            let mut meta_package = MetadataNode::from_package_id(
                &workspace_root,
                &lock_package.id,
                MetadataNodeKind::Package,
            );

            // Direct dependencies go on the package node
            for dependency in &lock_package.dependencies {
                meta_package.add_dependency(&workspace_root, dependency);
            }

            // Extras get their own nodes
            for (extra, dependencies) in &lock_package.optional_dependencies {
                let mut meta_extra = MetadataNode::from_package_id(
                    &workspace_root,
                    &lock_package.id,
                    MetadataNodeKind::Extra(extra.clone()),
                );
                // Extras always depend on the base package
                meta_extra.dependencies.push(MetadataDependency {
                    id: meta_package.id.to_flat(),
                    marker: None,
                });
                for dependency in dependencies {
                    meta_extra.add_dependency(&workspace_root, dependency);
                }

                meta_package.optional_dependencies.push(MetadataExtra {
                    name: extra.clone(),
                    id: meta_extra.id.to_flat(),
                });

                resolve.insert(meta_extra.id.to_flat(), meta_extra);
            }

            // Groups get their own nodes
            for (group, dependencies) in &lock_package.dependency_groups {
                let mut meta_group = MetadataNode::from_package_id(
                    &workspace_root,
                    &lock_package.id,
                    MetadataNodeKind::Group(group.clone()),
                );
                // Groups *do not* depend on the base package, so don't add that
                for dependency in dependencies {
                    meta_group.add_dependency(&workspace_root, dependency);
                }

                meta_package.dependency_groups.push(MetadataGroup {
                    name: group.clone(),
                    id: meta_group.id.to_flat(),
                });

                resolve.insert(meta_group.id.to_flat(), meta_group);
            }

            // Register this package if it appears to be a workspace member
            if let Some(workspace_package) = workspace.packages().get(lock_package.name()) {
                let member = MetadataWorkspaceMember {
                    name: meta_package.id.name.clone(),
                    path: normalize_workspace_relative_path(
                        &workspace_root,
                        workspace_package.root().as_path(),
                    ),
                    id: meta_package.id.to_flat(),
                };
                members.push(member);
            }

            // Record sdist/wheel information
            if let Some(sdist) = &lock_package.sdist {
                meta_package.sdist = Some(MetadataSourceDist::from_sdist(&workspace_root, sdist));
            }

            for wheel in &lock_package.wheels {
                meta_package
                    .wheels
                    .push(MetadataWheel::from_wheel(&workspace_root, wheel));
            }

            resolve.insert(meta_package.id.to_flat(), meta_package);
        }

        let conflicts = MetadataConflicts::from_conflicts(&members, &resolve, &lock.conflicts);

        Ok(Self {
            schema: SchemaReport {
                version: SchemaVersion::Preview,
            },
            conflicts,
            module_owners: BTreeMap::new(),
            workspace_root,
            requires_python: lock.requires_python.clone(),
            members,
            resolution: resolve,
        })
    }

    #[must_use]
    pub fn with_module_owners(
        mut self,
        module_owners: BTreeMap<ModuleName, Vec<PackageName>>,
    ) -> Self {
        self.module_owners = module_owners;
        self
    }

    pub fn to_json(&self) -> Result<String, MetadataError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
