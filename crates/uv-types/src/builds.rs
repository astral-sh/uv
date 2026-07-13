use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use papaya::{HashMap, ResizeMode};

use uv_cache_info::{CacheInfo, CacheInfoError};
use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::{
    BuildInfo, BuiltDist, ConfigSettings, Dist, ExtraBuildRequires, ExtraBuildVariables, Name,
    PackageConfigSettings, Requirement, Resolution, ResolvedDist, SourceDist,
};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::HashDigest;
use uv_python::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Default, Copy, Clone)]
pub enum BuildIsolation<'a> {
    #[default]
    Isolated,
    Shared(&'a PythonEnvironment),
    SharedPackage(&'a PythonEnvironment, &'a [PackageName]),
}

impl BuildIsolation<'_> {
    /// Returns `true` if build isolation is enforced for the given package name.
    pub fn is_isolated(&self, package: Option<&PackageName>) -> bool {
        match self {
            Self::Isolated => true,
            Self::Shared(_) => false,
            Self::SharedPackage(_, packages) => {
                package.is_none_or(|package| !packages.iter().any(|p| p == package))
            }
        }
    }

    /// Returns the shared environment for a given package, if build isolation is not enforced.
    pub fn shared_environment(&self, package: Option<&PackageName>) -> Option<&PythonEnvironment> {
        match self {
            Self::Isolated => None,
            Self::Shared(env) => Some(env),
            Self::SharedPackage(env, packages) => {
                if package.is_some_and(|package| packages.iter().any(|p| p == package)) {
                    Some(env)
                } else {
                    None
                }
            }
        }
    }
}

/// A key for the build cache, which includes the interpreter, source root, subdirectory, source
/// strategy, and build kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildKey {
    pub base_python: Box<Path>,
    pub source_root: Box<Path>,
    pub subdirectory: Option<Box<Path>>,
    pub no_sources: NoSources,
    pub build_kind: BuildKind,
}

/// An arena of in-process builds.
#[derive(Debug)]
pub struct BuildArena<T>(Arc<HashMap<BuildKey, Arc<T>>>);

impl<T> Default for BuildArena<T> {
    fn default() -> Self {
        Self(Arc::new(
            HashMap::builder().resize_mode(ResizeMode::Blocking).build(),
        ))
    }
}

impl<T> Clone for BuildArena<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> BuildArena<T> {
    /// Insert a build entry into the arena.
    pub fn insert(&self, key: BuildKey, value: impl Into<Arc<T>>) {
        self.0.pin().insert(key, value.into());
    }

    /// Remove a build entry from the arena.
    pub fn remove(&self, key: &BuildKey) -> Option<Arc<T>> {
        self.0.pin().remove(key).cloned()
    }
}

/// A resolved build dependency with its marker (an edge from the resolution root).
#[derive(Debug, Clone)]
pub struct ResolvedBuildDependency {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// The marker indicating when this dependency is needed.
    pub marker: MarkerTree,
    /// The extras requested on this direct build requirement.
    pub extras: BTreeSet<ExtraName>,
}

/// A dependency edge in the build resolution graph.
#[derive(Debug, Clone)]
pub struct BuildDependencyEdge {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The marker for when this dependency is needed.
    pub marker: MarkerTree,
    /// The extras requested on this dependency edge.
    pub extras: BTreeSet<ExtraName>,
}

/// A package in the build resolution graph, with its direct dependencies.
#[derive(Debug, Clone)]
pub struct BuildDependencyPackage {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// The marker environments in which this package is reachable.
    pub marker: MarkerTree,
    /// This package's direct dependencies with markers.
    pub dependencies: Vec<BuildDependencyEdge>,
    /// This package's extra dependencies with markers.
    pub optional_dependencies: BTreeMap<ExtraName, Vec<BuildDependencyEdge>>,
}

/// The build resolution graph for a single package: direct build requirements
/// and all packages with their dependency edges.
#[derive(Debug, Clone, Default)]
pub struct BuildResolutionGraph {
    /// Direct build requirements (edges from the resolution root to the
    /// packages that `build-system.requires` lists).
    pub direct_dependencies: Vec<ResolvedBuildDependency>,
    /// All packages in the resolution with their direct dependencies.
    pub packages: Vec<BuildDependencyPackage>,
}

