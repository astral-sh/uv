//! Resolve build environments independently of the runtime environment.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use anyhow::Context;
use futures::{StreamExt, TryStreamExt, stream};
use uv_client::RegistryClient;
use uv_command_support::Printer;
use uv_configuration::{Concurrency, Constraints, ExtrasSpecification, Reinstall, Upgrade};
use uv_dispatch::BuildDispatch;
use uv_distribution_types::{
    ArchiveHashPolicy, ExtraBuildRequires, IndexLocations, IndexMetadata, Name, Requirement,
    RequirementSource, UnresolvedRequirementSpecification,
};
use uv_lock::{
    BuildInputs, BuildUniverse, BuildUniverseOwner, Lock, Package, PackageId, ResolverManifest,
};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::{Conflicts, SupportedEnvironments};
use uv_python_interpreter::Interpreter;
use uv_resolve_operations::locked_requirements::read_lock_requirements;
use uv_resolve_operations::loggers::SummaryResolveLogger;
use uv_resolver::{
    FlatIndex, InMemoryIndex, NoSolutionError, Options, Preference, PythonRequirement,
    ResolverEnvironment, ResolverOutput,
};
use uv_types::HashStrategy;
use version_ranges::Ranges;

use crate::LockError;

struct DeclaredBuild {
    inputs: BuildInputs,
    backend: String,
    owner: BuildUniverseOwner,
}

struct Cluster {
    backend: String,
    /// Strongest comparable declaration, used only for grouping (never passed to the resolver).
    signature: Vec<Requirement>,
    owners: Vec<BuildUniverseOwner>,
}

/// Every solution of `stronger` must satisfy `weaker`. Mere overlap isn't enough: another
/// requirement in the same environment could force a version outside the overlap.
fn implies(stronger: &[Requirement], weaker: &[Requirement]) -> bool {
    weaker.iter().all(|wanted| {
        stronger.iter().any(|req| {
            req.name == wanted.name
                && req.extras == wanted.extras
                && req.marker == wanted.marker
                && match (&req.source, &wanted.source) {
                    (
                        RequirementSource::Registry {
                            specifier: left,
                            index: a,
                            conflict: ac,
                        },
                        RequirementSource::Registry {
                            specifier: right,
                            index: b,
                            conflict: bc,
                        },
                    ) if a == b && ac == bc => Ranges::<Version>::from(left.clone())
                        .subset_of(&Ranges::from(right.clone())),
                    (left, right) => left == right,
                }
        })
    })
}

/// Two differently named tools can need incompatible versions of a third package, yet never
/// be installed together (e.g. Windows vs Unix). Split only the failed fork, and only where a
/// root implicated in the conflict changes its requirements.
fn split_conflicting_markers(
    forks: &mut Vec<MarkerTree>,
    conflict: &NoSolutionError,
    requirements: &BTreeSet<Requirement>,
) -> bool {
    let Some(scope) = conflict.environment().fork_markers() else {
        return false;
    };
    let names: BTreeSet<_> = conflict.packages().collect();
    let Some(marker) = requirements
        .iter()
        .filter(|req| names.contains(&req.name))
        .map(|req| req.marker)
        .find(|marker| !scope.is_disjoint(*marker) && !scope.is_disjoint(marker.negate()))
    else {
        return false;
    };

    if forks.is_empty() {
        forks.push(MarkerTree::TRUE);
    }
    let Some(parent) = forks.iter_mut().find(|fork| !fork.is_disjoint(scope)) else {
        return false;
    };
    let complement = parent.and(marker.negate());
    *parent = parent.and(marker);
    forks.push(complement);
    true
}

/// A build-only dependency may first appear on 3.12+, then be needed on 3.11 through a
/// different backend. Revisit it only when its Python coverage grows, so cycles terminate.
#[derive(Default)]
struct BuildQueue {
    coverage: BTreeMap<PackageId, MarkerTree>,
    pending: BTreeMap<PackageId, (Package, MarkerTree)>,
}

