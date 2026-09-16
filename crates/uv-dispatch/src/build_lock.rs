//! Capture and replay independently locked PEP 517 build environments.

use std::collections::{BTreeMap, BTreeSet};
use std::future::{self, Future};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail, ensure};
use tokio::sync::Mutex;
use uv_build_frontend::{SourceBuild, SourceBuildContext};
use uv_cache::Cache;
use uv_configuration::{
    BuildKind, BuildOptions, BuildOutput, Constraints, HashCheckingMode, NoBuild, NoSources,
    Upgrade,
};
use uv_distribution::DistributionDatabase;
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{
    ArchiveHashPolicy, BuildLockFingerprint, BuiltDist, CachedDist, ConfigSettings,
    DependencyMetadata, Dist, ExtraBuildRequires, ExtraBuildVariables, HashCollection, Hashed,
    IndexCapabilities, IndexLocations, IsBuildBackendError, PackageConfigSettings, Requirement,
    Resolution, ResolvedDist, SourceDist,
};
use uv_git::GitResolver;
use uv_installer::InstallationStrategy;
use uv_lock::{
    BuildExecutor, BuildOperation, BuildSourceId, BuildSourceInput, BuildStage, Lock, LockedBuild,
    LockedBuilds, ResolverManifest, ensure_build_wheels,
};
use uv_pypi_types::HashDigests;
use uv_python::{Interpreter, PythonEnvironment};
use uv_resolver::{Preference, UpgradePackages};
use uv_resolver_types::{ResolutionGraphNode, ResolverOutput};
use uv_types::{
    AnyErrorBuild, BuildArena, BuildContext, BuildIsolation, BuildRequirementKind, BuildStack,
    HashStrategy, ResolvedRequirements, SourceTreeEditablePolicy,
};
use uv_workspace::WorkspaceCache;

use crate::{BuildDispatch, BuildDispatchError};

type CapturedBuilds = BTreeMap<(BuildSourceId, BuildOperation), LockedBuild>;

#[derive(Clone)]
pub(super) enum BuildLocking {
    Capture {
        root: Arc<Path>,
        captured: Arc<Mutex<CapturedBuilds>>,
        previous: Option<Arc<LockedBuild>>,
        upgrades: Upgrade,
    },
    Replay {
        root: Arc<Path>,
        builds: Arc<LockedBuilds>,
        fingerprint: BuildLockFingerprint,
    },
}

impl BuildLocking {
    fn root(&self) -> &Path {
        match self {
            Self::Capture { root, .. } | Self::Replay { root, .. } => root,
        }
    }
}