/// A source discriminator for a package key used by build dependency locking.
///
/// This intentionally stores canonicalized strings so the key can be serialized,
/// compared, and reconstructed across crates without carrying resolver-specific
/// source types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildPackageSource {
    Registry(String),
    DirectUrl(String),
    Git(String),
    Path(String),
    Directory(String),
    Editable(String),
    Virtual(String),
}

/// Return whether two build package keys refer to the same source package.
///
/// Dynamic source packages can omit their version, and a workspace source can be represented as
/// either editable or non-editable depending on the build operation.
pub fn build_keys_match(left: &BuildPackageKey, right: &BuildPackageKey) -> bool {
    left.name == right.name
        && (left.version == right.version || left.version.is_none() || right.version.is_none())
        && match (left.source.as_ref(), right.source.as_ref()) {
            (left, right) if left == right => true,
            (
                Some(BuildPackageSource::Directory(left)),
                Some(BuildPackageSource::Editable(right)),
            )
            | (
                Some(BuildPackageSource::Editable(left)),
                Some(BuildPackageSource::Directory(right)),
            ) => left == right,
            _ => false,
        }
}

impl BuildPackageSource {
    /// Construct a source discriminator from a [`SourceDist`].
    fn from_source_dist(source: &SourceDist) -> Self {
        match source {
            SourceDist::Registry(dist) => {
                Self::Registry(dist.index.without_credentials().as_ref().to_string())
            }
            SourceDist::DirectUrl(dist) => Self::DirectUrl(dist.url.to_string()),
            SourceDist::GitDirectory(dist) => Self::Git(dist.url.to_string()),
            SourceDist::GitPath(dist) => Self::Git(dist.url.to_string()),
            SourceDist::Path(dist) => Self::Path(dist.url.to_string()),
            SourceDist::Directory(dist) => {
                if dist.editable.unwrap_or(false) {
                    Self::Editable(dist.url.to_string())
                } else if dist.r#virtual.unwrap_or(false) {
                    Self::Virtual(dist.url.to_string())
                } else {
                    Self::Directory(dist.url.to_string())
                }
            }
        }
    }
}

/// A key identifying a package by name, optional version, and source.
///
/// This mirrors `PackageId` semantics for build-locking, while allowing
/// `version = None` for dynamic local sources whose version is unknown until
/// metadata is built.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildPackageKey {
    /// The package name.
    pub name: PackageName,
    /// The package version, if known.
    ///
    /// This is `None` for source trees with dynamic metadata (for example,
    /// `dynamic = ["version"]`) before metadata is available.
    pub version: Option<Version>,
    /// The package source discriminator.
    pub source: Option<BuildPackageSource>,
}

impl BuildPackageKey {
    /// Create a new key from package name and version.
    pub fn new(name: PackageName, version: Option<Version>) -> Self {
        Self {
            name,
            version,
            source: None,
        }
    }

    /// Create a new key from package fields.
    pub fn with_source(
        name: PackageName,
        version: Option<Version>,
        source: Option<BuildPackageSource>,
    ) -> Self {
        Self {
            name,
            version,
            source,
        }
    }

    /// Create a new key from package fields and an optional source distribution.
    pub fn from_source_dist(
        name: PackageName,
        version: Option<Version>,
        source: Option<&SourceDist>,
    ) -> Self {
        Self {
            name,
            version,
            source: source.map(BuildPackageSource::from_source_dist),
        }
    }
}

/// The PEP 517 requirement-discovery stage captured by a build resolution graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildResolutionStage {
    /// The environment used before calling `get_requires_for_build_*`.
    Bootstrap,
    /// The environment used after adding requirements returned by `get_requires_for_build_*`.
    Build,
}

impl BuildResolutionStage {
    /// Return the serialized stage name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Build => "build",
        }
    }
}

/// The build operation captured by a build resolution graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildResolutionOperation {
    /// A PEP 517 wheel build.
    Wheel,
    /// A PEP 660 editable wheel build.
    Editable,
}

impl BuildResolutionOperation {
    /// Return the serialized operation name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wheel => "wheel",
            Self::Editable => "editable",
        }
    }
}

/// A captured build resolution graph key.
///
/// A source package can have multiple independently resolved build graphs when
/// it is built for different target, executor, or requirement-discovery stage
/// contexts. The optional context distinguishes those captures while preserving
/// the legacy package-only key for callers that have not been moved to
/// first-class resolution contexts yet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildResolutionGraphKey {
    /// The source package being built.
    pub package: BuildPackageKey,
    /// The operation used to build the source package.
    pub operation: BuildResolutionOperation,
    /// The captured resolution context identity, when known.
    pub context: Option<String>,
    /// The requirement-discovery stage captured by the graph.
    pub stage: Option<BuildResolutionStage>,
    /// The target marker reachability used to derive the context, when known.
    pub target_marker: Option<MarkerTree>,
}

