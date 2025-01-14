use std::cmp::min;
use std::sync::Arc;

use itertools::Itertools;
use pubgrub::{Range, Ranges, Term};
use rustc_hash::{FxHashMap, FxHashSet};
use tokio::sync::mpsc::Sender;
use tracing::{debug, trace};

use crate::candidate_selector::CandidateSelector;
use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};
use crate::resolver::Request;
use crate::{
    InMemoryIndex, PythonRequirement, ResolveError, ResolverEnvironment, VersionsResponse,
};
use uv_distribution_types::{CompatibleDist, DistributionMetadata, IndexCapabilities, IndexUrl};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

enum BatchPrefetchStrategy {
    /// Go through the next versions assuming the existing selection and its constraints
    /// remain.
    Compatible {
        compatible: Range<Version>,
        previous: Version,
    },
    /// We encounter cases (botocore) where the above doesn't work: Say we previously selected
    /// a==x.y.z, which depends on b==x.y.z. a==x.y.z is incompatible, but we don't know that
    /// yet. We just selected b==x.y.z and want to prefetch, since for all versions of a we try,
    /// we have to wait for the matching version of b. The exiting range gives us only one version
    /// of b, so the compatible strategy doesn't prefetch any version. Instead, we try the next
    /// heuristic where the next version of b will be x.y.(z-1) and so forth.
    InOrder { previous: Version },
}

/// Prefetch a large number of versions if we already unsuccessfully tried many versions.
///
/// This is an optimization specifically targeted at cold cache urllib3/boto3/botocore, where we
/// have to fetch the metadata for a lot of versions.
///
/// Note that these all heuristics that could totally prefetch lots of irrelevant versions.
#[derive(Clone)]
pub(crate) struct BatchPrefetcher {
    // Types to determine whether we need to prefetch.
    tried_versions: FxHashMap<PackageName, FxHashSet<Version>>,
    last_prefetch: FxHashMap<PackageName, usize>,
    // Types to execute the prefetch.
    prefetch_runner: BatchPrefetcherRunner,
}

/// The types that are needed for running the batch prefetching after we determined that we need to
/// prefetch.
///
/// These types are shared (e.g., `Arc`) so they can be cheaply cloned and moved between threads.
#[derive(Clone)]
pub(crate) struct BatchPrefetcherRunner {
    capabilities: IndexCapabilities,
    index: InMemoryIndex,
    request_sink: Sender<Request>,
}

impl BatchPrefetcher {
    pub(crate) fn new(
        capabilities: IndexCapabilities,
        index: InMemoryIndex,
        request_sink: Sender<Request>,
    ) -> Self {
        Self {
            tried_versions: FxHashMap::default(),
            last_prefetch: FxHashMap::default(),
            prefetch_runner: BatchPrefetcherRunner {
                capabilities,
                index,
                request_sink,
            },
        }
    }

    /// Prefetch a large number of versions if we already unsuccessfully tried many versions.
    pub(crate) fn prefetch_batches(
        &mut self,
        next: &PubGrubPackage,
        index: Option<&IndexUrl>,
        version: &Version,
        current_range: &Range<Version>,
        unchangeable_constraints: Option<&Term<Range<Version>>>,
        python_requirement: &PythonRequirement,
        selector: &CandidateSelector,
        env: &ResolverEnvironment,
    ) -> Result<(), ResolveError> {
        let PubGrubPackageInner::Package {
            name,
            extra: None,
            dev: None,
            marker: MarkerTree::TRUE,
        } = &**next
        else {
            return Ok(());
        };

        let (num_tried, do_prefetch) = self.should_prefetch(next);
        if !do_prefetch {
            return Ok(());
        }
        let total_prefetch = min(num_tried, 50);

        // This is immediate, we already fetched the version map.
        let versions_response = if let Some(index) = index {
            self.prefetch_runner
                .index
                .explicit()
                .wait_blocking(&(name.clone(), index.clone()))
                .ok_or_else(|| ResolveError::UnregisteredTask(name.to_string()))?
        } else {
            self.prefetch_runner
                .index
                .implicit()
                .wait_blocking(name)
                .ok_or_else(|| ResolveError::UnregisteredTask(name.to_string()))?
        };

        let phase = BatchPrefetchStrategy::Compatible {
            compatible: current_range.clone(),
            previous: version.clone(),
        };

        self.last_prefetch.insert(name.clone(), num_tried);

        self.prefetch_runner.send_prefetch(
            name,
            unchangeable_constraints,
            total_prefetch,
            &versions_response,
            phase,
            python_requirement,
            selector,
            env,
        )?;

        Ok(())
    }