impl BuildDispatch<'_> {
    /// Capture build environments for sources selected from the runtime lock on this executor.
    pub async fn capture_build_lock(
        &self,
        lock: &mut Lock,
        root: &Path,
        previous: Option<&LockedBuilds>,
        upgrade: &Upgrade,
    ) -> Result<LockedBuilds> {
        self.validate_build_lock_settings()?;
        let executor = BuildExecutor::from_interpreter(self.interpreter)?;
        let mut captured = BTreeMap::new();
        for (source, hashes) in lock.build_sources(
            root,
            self.interpreter.tags()?,
            self.interpreter.markers(),
            self.build_options,
        )? {
            let policy = if hashes.is_empty() {
                ArchiveHashPolicy::Generate
            } else {
                ArchiveHashPolicy::Any(hashes.as_slice())
            };
            let mut sources = vec![source];
            // A local project may be installed in either mode without changing its lock identity.
            if let SourceDist::Directory(directory) = &sources[0] {
                let mut other = directory.clone();
                other.editable = Some(!directory.editable.unwrap_or(false));
                sources.push(SourceDist::Directory(other));
            }
            for source in sources {
                let id = BuildSourceId::from_source_dist(&source, root)?;
                let operation = if source.is_editable() {
                    BuildOperation::Editable
                } else {
                    BuildOperation::Wheel
                };
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    captured.entry((id, operation))
                {
                    let previous = previous.and_then(|previous| {
                        previous.get(&entry.key().0, operation, &executor).ok()
                    });
                    let (build, hashes) = self
                        .capture_build(&source, root, policy, previous, upgrade)
                        .await?;
                    lock.record_build_source_hash(&source, root, &hashes, self.index_locations)?;
                    entry.insert(build);
                }
            }
        }
        Ok(LockedBuilds::new(
            executor,
            captured.into_values().collect(),
        )?)
    }

    /// Enforce an existing project build lock, with fresh in-process build state.
    pub fn with_build_lock(mut self, builds: &LockedBuilds, root: &Path) -> Result<Self> {
        self.validate_build_lock_settings()?;
        self.shared_state = self.shared_state.fork();
        self.shared_state.build_arena = BuildArena::default();
        self.source_build_context =
            SourceBuildContext::new(self.concurrency.builds_semaphore.clone());
        self.build_locking = Some(BuildLocking::Replay {
            root: Arc::from(root),
            builds: Arc::new(builds.clone()),
            fingerprint: builds.fingerprint()?,
        });
        Ok(self)
    }

    /// Capture the actual isolated environment used by one selected source and operation.
    async fn capture_build(
        &self,
        source: &SourceDist,
        root: &Path,
        hashes: ArchiveHashPolicy<'_>,
        previous: Option<&LockedBuild>,
        upgrades: &Upgrade,
    ) -> Result<(LockedBuild, HashDigests)> {
        self.validate_build_lock_settings()?;
        let id = BuildSourceId::from_source_dist(source, root)?;
        let operation = if source.is_editable() {
            BuildOperation::Editable
        } else {
            BuildOperation::Wheel
        };
        let captured = Arc::new(Mutex::new(CapturedBuilds::new()));
        let hasher = HashStrategy::collect(HashCollection::All)
            .with_verification(self.hasher.verification().clone());
        let mut dispatch = self.fork(&hasher);
        dispatch.build_locking = Some(BuildLocking::Capture {
            root: Arc::from(root),
            captured: captured.clone(),
            previous: previous.cloned().map(Arc::new),
            upgrades: upgrades.clone(),
        });
        let hashes = DistributionDatabase::new(
            self.client,
            &dispatch,
            self.concurrency.downloads_semaphore.clone(),
        )
        .resolve_static_build_requirements(source, hashes)
        .await?;
        let mut captured = captured.lock().await;
        ensure!(
            captured.len() == 1,
            "Expected one captured build environment for `{source}`"
        );
        let build = captured
            .remove(&(id, operation))
            .context("The selected source build was not captured")?;
        Ok((build, hashes))
    }

    /// The initial public contract does not infer how unrecorded build settings affect hooks.
    fn validate_build_lock_settings(&self) -> Result<()> {
        ensure!(
            matches!(self.build_isolation, BuildIsolation::Isolated),
            "Build dependency locking requires build isolation"
        );
        ensure!(
            *self.config_settings == ConfigSettings::default()
                && *self.config_settings_package == PackageConfigSettings::default(),
            "Build dependency locking does not yet support config settings"
        );
        ensure!(
            self.extra_build_requires.is_empty(),
            "Build dependency locking does not yet support extra build dependencies"
        );
        ensure!(
            self.extra_build_variables.is_empty() && self.build_extra_env_vars.is_empty(),
            "Build dependency locking does not yet support extra build variables"
        );
        ensure!(
            self.sources.is_none(),
            "Build dependency locking does not yet support disabling package sources"
        );
        Ok(())
    }

    pub(super) async fn setup_locked_build(
        &self,
        locking: &BuildLocking,
        source: &Path,
        subdirectory: Option<&Path>,
        install_path: &Path,
        stop_discovery_at: Option<&Path>,
        version_id: Option<&str>,
        dist: Option<&SourceDist>,
        sources: &NoSources,
        build_kind: BuildKind,
        build_output: BuildOutput,
        build_stack: BuildStack,
    ) -> Result<SourceBuild> {
        let root = locking.root();
        let dist = dist.context("A locked build requires a known source identity")?;
        let id = BuildSourceId::from_source_dist(dist, root).map_err(anyhow::Error::from)?;
        let operation = BuildOperation::try_from(build_kind).map_err(anyhow::Error::from)?;
        let source_tree =
            subdirectory.map_or_else(|| source.to_path_buf(), |subdir| source.join(subdir));
        let input = BuildSourceInput::read(&source_tree).map_err(anyhow::Error::from)?;
        let replay = match locking {
            BuildLocking::Capture { .. } => None,
            BuildLocking::Replay { builds, .. } => {
                let executor = BuildExecutor::from_interpreter(self.interpreter)?;
                let build = builds
                    .get(&id, operation, &executor)
                    .map_err(anyhow::Error::from)?;
                ensure!(
                    build.input() == &input,
                    "The build declarations for `{id}` changed; update the build lock"
                );
                Some(build)
            }
        };
        let (previous, upgrades) = match locking {
            BuildLocking::Capture {
                previous, upgrades, ..
            } => (previous.as_deref(), upgrades.clone()),
            BuildLocking::Replay { .. } => (None, Upgrade::default()),
        };
        let scoped = ScopedBuild::new(self, root, replay, previous, upgrades);
        let builder = self
            .setup_build_with_context(
                &scoped,
                SourceBuildContext::new(self.concurrency.builds_semaphore.clone()),
                source,
                subdirectory,
                install_path,
                stop_discovery_at,
                version_id,
                Some(dist),
                sources,
                build_kind,
                build_output,
                build_stack,
            )
            .await
            .map_err(AnyErrorBuild::from)?;
        ensure!(
            BuildSourceInput::read(&source_tree).map_err(anyhow::Error::from)? == input,
            "The build declarations for `{id}` changed during backend discovery"
        );
        if let Some(build) = scoped.finish(id, operation, input)? {
            let BuildLocking::Capture { captured, .. } = locking else {
                return Err(anyhow!("Unexpected build capture during replay"));
            };
            let mut captured = captured.lock().await;
            let key = (build.source().clone(), build.operation());
            if let Some(previous) = captured.get(&key) {
                ensure!(
                    previous == &build,
                    "Build dependency observations changed while locking `{}`",
                    build.source()
                );
            } else {
                captured.insert(key, build);
            }
        }
        Ok(builder)
    }
}