impl BuildResolutionGraphKey {
    /// Create a package-only build resolution graph key.
    fn package(package: BuildPackageKey) -> Self {
        Self {
            package,
            operation: BuildResolutionOperation::Wheel,
            context: None,
            stage: None,
            target_marker: None,
        }
    }

    /// Create an operation- and context-qualified build resolution graph key with target
    /// reachability.
    pub fn context_with_marker_and_operation(
        package: BuildPackageKey,
        operation: BuildResolutionOperation,
        context: String,
        stage: BuildResolutionStage,
        target_marker: Option<MarkerTree>,
    ) -> Self {
        Self {
            package,
            operation,
            context: Some(context),
            stage: Some(stage),
            target_marker,
        }
    }
}

/// Map of build resolution graph keys to their captured graphs.
pub type BuildResolutionGraphMap = BTreeMap<BuildResolutionGraphKey, BuildResolutionGraph>;

fn get_unambiguous_key<'a, T>(
    map: &'a BTreeMap<BuildPackageKey, T>,
    package: &BuildPackageKey,
) -> Option<&'a T> {
    if let Some(value) = map.get(package) {
        return Some(value);
    }

    let mut matches = map
        .iter()
        .filter(|(key, _)| build_keys_match(key, package))
        .map(|(_, value)| value);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Locked build dependency resolutions, indexed by package key.
#[derive(Debug, Default, Clone)]
pub struct LockedBuildResolutions(BTreeMap<BuildPackageKey, LockedBuildResolution>);

/// A locked build dependency resolution and its direct requirements.
#[derive(Debug, Clone)]
pub struct LockedBuildResolution {
    resolution: Resolution,
    direct_dependencies: Vec<LockedBuildDependency>,
    bootstrap_direct_dependencies: Option<Vec<LockedBuildDependency>>,
    initial_requirements: Option<Vec<Requirement>>,
}

impl LockedBuildResolution {
    /// Create a locked build dependency resolution.
    pub fn new(
        resolution: Resolution,
        direct_dependencies: Vec<LockedBuildDependency>,
        initial_requirements: Option<Vec<Requirement>>,
    ) -> Self {
        Self {
            resolution,
            direct_dependencies,
            bootstrap_direct_dependencies: None,
            initial_requirements,
        }
    }

    /// Attach the direct dependencies for the initial backend-hook environment.
    #[must_use]
    pub fn with_bootstrap_direct_dependencies(
        mut self,
        bootstrap_direct_dependencies: Vec<LockedBuildDependency>,
    ) -> Self {
        self.bootstrap_direct_dependencies = Some(bootstrap_direct_dependencies);
        self
    }

    /// Return the installable resolution.
    pub fn resolution(&self) -> &Resolution {
        &self.resolution
    }

    /// Return the direct build dependencies.
    pub fn direct_dependencies(&self) -> &[LockedBuildDependency] {
        &self.direct_dependencies
    }

    /// Return the direct build dependencies used before backend hooks are called.
    pub fn bootstrap_direct_dependencies(&self) -> Option<&[LockedBuildDependency]> {
        self.bootstrap_direct_dependencies.as_deref()
    }

    /// Return the requirements used for the initial backend environment, if recorded.
    pub fn initial_requirements(&self) -> Option<&[Requirement]> {
        self.initial_requirements.as_deref()
    }
}

/// A direct dependency in a locked build resolution.
#[derive(Debug, Clone)]
pub struct LockedBuildDependency {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The extras requested on this direct build requirement.
    pub extras: BTreeSet<ExtraName>,
    /// The selected distribution and its transitive installable dependencies.
    resolution: Resolution,
}

impl LockedBuildDependency {
    /// Create a locked direct build dependency and its installable closure.
    pub fn new(dist: ResolvedDist, extras: BTreeSet<ExtraName>, resolution: Resolution) -> Self {
        Self {
            dist,
            extras,
            resolution,
        }
    }

    /// Return the installable closure for this direct dependency.
    pub fn resolution(&self) -> &Resolution {
        &self.resolution
    }
}