    /// Each time we tried a version for a package, we register that here.
    pub(crate) fn version_tried(&mut self, package: &PubGrubPackage, version: &Version) {
        // Only track base packages, no virtual packages from extras.
        let PubGrubPackageInner::Package {
            name,
            extra: None,
            dev: None,
            marker: MarkerTree::TRUE,
        } = &**package
        else {
            return;
        };
        self.tried_versions
            .entry(name.clone())
            .or_default()
            .insert(version.clone());
    }

    /// After 5, 10, 20, 40 tried versions, prefetch that many versions to start early but not
    /// too aggressive. Later we schedule the prefetch of 50 versions every 20 versions, this gives
    /// us a good buffer until we see prefetch again and is high enough to saturate the task pool.
    fn should_prefetch(&self, next: &PubGrubPackage) -> (usize, bool) {
        let PubGrubPackageInner::Package {
            name,
            extra: None,
            dev: None,
            marker: MarkerTree::TRUE,
        } = &**next
        else {
            return (0, false);
        };

        let num_tried = self.tried_versions.get(name).map_or(0, FxHashSet::len);
        let previous_prefetch = self.last_prefetch.get(name).copied().unwrap_or_default();
        let do_prefetch = (num_tried >= 5 && previous_prefetch < 5)
            || (num_tried >= 10 && previous_prefetch < 10)
            || (num_tried >= 20 && previous_prefetch < 20)
            || (num_tried >= 20 && num_tried - previous_prefetch >= 20);
        (num_tried, do_prefetch)
    }

    /// Log stats about how many versions we tried.
    pub(crate) fn log_tried_versions(&self) {
        let total_versions: usize = self.tried_versions.values().map(FxHashSet::len).sum();
        let mut tried_versions: Vec<_> = self
            .tried_versions
            .iter()
            .map(|(name, versions)| (name, versions.len()))
            .collect();
        tried_versions.sort_by(|(p1, c1), (p2, c2)| {
            c1.cmp(c2)
                .reverse()
                .then(p1.to_string().cmp(&p2.to_string()))
        });
        let counts = tried_versions
            .iter()
            .map(|(package, count)| format!("{package} {count}"))
            .join(", ");
        debug!("Tried {total_versions} versions: {counts}");
    }
}