impl BuildQueue {
    fn extend(
        &mut self,
        lock: &Lock,
        resolution: &ResolverOutput,
        root: &Path,
    ) -> Result<(), LockError> {
        let environments = Lock::build_python_environments(resolution, root)?;
        for package in lock.packages() {
            let python = environments
                .get(package.id())
                .copied()
                .unwrap_or(MarkerTree::TRUE);
            let covered = self
                .coverage
                .entry(package.id().clone())
                .or_insert(MarkerTree::FALSE);
            let combined = covered.or(python);
            if combined != *covered {
                *covered = combined;
                self.pending
                    .insert(package.id().clone(), (package.clone(), combined));
            }
        }
        Ok(())
    }
}

pub(crate) struct BuildResolver<'a> {
    pub root: &'a Path,
    pub client: &'a RegistryClient,
    pub dispatch: &'a BuildDispatch<'a>,
    pub index_locations: &'a IndexLocations,
    pub flat_index: &'a FlatIndex,
    pub index: InMemoryIndex,
    pub interpreter: &'a Interpreter,
    pub python: PythonRequirement,
    pub options: Options,
    pub environments: SupportedEnvironments,
    pub constraints: &'a Constraints,
    pub hasher: &'a HashStrategy,
    pub extra: &'a ExtraBuildRequires,
    pub concurrency: &'a Concurrency,
    pub printer: Printer,
}