impl LockedBuildResolutions {
    /// Create locked build resolutions from a map keyed by [`BuildPackageKey`].
    pub fn new(map: BTreeMap<BuildPackageKey, LockedBuildResolution>) -> Self {
        Self(map)
    }

    /// Return whether there are no locked build resolutions.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the pre-built resolution for a package key.
    pub fn get(&self, package: &BuildPackageKey) -> Option<&LockedBuildResolution> {
        get_unambiguous_key(&self.0, package)
    }

    /// Return a stable digest for the complete locked build environment and any nested source
    /// builds it contains.
    pub fn cache_key(
        &self,
        package: &BuildPackageKey,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
    ) -> Result<Option<String>, CacheInfoError> {
        self.cache_key_with_stack(
            package,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
            &mut BTreeSet::new(),
            &mut BTreeMap::new(),
        )
    }

    fn cache_key_with_stack(
        &self,
        package: &BuildPackageKey,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
        stack: &mut BTreeSet<BuildPackageKey>,
        memo: &mut BTreeMap<BuildPackageKey, String>,
    ) -> Result<Option<String>, CacheInfoError> {
        let Some(resolution) = self.get(package) else {
            return Ok(None);
        };
        if stack.contains(package) {
            return Ok(None);
        }
        if let Some(cache_key) = memo.get(package) {
            return Ok(Some(cache_key.clone()));
        }
        stack.insert(package.clone());

        let mut distributions = Vec::new();
        let mut bootstrap_distributions = Vec::new();
        for (bootstrap, distribution, hashes) in resolution
            .resolution
            .hashes()
            .map(|(distribution, hashes)| (false, distribution, hashes))
            .chain(
                resolution
                    .bootstrap_direct_dependencies
                    .iter()
                    .flatten()
                    .flat_map(|dependency| dependency.resolution().hashes())
                    .map(|(distribution, hashes)| (true, distribution, hashes)),
            )
        {
            let mut hashes = hashes.to_vec();
            hashes.sort();

            let (build_info, nested) = if let ResolvedDist::Installable { dist, version } =
                distribution
                && let Dist::Source(source) = dist.as_ref()
            {
                let name = distribution.name();
                let settings = config_settings_package.get(name).map_or_else(
                    || config_settings.clone(),
                    |settings| settings.clone().merge(config_settings.clone()),
                );
                let build_info = BuildInfo::from_settings(
                    settings,
                    extra_build_requires.get(name).cloned().unwrap_or_default(),
                    extra_build_variables.get(name).cloned(),
                )
                .cache_shard();
                let nested_package = BuildPackageKey::from_source_dist(
                    name.clone(),
                    source.version().cloned().or_else(|| version.clone()),
                    Some(source),
                );
                let nested = self.cache_key_with_stack(
                    &nested_package,
                    config_settings,
                    config_settings_package,
                    extra_build_requires,
                    extra_build_variables,
                    stack,
                    memo,
                )?;
                (build_info, nested)
            } else {
                (None, None)
            };

            let (artifact_url, cache_info) = match distribution {
                ResolvedDist::Installable { dist, .. } => {
                    locked_build_artifact_info(dist, &hashes)?
                }
                ResolvedDist::Installed { .. } => (None, None),
            };

            let (kind, filename) = match distribution {
                ResolvedDist::Installable { dist, .. } => (
                    match dist.as_ref() {
                        Dist::Built(_) => "wheel",
                        Dist::Source(_) => "sdist",
                    },
                    dist.file().map(|file| file.filename.to_string()),
                ),
                ResolvedDist::Installed { .. } => ("installed", None),
            };

            let entry = (
                distribution.to_string(),
                distribution
                    .index()
                    .map(|index| index.without_credentials().as_ref().to_string()),
                hashes,
                kind,
                filename,
                artifact_url,
                build_info,
                cache_info,
                nested,
            );
            if bootstrap {
                bootstrap_distributions.push(entry);
            } else {
                distributions.push(entry);
            }
        }
        stack.remove(package);
        distributions.sort();
        bootstrap_distributions.sort();
        let cache_key = if resolution.bootstrap_direct_dependencies.is_some() {
            uv_cache_key::hash_digest(&("bootstrap", distributions, bootstrap_distributions))
        } else {
            uv_cache_key::hash_digest(&distributions)
        };
        memo.insert(package.clone(), cache_key.clone());
        Ok(Some(cache_key))
    }
}