impl BatchPrefetcherRunner {
    /// Given that the conditions for prefetching are met, find the versions to prefetch and
    /// send the prefetch requests.
    fn send_prefetch(
        &self,
        name: &PackageName,
        unchangeable_constraints: Option<&Term<Ranges<Version>>>,
        total_prefetch: usize,
        versions_response: &Arc<VersionsResponse>,
        mut phase: BatchPrefetchStrategy,
        python_requirement: &PythonRequirement,
        selector: &CandidateSelector,
        env: &ResolverEnvironment,
    ) -> Result<(), ResolveError> {
        let VersionsResponse::Found(ref version_map) = &**versions_response else {
            return Ok(());
        };

        let mut prefetch_count = 0;
        for _ in 0..total_prefetch {
            let candidate = match phase {
                BatchPrefetchStrategy::Compatible {
                    compatible,
                    previous,
                } => {
                    if let Some(candidate) =
                        selector.select_no_preference(name, &compatible, version_map, env)
                    {
                        let compatible = compatible.intersection(
                            &Range::singleton(candidate.version().clone()).complement(),
                        );
                        phase = BatchPrefetchStrategy::Compatible {
                            compatible,
                            previous: candidate.version().clone(),
                        };
                        candidate
                    } else {
                        // We exhausted the compatible version, switch to ignoring the existing
                        // constraints on the package and instead going through versions in order.
                        phase = BatchPrefetchStrategy::InOrder { previous };
                        continue;
                    }
                }
                BatchPrefetchStrategy::InOrder { previous } => {
                    let mut range = if selector.use_highest_version(name, env) {
                        Range::strictly_lower_than(previous)
                    } else {
                        Range::strictly_higher_than(previous)
                    };
                    // If we have constraints from root, don't go beyond those. Example: We are
                    // prefetching for foo 1.60 and have a dependency for `foo>=1.50`, so we should
                    // only prefetch 1.60 to 1.50, knowing 1.49 will always be rejected.
                    if let Some(unchangeable_constraints) = &unchangeable_constraints {
                        range = match unchangeable_constraints {
                            Term::Positive(constraints) => range.intersection(constraints),
                            Term::Negative(negative_constraints) => {
                                range.intersection(&negative_constraints.complement())
                            }
                        };
                    }
                    if let Some(candidate) =
                        selector.select_no_preference(name, &range, version_map, env)
                    {
                        phase = BatchPrefetchStrategy::InOrder {
                            previous: candidate.version().clone(),
                        };
                        candidate
                    } else {
                        // Both strategies exhausted their candidates.
                        break;
                    }
                }
            };

            let Some(dist) = candidate.compatible() else {
                continue;
            };

            // Avoid prefetching source distributions, which could be expensive.
            let Some(wheel) = dist.wheel() else {
                continue;
            };

            // Avoid prefetching built distributions that don't support _either_ PEP 658 (`.metadata`)
            // or range requests.
            if !(wheel.file.dist_info_metadata
                || self.capabilities.supports_range_requests(&wheel.index))
            {
                debug!("Abandoning prefetch for {wheel} due to missing registry capabilities");
                return Ok(());
            }

            // Avoid prefetching for distributions that don't satisfy the Python requirement.
            if !satisfies_python(dist, python_requirement) {
                continue;
            }

            let dist = dist.for_resolution();

            // Emit a request to fetch the metadata for this version.
            trace!(
                "Prefetching {prefetch_count} ({}) {}",
                match phase {
                    BatchPrefetchStrategy::Compatible { .. } => "compatible",
                    BatchPrefetchStrategy::InOrder { .. } => "in order",
                },
                dist
            );
            prefetch_count += 1;

            if self.index.distributions().register(candidate.version_id()) {
                let request = Request::from(dist);
                self.request_sink.blocking_send(request)?;
            }
        }

        match prefetch_count {
            0 => debug!("No `{name}` versions to prefetch"),
            1 => debug!("Prefetched 1 `{name}` version"),
            _ => debug!("Prefetched {prefetch_count} `{name}` versions"),
        }

        Ok(())
    }
}

fn satisfies_python(dist: &CompatibleDist, python_requirement: &PythonRequirement) -> bool {
    match dist {
        CompatibleDist::InstalledDist(_) => {}
        CompatibleDist::SourceDist { sdist, .. }
        | CompatibleDist::IncompatibleWheel { sdist, .. } => {
            // Source distributions must meet both the _target_ Python version and the
            // _installed_ Python version (to build successfully).
            if let Some(requires_python) = sdist.file.requires_python.as_ref() {
                if !python_requirement
                    .installed()
                    .is_contained_by(requires_python)
                {
                    return false;
                }
                if !python_requirement.target().is_contained_by(requires_python) {
                    return false;
                }
            }
        }
        CompatibleDist::CompatibleWheel { wheel, .. } => {
            // Wheels must meet the _target_ Python version.
            if let Some(requires_python) = wheel.file.requires_python.as_ref() {
                if !python_requirement.target().is_contained_by(requires_python) {
                    return false;
                }
            }
        }
    }

    true
}