/// A single frontend invocation. Nested source builds cannot borrow its identity or stages.
struct ScopedBuild<'a> {
    dispatch: BuildDispatch<'a>,
    root: &'a Path,
    replay: Option<&'a LockedBuild>,
    previous: Option<&'a LockedBuild>,
    upgrades: Upgrade,
    build_options: BuildOptions,
    state: Mutex<ScopeState>,
}

#[derive(Default)]
struct ScopeState {
    declared: Option<Vec<Requirement>>,
    backend: Option<Vec<Requirement>>,
    resolved: usize,
    graphs: Vec<Lock>,
}

impl<'a> ScopedBuild<'a> {
    fn new(
        dispatch: &'a BuildDispatch<'a>,
        root: &'a Path,
        replay: Option<&'a LockedBuild>,
        previous: Option<&'a LockedBuild>,
        upgrades: Upgrade,
    ) -> Self {
        let mut dispatch = dispatch.clone();
        dispatch.build_locking = None;
        dispatch.build_installation = InstallationStrategy::Strict;
        let build_options =
            BuildOptions::new(dispatch.build_options.no_binary().clone(), NoBuild::All);
        Self {
            dispatch,
            root,
            replay,
            previous,
            upgrades,
            build_options,
            state: Mutex::new(ScopeState::default()),
        }
    }

    fn finish(
        self,
        source: BuildSourceId,
        operation: BuildOperation,
        input: BuildSourceInput,
    ) -> Result<Option<LockedBuild>> {
        let state = self.state.into_inner();
        let declared = state
            .declared
            .context("The frontend did not report declared requirements")?;
        let backend = state
            .backend
            .context("The frontend did not report backend requirements")?;
        if let Some(replay) = self.replay {
            ensure!(
                state.resolved == 1 + usize::from(replay.has_final_resolution()),
                "The build did not consume every locked stage"
            );
            return Ok(None);
        }
        let mut graphs = state.graphs.into_iter();
        let bootstrap = graphs
            .next()
            .context("The frontend did not resolve its bootstrap environment")?;
        let final_resolution = graphs.next();
        ensure!(
            graphs.next().is_none(),
            "Unexpected additional build resolution"
        );
        Ok(Some(
            LockedBuild::new(
                source,
                operation,
                input,
                declared,
                backend,
                bootstrap,
                final_resolution,
            )
            .map_err(anyhow::Error::from)?,
        ))
    }