fn locked_build_artifact_info(
    dist: &Dist,
    hashes: &[HashDigest],
) -> Result<(Option<String>, Option<String>), CacheInfoError> {
    let artifact_url = dist
        .file()
        .map(|file| file.url.to_url())
        .transpose()
        .map_err(|err| {
            CacheInfoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;

    let cache_info = match dist {
        Dist::Source(SourceDist::Path(source)) => Some(CacheInfo::from_file(&source.install_path)?),
        Dist::Source(SourceDist::Directory(source)) => {
            Some(CacheInfo::from_directory(&source.install_path)?)
        }
        Dist::Built(BuiltDist::Path(wheel)) if hashes.is_empty() => {
            Some(CacheInfo::from_file(&wheel.install_path)?)
        }
        Dist::Built(BuiltDist::Registry(_)) | Dist::Source(SourceDist::Registry(_))
            if hashes.is_empty() =>
        {
            if let Some(url) = artifact_url.as_ref()
                && url.scheme() == "file"
            {
                let path = url.to_file_path().map_err(|()| {
                    CacheInfoError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Expected a file URL, but received: {url}"),
                    ))
                })?;
                Some(CacheInfo::from_file(path)?)
            } else {
                None
            }
        }
        _ => None,
    }
    .as_ref()
    .map(uv_cache_key::hash_digest);

    let artifact_url = artifact_url.map(|mut url| {
        url.remove_credentials();
        url.to_string()
    });

    Ok((artifact_url, cache_info))
}

/// A list of `(name, version)` pairs representing preferred build dependency versions.
type BuildDependencyVersions = Vec<(PackageName, Version)>;

/// Build dependency version preferences, indexed by package key.
#[derive(Debug, Default, Clone)]
pub struct BuildPreferences(BTreeMap<BuildPackageKey, BuildDependencyVersions>);

impl BuildPreferences {
    /// Create build preferences from a map keyed by [`BuildPackageKey`].
    pub fn new(map: BTreeMap<BuildPackageKey, BuildDependencyVersions>) -> Self {
        Self(map)
    }

    /// Get the build dependency preferences for a package key.
    pub fn get(&self, package: &BuildPackageKey) -> Option<&[(PackageName, Version)]> {
        get_unambiguous_key(&self.0, package).map(Vec::as_slice)
    }
}

/// Captured build dependency resolutions with markers.
#[derive(Debug, Default, Clone)]
pub struct BuildResolutions(Arc<Mutex<BuildResolutionGraphMap>>);

impl BuildResolutions {
    /// Record a build resolution for the given package key.
    pub fn insert(&self, package: BuildPackageKey, graph: BuildResolutionGraph) {
        self.insert_key(BuildResolutionGraphKey::package(package), graph);
    }

    /// Record a build resolution for the given context-qualified key.
    pub fn insert_key(&self, key: BuildResolutionGraphKey, graph: BuildResolutionGraph) {
        let mut graphs = self.0.lock().unwrap();
        graphs.insert(key, graph);
    }

    /// Get the exact graph for a build resolution graph key.
    pub fn get(&self, key: &BuildResolutionGraphKey) -> Option<BuildResolutionGraph> {
        let graphs = self.0.lock().unwrap();
        graphs.get(key).cloned()
    }

    /// Get all captured build resolutions, including context-qualified graphs.
    pub fn snapshot_contexts(&self) -> BuildResolutionGraphMap {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_distribution_filename::{SourceDistExtension, WheelFilename};
    use uv_distribution_types::{
        BuiltDist, ConfigSettingPackageEntry, File, FileLocation, IndexUrl, Node,
        RegistryBuiltDist, RegistryBuiltWheel, RegistrySourceDist,
    };
    use uv_pypi_types::HashDigests;

    use super::*;

    fn package_key() -> BuildPackageKey {
        BuildPackageKey::new(
            PackageName::from_str("dep").expect("valid package name"),
            Some(Version::from_str("1.0.0").expect("valid version")),
        )
    }

    fn context_key(
        package: BuildPackageKey,
        context: String,
        stage: BuildResolutionStage,
        target_marker: Option<MarkerTree>,
    ) -> BuildResolutionGraphKey {
        BuildResolutionGraphKey::context_with_marker_and_operation(
            package,
            BuildResolutionOperation::Wheel,
            context,
            stage,
            target_marker,
        )
    }

    #[test]
    fn build_resolutions_retain_contextual_graphs_for_same_package() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();

        build_resolutions.insert_key(
            context_key(
                package.clone(),
                "build:dep:wheel:one".to_string(),
                BuildResolutionStage::Build,
                None,
            ),
            BuildResolutionGraph::default(),
        );
        build_resolutions.insert_key(
            context_key(
                package.clone(),
                "build:dep:wheel:two".to_string(),
                BuildResolutionStage::Build,
                None,
            ),
            BuildResolutionGraph::default(),
        );

        let graphs = build_resolutions.snapshot_contexts();
        assert_eq!(graphs.len(), 2);
        let one = context_key(
            package.clone(),
            "build:dep:wheel:one".to_string(),
            BuildResolutionStage::Build,
            None,
        );
        let two = context_key(
            package.clone(),
            "build:dep:wheel:two".to_string(),
            BuildResolutionStage::Build,
            None,
        );
        assert!(graphs.contains_key(&one));
        assert!(graphs.contains_key(&two));
        assert!(build_resolutions.get(&one).is_some());
        assert!(build_resolutions.get(&two).is_some());
    }