impl BuildResolver<'_> {
    async fn declarations(
        &self,
        package: &Package,
        python: MarkerTree,
        runtime: &ResolverOutput,
    ) -> anyhow::Result<Option<DeclaredBuild>> {
        let Some(source) = package.build_source(self.root)? else {
            return Ok(None);
        };
        let digests = package
            .source_hash()
            .map(|digest| self.hasher.constrained_archive_digests(&source, digest))
            .transpose()
            .with_context(|| {
                format!("Source archive for `{source}` does not match the allowed build hashes")
            })?;
        let hashes = digests
            .as_deref()
            .map_or(ArchiveHashPolicy::None, ArchiveHashPolicy::All);
        let (backend, mut requirements) = self
            .dispatch
            .read_build_system(&source, hashes)
            .await
            .with_context(|| format!("Failed to read build requirements for `{}`", package.id()))?;
        let extra = self
            .extra
            .get(package.name())
            .map_or(&[][..], Vec::as_slice);
        let inputs = BuildInputs::new(
            self.root,
            if package.has_local_build_source() {
                &requirements
            } else {
                &[]
            },
            extra,
        )?;
        for extra in extra {
            if extra.match_runtime {
                anyhow::ensure!(
                    extra.requirement.source.is_empty(),
                    "`match-runtime` cannot be combined with a version or source for `{}`",
                    extra.requirement.name
                );
                let mut found = false;
                for (_, dist) in runtime
                    .base_dists()
                    .filter(|(_, dist)| dist.name() == &extra.requirement.name)
                {
                    found = true;
                    requirements.push(Requirement {
                        source: RequirementSource::from(&dist.dist),
                        marker: extra.requirement.marker.and(dist.marker.pep508()),
                        ..extra.requirement.clone()
                    });
                }
                anyhow::ensure!(
                    found,
                    "No runtime version for build requirement `{}` of `{}`",
                    extra.requirement.name,
                    package.id()
                );
            } else {
                requirements.push(extra.requirement.clone());
            }
        }
        for req in &mut requirements {
            req.marker = req.marker.and(python);
            req.origin = None;
        }
        requirements.retain(|req| !req.marker.is_false());
        requirements.sort();
        requirements.dedup();
        Ok(Some(DeclaredBuild {
            inputs,
            backend: backend
                .split(['.', ':'])
                .next()
                .unwrap_or(&backend)
                .to_owned(),
            owner: BuildUniverseOwner {
                id: package.id().clone(),
                requirements,
            },
        }))
    }

    async fn solve(
        &self,
        requirements: &BTreeSet<Requirement>,
        preferences: &[Preference],
        forks: Vec<MarkerTree>,
    ) -> Result<ResolverOutput, uv_resolve_operations::Error> {
        uv_resolve_operations::resolve(
            requirements
                .iter()
                .cloned()
                .map(UnresolvedRequirementSpecification::from)
                .collect(),
            self.constraints.specifications().cloned().collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            BTreeMap::new(),
            &ExtrasSpecification::default(),
            &BTreeMap::new(),
            preferences.to_vec(),
            None,
            self.hasher,
            &Reinstall::default(),
            &Upgrade::default(),
            None,
            ResolverEnvironment::universal(forks),
            self.python.clone(),
            self.interpreter.markers(),
            Conflicts::empty(),
            self.client,
            self.flat_index,
            &self.index,
            self.dispatch,
            self.concurrency,
            self.options.clone(),
            None,
            Box::new(SummaryResolveLogger),
            self.printer,
        )
        .await
        .map(|(resolution, _)| resolution)
    }

    pub(crate) async fn resolve(
        &self,
        mut lock: Lock,
        runtime: &ResolverOutput,
        previous: Option<&Lock>,
        prefer_previous_versions: bool,
        upgrade: &Upgrade,
    ) -> Result<Lock, LockError> {
        let upgrading = previous.map(|lock| (lock, lock.upgrade_packages(upgrade)));
        lock.complete_registry_hashes(
            self.client,
            self.root,
            self.concurrency.downloads,
            upgrading.as_ref(),
        )
        .await?;
        let mut preferences =
            read_lock_requirements(&lock, self.root, &Upgrade::default())?.preferences;
        if prefer_previous_versions && let Some((previous, upgraded)) = &upgrading {
            let runtime_names: BTreeSet<_> = lock.packages().iter().map(Package::name).collect();
            for package in previous.all_packages() {
                if !upgrade.is_all()
                    && !upgraded.contains(package.name())
                    && !runtime_names.contains(package.name())
                    && let Some(version) = package.version()
                {
                    preferences.push(Preference::from_locked(
                        package.name().clone(),
                        version.clone(),
                        package.index(self.root)?,
                        package.fork_markers().to_vec(),
                    ));
                }
            }
        }
        let mut inputs = BTreeMap::new();
        let mut queue = BuildQueue::default();
        queue.extend(&lock, runtime, self.root)?;
        let mut resolved: Vec<(Vec<Requirement>, BuildUniverse)> = Vec::new();
        while !queue.pending.is_empty() {
            // A wider solve supersedes an owner's earlier, narrower build environment.
            for (_, universe) in &mut resolved {
                universe
                    .owners
                    .retain(|owner| !queue.pending.contains_key(&owner.id));
            }
            // Keep output order deterministic even though downloads run concurrently.
            let declarations = stream::iter(std::mem::take(&mut queue.pending).into_values())
                .map(|(package, python)| async move {
                    self.declarations(&package, python, runtime).await
                })
                .buffered(self.concurrency.downloads)
                .try_collect::<Vec<_>>()
                .await?;
            let mut clusters: Vec<Cluster> = Vec::new();
            for declaration in declarations.into_iter().flatten() {
                let DeclaredBuild {
                    backend,
                    owner,
                    inputs: declared_inputs,
                } = declaration;
                if declared_inputs != BuildInputs::default() {
                    inputs.insert(owner.id.clone(), declared_inputs);
                }
                if owner.requirements.is_empty() {
                    continue;
                }
                if let Some((_, universe)) = resolved
                    .iter_mut()
                    .find(|(requirements, _)| implies(requirements, &owner.requirements))
                {
                    universe.owners.push(owner);
                    continue;
                }
                // A platform-specific spelling doesn't need a separate resolver run. Python
                // bounds do matter: two versions of the build tool may target different Pythons.
                let mut signature = owner.requirements.clone();
                for req in &mut signature {
                    req.marker = req.marker.only_python_version();
                }
                if let Some(cluster) = clusters.iter_mut().find(|cluster| {
                    implies(&cluster.signature, &signature)
                        || implies(&signature, &cluster.signature)
                }) {
                    if implies(&signature, &cluster.signature) {
                        cluster.signature = signature;
                    }
                    cluster.owners.push(owner);
                } else {
                    clusters.push(Cluster {
                        backend,
                        signature,
                        owners: vec![owner],
                    });
                }
            }

            let mut clusters = VecDeque::from(clusters);
            'clusters: while let Some(cluster) = clusters.pop_front() {
                // Keep *all* original markers in the solve. For example, a Windows owner must
                // not inherit the Linux owner's "only on Linux" condition for the same helper.
                let requirements: BTreeSet<_> = cluster
                    .owners
                    .iter()
                    .flat_map(|owner| owner.requirements.iter().cloned())
                    .collect();
                let mut forks = self.environments.clone().into_markers();
                let resolution = loop {
                    let err = match self.solve(&requirements, &preferences, forks.clone()).await {
                        Ok(resolution) => break resolution,
                        Err(err) => err,
                    };
                    if let Some(conflict) = err.as_no_solution() {
                        if cluster.owners.len() > 1 {
                            // A conflict between builds is not a conflict in either individual build.
                            clusters.extend(cluster.owners.into_iter().map(|owner| Cluster {
                                backend: cluster.backend.clone(),
                                signature: Vec::new(),
                                owners: vec![owner],
                            }));
                            continue 'clusters;
                        }
                        if split_conflicting_markers(&mut forks, conflict, &requirements) {
                            continue;
                        }
                    }
                    return Err(err.with_resolution_context("build").into());
                };
                let mut build = Lock::from_resolution(
                    &resolution,
                    ResolverManifest::default(),
                    self.root,
                    self.environments.clone().into_markers(),
                    self.index_locations,
                    false,
                )?;
                build
                    .complete_registry_hashes(
                        self.client,
                        self.root,
                        self.concurrency.downloads,
                        upgrading.as_ref(),
                    )
                    .await?;
                queue.extend(&build, &resolution, self.root)?;
                resolved.push((
                    requirements.into_iter().collect(),
                    BuildUniverse {
                        backend: cluster.backend,
                        owners: cluster.owners,
                        lock: build,
                    },
                ));
            }
        }
        Ok(lock.with_build_universes(
            resolved.into_iter().map(|(_, universe)| universe).collect(),
            &inputs,
            self.root,
        )?)
    }
}