    /// Retain only the selected wheel, with hashes measured from that artifact's bytes.
    async fn observe_wheels(
        &self,
        output: &mut ResolverOutput,
        hasher: &HashStrategy,
    ) -> Result<()> {
        let database = DistributionDatabase::new(
            self.dispatch.client,
            self,
            self.dispatch.concurrency.downloads_semaphore.clone(),
        );
        let tags = self.dispatch.interpreter.tags()?;
        for node in output.graph.node_weights_mut() {
            let ResolutionGraphNode::Dist(distribution) = node else {
                continue;
            };
            let ResolvedDist::Installable { dist, .. } = &mut distribution.dist else {
                bail!("An installed package cannot identify a locked build artifact");
            };
            let Dist::Built(built) = dist.as_ref() else {
                bail!("Nested source builds are not supported by build dependency locking: {dist}");
            };
            let policy = match built {
                BuiltDist::Registry(registry) => hasher
                    .archive_policy(dist.as_ref())
                    .with_index_hashes(registry.best_wheel().file.hashes.as_slice()),
                BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_) => {
                    hasher.archive_policy(dist.as_ref())
                }
            };
            let wheel = database.get_or_build_wheel(dist, tags, policy).await?;
            ensure!(
                wheel.satisfies(policy),
                "The cached wheel does not satisfy its hash policy: {dist}"
            );
            let hashes = HashDigests::from(wheel.hashes());
            if let Dist::Built(BuiltDist::Registry(registry)) = Arc::make_mut(dist) {
                let mut selected = registry.best_wheel().clone();
                selected.file.hashes = hashes.clone();
                registry.wheels = vec![selected];
                registry.best_wheel_index = 0;
                registry.sdist = None;
            }
            distribution.hashes = hashes;
        }
        Ok(())
    }

    fn normalize(&self, requirements: &[Requirement]) -> Result<Vec<Requirement>> {
        Ok(requirements
            .iter()
            .cloned()
            .map(|requirement| requirement.relative_to(self.root))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect())
    }
}