    #[test]
    fn build_resolutions_retain_stage_qualified_context_captures() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();
        let context = "build:dep:wheel:one".to_string();
        let first = context_key(
            package.clone(),
            context.clone(),
            BuildResolutionStage::Build,
            None,
        );
        let second = context_key(
            package,
            context,
            BuildResolutionStage::Bootstrap,
            Some(MarkerTree::TRUE),
        );

        build_resolutions.insert_key(first.clone(), BuildResolutionGraph::default());
        build_resolutions.insert_key(second.clone(), BuildResolutionGraph::default());

        let graphs = build_resolutions.snapshot_contexts();
        assert_eq!(graphs.len(), 2);
        assert!(graphs.contains_key(&first));
        assert!(graphs.contains_key(&second));
    }

    #[test]
    fn build_resolutions_keep_package_keyed_capture() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();

        build_resolutions.insert(package.clone(), BuildResolutionGraph::default());

        let graphs = build_resolutions.snapshot_contexts();
        assert_eq!(graphs.len(), 1);
        assert!(graphs.contains_key(&BuildResolutionGraphKey::package(package)));
    }

    #[test]
    fn build_resolution_lookup_respects_source_identity() {
        let name = PackageName::from_str("dep").expect("valid package name");
        let version = Some(Version::from_str("1.0.0").expect("valid version"));
        let first = BuildPackageKey::with_source(
            name.clone(),
            version.clone(),
            Some(BuildPackageSource::Registry(
                "https://one.example/simple".to_string(),
            )),
        );
        let second = BuildPackageKey::with_source(
            name.clone(),
            version,
            Some(BuildPackageSource::Registry(
                "https://two.example/simple".to_string(),
            )),
        );
        let versionless = BuildPackageKey::with_source(
            name,
            None,
            Some(BuildPackageSource::Registry(
                "https://one.example/simple".to_string(),
            )),
        );

        let only_first = BTreeMap::from([(first.clone(), 1)]);
        assert_eq!(get_unambiguous_key(&only_first, &versionless), Some(&1));
        assert!(get_unambiguous_key(&only_first, &second).is_none());

        let both = BTreeMap::from([(first, 1), (second.clone(), 2)]);
        assert_eq!(get_unambiguous_key(&both, &versionless), Some(&1));
        assert_eq!(get_unambiguous_key(&both, &second), Some(&2));

        let directory = BuildPackageKey::with_source(
            PackageName::from_str("workspace").expect("valid package name"),
            Some(Version::from_str("1.0.0").expect("valid version")),
            Some(BuildPackageSource::Directory(
                "file:///workspace".to_string(),
            )),
        );
        let editable = BuildPackageKey::with_source(
            PackageName::from_str("workspace").expect("valid package name"),
            None,
            Some(BuildPackageSource::Editable(
                "file:///workspace".to_string(),
            )),
        );
        assert!(build_keys_match(&directory, &editable));
    }

    #[test]
    fn locked_build_cache_key_handles_shared_nested_resolutions() {
        fn source_dist(name: &str) -> (BuildPackageKey, ResolvedDist) {
            let name = PackageName::from_str(name).expect("valid package name");
            let version = Version::from_str("1.0.0").expect("valid version");
            let filename = format!("{name}-{version}.tar.gz");
            let source = SourceDist::Registry(RegistrySourceDist {
                name: name.clone(),
                version: version.clone(),
                file: Box::new(File {
                    dist_info_metadata: false,
                    filename: filename.clone().into(),
                    hashes: HashDigests::empty(),
                    requires_python: None,
                    size: None,
                    upload_time_utc_ms: None,
                    url: FileLocation::new(
                        format!("https://files.example/{filename}").into(),
                        &"https://files.example/".into(),
                    ),
                    yanked: None,
                    zstd: None,
                }),
                ext: SourceDistExtension::TarGz,
                index: IndexUrl::from_str("https://example.com/simple").expect("valid index"),
                wheels: vec![],
            });
            let key = BuildPackageKey::from_source_dist(name, Some(version.clone()), Some(&source));
            let dist = ResolvedDist::Installable {
                dist: Arc::new(Dist::Source(source)),
                version: Some(version),
            };
            (key, dist)
        }

        fn locked_resolution(dependencies: &[String]) -> LockedBuildResolution {
            let mut graph = petgraph::graph::DiGraph::new();
            for dependency in dependencies {
                let (_, dist) = source_dist(dependency);
                graph.add_node(Node::Dist {
                    dist,
                    hashes: HashDigests::empty(),
                    install: true,
                });
            }
            LockedBuildResolution::new(Resolution::new(graph), Vec::new(), None)
        }

        const DEPTH: usize = 24;
        let names = |layer| {
            [
                format!("layer-{layer}-left"),
                format!("layer-{layer}-right"),
            ]
        };
        let package = package_key();
        let mut resolutions = BTreeMap::from([(package.clone(), locked_resolution(&names(0)))]);

        for layer in 0..DEPTH {
            let dependencies = if layer + 1 == DEPTH {
                Vec::new()
            } else {
                names(layer + 1).into()
            };
            for name in names(layer) {
                let (key, _) = source_dist(&name);
                resolutions.insert(key, locked_resolution(&dependencies));
            }
        }

        // Each layer shares its two nested resolutions. Without per-call memoization, every
        // layer doubles the traversal and this graph requires more than 16 million visits.
        assert!(
            LockedBuildResolutions::new(resolutions)
                .cache_key(
                    &package,
                    &ConfigSettings::default(),
                    &PackageConfigSettings::default(),
                    &ExtraBuildRequires::default(),
                    &ExtraBuildVariables::default(),
                )
                .expect("readable cache info")
                .is_some()
        );
    }

    #[test]
    fn locked_build_cache_key_includes_bootstrap_resolution() {
        fn bootstrap_dependency(artifact_url: &str) -> LockedBuildDependency {
            let version = Version::from_str("1.0.0").expect("valid version");
            let filename = "helper-1.0.0-py3-none-any.whl";
            let dist = ResolvedDist::Installable {
                dist: Arc::new(Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![RegistryBuiltWheel {
                        filename: WheelFilename::from_str(filename).expect("valid wheel filename"),
                        file: Box::new(File {
                            dist_info_metadata: false,
                            filename: filename.into(),
                            hashes: HashDigests::empty(),
                            requires_python: None,
                            size: None,
                            upload_time_utc_ms: None,
                            url: FileLocation::new(
                                format!("{artifact_url}/{filename}").into(),
                                &format!("{artifact_url}/").into(),
                            ),
                            yanked: None,
                            zstd: None,
                        }),
                        index: IndexUrl::from_str("https://example.com/simple")
                            .expect("valid index"),
                    }],
                    best_wheel_index: 0,
                    sdist: None,
                }))),
                version: Some(version),
            };
            let mut graph = petgraph::graph::DiGraph::new();
            graph.add_node(Node::Dist {
                dist: dist.clone(),
                hashes: HashDigests::empty(),
                install: true,
            });
            LockedBuildDependency::new(dist, BTreeSet::new(), Resolution::new(graph))
        }

        let package = package_key();
        let cache_key = |bootstrap| {
            LockedBuildResolutions::new(BTreeMap::from([(
                package.clone(),
                LockedBuildResolution::new(Resolution::default(), Vec::new(), None)
                    .with_bootstrap_direct_dependencies(bootstrap),
            )]))
            .cache_key(
                &package,
                &ConfigSettings::default(),
                &PackageConfigSettings::default(),
                &ExtraBuildRequires::default(),
                &ExtraBuildVariables::default(),
            )
            .expect("readable cache info")
        };

        assert_ne!(
            cache_key(vec![bootstrap_dependency("https://one-files.example")]),
            cache_key(vec![bootstrap_dependency("https://two-files.example")])
        );
        assert_ne!(
            cache_key(Vec::new()),
            LockedBuildResolutions::new(BTreeMap::from([(
                package.clone(),
                LockedBuildResolution::new(Resolution::default(), Vec::new(), None),
            )]))
            .cache_key(
                &package,
                &ConfigSettings::default(),
                &PackageConfigSettings::default(),
                &ExtraBuildRequires::default(),
                &ExtraBuildVariables::default(),
            )
            .expect("readable cache info")
        );

        let staged_cache_key = |build: LockedBuildDependency, bootstrap: LockedBuildDependency| {
            LockedBuildResolutions::new(BTreeMap::from([(
                package.clone(),
                LockedBuildResolution::new(build.resolution().clone(), Vec::new(), None)
                    .with_bootstrap_direct_dependencies(vec![bootstrap]),
            )]))
            .cache_key(
                &package,
                &ConfigSettings::default(),
                &PackageConfigSettings::default(),
                &ExtraBuildRequires::default(),
                &ExtraBuildVariables::default(),
            )
            .expect("readable cache info")
        };

        assert_ne!(
            staged_cache_key(
                bootstrap_dependency("https://one-files.example"),
                bootstrap_dependency("https://two-files.example")
            ),
            staged_cache_key(
                bootstrap_dependency("https://two-files.example"),
                bootstrap_dependency("https://one-files.example")
            )
        );
    }

    #[test]
    fn locked_build_cache_key_includes_registry_artifact_without_credentials() {
        fn locked_resolution(
            index: &str,
            artifact_url: &str,
            filename: &str,
        ) -> LockedBuildResolution {
            let name = PackageName::from_str("helper").expect("valid package name");
            let version = Version::from_str("1.0.0").expect("valid version");
            let index = IndexUrl::from_str(index).expect("valid index");
            let file = Box::new(File {
                dist_info_metadata: false,
                filename: filename.into(),
                hashes: HashDigests::empty(),
                requires_python: None,
                size: None,
                upload_time_utc_ms: None,
                url: FileLocation::new(
                    format!("{artifact_url}/{filename}").into(),
                    &format!("{artifact_url}/").into(),
                ),
                yanked: None,
                zstd: None,
            });
            let dist = if Path::new(filename)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("whl"))
            {
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![RegistryBuiltWheel {
                        filename: WheelFilename::from_str(filename).expect("valid wheel filename"),
                        file,
                        index,
                    }],
                    best_wheel_index: 0,
                    sdist: None,
                }))
            } else {
                Dist::Source(SourceDist::Registry(RegistrySourceDist {
                    name,
                    version: version.clone(),
                    file,
                    ext: SourceDistExtension::TarGz,
                    index,
                    wheels: vec![],
                }))
            };
            let mut graph = petgraph::graph::DiGraph::new();
            graph.add_node(Node::Dist {
                dist: ResolvedDist::Installable {
                    dist: Arc::new(dist),
                    version: Some(version),
                },
                hashes: HashDigests::empty(),
                install: true,
            });
            LockedBuildResolution::new(Resolution::new(graph), Vec::new(), None)
        }

        let package = package_key();
        let cache_key = |index, artifact_url, filename, package_settings| {
            LockedBuildResolutions::new(BTreeMap::from([(
                package.clone(),
                locked_resolution(index, artifact_url, filename),
            )]))
            .cache_key(
                &package,
                &ConfigSettings::default(),
                package_settings,
                &ExtraBuildRequires::default(),
                &ExtraBuildVariables::default(),
            )
            .expect("readable cache info")
        };

        let default_settings = PackageConfigSettings::default();
        let custom_settings =
            [ConfigSettingPackageEntry::from_str("helper:mode=custom").expect("valid setting")]
                .into_iter()
                .collect();

        assert_eq!(
            cache_key(
                "https://user:password@one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://two.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &custom_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0-py3-none-any.whl",
                &default_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0-py3-none-any.whl",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0-py3-none-macosx_11_0_arm64.whl",
                &default_settings
            )
        );
        assert_eq!(
            cache_key(
                "https://one.example/simple",
                "https://user:password@files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://one-files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://two-files.example",
                "helper-1.0.0.tar.gz",
                &default_settings
            )
        );
        assert_ne!(
            cache_key(
                "https://one.example/simple",
                "https://one-files.example",
                "helper-1.0.0-py3-none-any.whl",
                &default_settings
            ),
            cache_key(
                "https://one.example/simple",
                "https://two-files.example",
                "helper-1.0.0-py3-none-any.whl",
                &default_settings
            )
        );
    }
}