/// Re-read mutable inputs without visiting a registry. Only current declarations can authorize
/// a member-specific index; a stale build dependency in the lockfile cannot authorize itself.
pub(crate) async fn validated_build_indexes(
    lock: &Lock,
    root: &Path,
    dispatch: &BuildDispatch<'_>,
    extra: &ExtraBuildRequires,
) -> anyhow::Result<Option<Vec<IndexMetadata>>> {
    if !lock.locks_build_dependencies() {
        return Ok(None);
    }
    let mut indexes = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for package in lock.all_packages() {
        if !seen.insert(package.id()) {
            continue;
        }
        let extra = extra.get(package.name()).map_or(&[][..], Vec::as_slice);
        let requires = if package.has_local_build_source() {
            if let Some(source) = package.build_source(root)? {
                dispatch
                    .read_build_system(&source, ArchiveHashPolicy::None)
                    .await?
                    .1
            } else {
                Vec::new()
            }
        } else if extra.is_empty() || package.build_source(root)?.is_some() {
            Vec::new()
        } else {
            continue;
        };
        if !package.matches_build_inputs(&BuildInputs::new(root, &requires, extra)?) {
            return Ok(None);
        }
        for req in requires
            .iter()
            .chain(extra.iter().map(|entry| &entry.requirement))
        {
            if let RequirementSource::Registry {
                index: Some(index), ..
            } = &req.source
            {
                indexes.insert(index.clone());
            }
        }
    }
    Ok(Some(indexes.into_iter().collect()))
}