impl BuildContext for ScopedBuild<'_> {
    type SourceDistBuilder = SourceBuild;

    fn interpreter(&self) -> impl Future<Output = &Interpreter> + '_ {
        self.dispatch.interpreter()
    }
    fn cache(&self) -> &Cache {
        self.dispatch.cache()
    }
    fn git(&self) -> &GitResolver {
        self.dispatch.git()
    }
    fn build_arena(&self) -> &BuildArena<SourceBuild> {
        self.dispatch.build_arena()
    }
    fn capabilities(&self) -> &IndexCapabilities {
        self.dispatch.capabilities()
    }
    fn dependency_metadata(&self) -> &DependencyMetadata {
        self.dispatch.dependency_metadata()
    }
    fn build_options(&self) -> &BuildOptions {
        &self.build_options
    }
    fn build_isolation(&self) -> BuildIsolation<'_> {
        BuildIsolation::Isolated
    }
    fn config_settings(&self) -> &ConfigSettings {
        self.dispatch.config_settings()
    }
    fn config_settings_package(&self) -> &PackageConfigSettings {
        self.dispatch.config_settings_package()
    }
    fn sources(&self) -> &NoSources {
        self.dispatch.sources()
    }
    fn source_tree_editable_policy(&self) -> SourceTreeEditablePolicy {
        self.dispatch.source_tree_editable_policy()
    }
    fn locations(&self) -> &IndexLocations {
        self.dispatch.locations()
    }
    fn workspace_cache(&self) -> &WorkspaceCache {
        self.dispatch.workspace_cache()
    }
    fn extra_build_requires(&self) -> &ExtraBuildRequires {
        self.dispatch.extra_build_requires()
    }
    fn extra_build_variables(&self) -> &ExtraBuildVariables {
        self.dispatch.extra_build_variables()
    }

    async fn observe_build_requirements<'a>(
        &'a self,
        kind: BuildRequirementKind,
        requirements: &'a [Requirement],
    ) -> Result<(), AnyErrorBuild> {
        let requirements = self
            .normalize(requirements)
            .map_err(BuildDispatchError::from)?;
        let mut state = self.state.lock().await;
        let (slot, expected) = match kind {
            BuildRequirementKind::Declared => (
                &mut state.declared,
                self.replay.map(LockedBuild::declared_requirements),
            ),
            BuildRequirementKind::Backend => (
                &mut state.backend,
                self.replay.map(LockedBuild::backend_requirements),
            ),
        };
        if slot.is_some() {
            return Err(BuildDispatchError::from(anyhow!(
                "The frontend reported {kind:?} requirements twice"
            ))
            .into());
        }
        if expected.is_some_and(|expected| expected != requirements) {
            return Err(BuildDispatchError::from(anyhow!(
                "The {kind:?} build requirements differ from the build lock"
            ))
            .into());
        }
        *slot = Some(requirements);
        Ok(())
    }

    async fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
        build_stack: &'a BuildStack,
    ) -> Result<ResolvedRequirements, impl IsBuildBackendError> {
        let mut state = self.state.lock().await;
        let stage = match state.resolved {
            0 if state.declared.is_some() => BuildStage::Bootstrap,
            1 if state.backend.is_some() => BuildStage::Final,
            _ => {
                return Err(BuildDispatchError::from(anyhow!(
                    "Unexpected build resolution stage"
                )));
            }
        };
        let result = if let Some(replay) = self.replay {
            replay
                .materialize(
                    stage,
                    requirements,
                    self.root,
                    self.dispatch.interpreter,
                    self.build_options(),
                )
                .map_err(anyhow::Error::from)?
        } else {
            let upgrades = UpgradePackages::for_non_project(&self.upgrades);
            let preferences = self
                .previous
                .and_then(|previous| previous.graph(stage))
                .into_iter()
                .flat_map(Lock::packages)
                .filter(|package| !upgrades.contains(package.name()))
                .filter_map(|package| package.version().map(|version| (package, version)))
                .map(|(package, version)| {
                    Ok(Preference::from_locked(
                        package.name().clone(),
                        version.clone(),
                        package.index(self.root)?,
                        package.fork_markers().to_vec(),
                    ))
                })
                .collect::<Result<Vec<_>, uv_lock::LockError>>()
                .map_err(anyhow::Error::from)?;
            let constraints = Constraints::from_specifications(
                self.dispatch
                    .constraints
                    .specifications()
                    .cloned()
                    .chain(self.upgrades.constraints().cloned().map(Into::into)),
            );
            let (mut output, hasher) = self
                .dispatch
                .resolve_build_graph_with_preferences(
                    requirements,
                    build_stack,
                    self,
                    preferences,
                    &constraints,
                )
                .await?;
            self.observe_wheels(&mut output, &hasher).await?;
            let manifest = ResolverManifest::new(
                [],
                requirements.to_vec(),
                self.dispatch.constraints.requirements().cloned(),
                [],
                [],
                [],
                [],
                [],
            )
            .relative_to(self.root)
            .map_err(anyhow::Error::from)?;
            let lock = Lock::from_resolution(
                &output,
                manifest,
                self.root,
                vec![],
                self.locations(),
                false,
            )
            .map_err(anyhow::Error::from)?;
            let resolution = Resolution::from(output);
            ensure_build_wheels(&resolution).map_err(anyhow::Error::from)?;
            let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Require)
                .map_err(anyhow::Error::from)?;
            state.graphs.push(lock);
            ResolvedRequirements::new(resolution, hasher)
        };
        state.resolved += 1;
        Ok::<_, BuildDispatchError>(result)
    }

    async fn install<'a>(
        &'a self,
        requirements: &'a ResolvedRequirements,
        venv: &'a PythonEnvironment,
        build_stack: &'a BuildStack,
    ) -> Result<Vec<CachedDist>, impl IsBuildBackendError> {
        ensure_build_wheels(requirements.resolution()).map_err(anyhow::Error::from)?;
        self.dispatch.install(requirements, venv, build_stack).await
    }

    fn setup_build<'a>(
        &'a self,
        source: &'a Path,
        _subdirectory: Option<&'a Path>,
        _install_path: &'a Path,
        _stop_discovery_at: Option<&'a Path>,
        _version_id: Option<&'a str>,
        _dist: Option<&'a SourceDist>,
        _sources: &'a NoSources,
        _build_kind: BuildKind,
        _build_output: BuildOutput,
        _build_stack: BuildStack,
    ) -> impl Future<Output = Result<SourceBuild, impl IsBuildBackendError>> + 'a {
        future::ready(Err::<SourceBuild, _>(BuildDispatchError::from(anyhow!(
            "Nested source builds are not supported by build dependency locking: {}",
            source.display()
        ))))
    }

    fn direct_build<'a>(
        &'a self,
        _source: &'a Path,
        _subdirectory: Option<&'a Path>,
        _output_dir: &'a Path,
        _sources: NoSources,
        _build_kind: BuildKind,
        _version_id: Option<&'a str>,
    ) -> impl Future<Output = Result<Option<DistFilename>, impl IsBuildBackendError>> + 'a {
        future::ready(Ok::<_, BuildDispatchError>(None))
    }
}
