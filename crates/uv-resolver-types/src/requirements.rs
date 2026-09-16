use std::borrow::Cow;

use futures::{StreamExt, TryStreamExt};
use petgraph::graph::NodeIndex;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_client::{FileHashError, RegistryClient};
use uv_configuration::BuildOptions;
use uv_distribution_types::{
    BuiltDist, Dist, File, FileLocation, Name, PinnedDist, ResolvedDist, SourceDist,
};
use uv_normalize::PackageName;
use uv_pypi_types::HashDigest;

use crate::{ResolutionGraphNode, ResolverOutput};

/// An immutable requirements export with hashes prepared for the packages it will emit.
///
/// Borrowing the resolution prevents the selected distributions from changing after hashing.
/// Package-level hashes are only reused for pins that do not require artifact-specific hashes.
#[derive(Debug)]
pub struct RequirementsExport<'a> {
    resolution: &'a ResolverOutput,
    omit: &'a [PackageName],
    hashes: Option<HashedArtifacts<'a>>,
}

impl<'a> RequirementsExport<'a> {
    /// Prepare hashes without modifying the resolution or downloading omitted packages.
    pub async fn new(
        resolution: &'a ResolverOutput,
        omit: &'a [PackageName],
        generate_hashes: bool,
        client: &RegistryClient,
        concurrency: usize,
    ) -> Result<Self, FileHashError> {
        let hashes = if generate_hashes {
            let pins = resolution.graph.node_indices().filter_map(|index| {
                let ResolutionGraphNode::Dist(distribution) = &resolution.graph[index] else {
                    return None;
                };
                (!omit.contains(&distribution.name)).then_some((index, &distribution.dist))
            });
            Some(
                HashedArtifacts::generate(
                    pins,
                    &resolution.options.build_options,
                    client,
                    concurrency,
                )
                .await?,
            )
        } else {
            None
        };
        Ok(Self {
            resolution,
            omit,
            hashes,
        })
    }

    pub fn resolution(&self) -> &'a ResolverOutput {
        self.resolution
    }

    pub fn omit(&self) -> &'a [PackageName] {
        self.omit
    }

    /// Flatten artifact hashes only when serializing a requirements entry.
    pub fn hashes(&self, index: NodeIndex) -> Cow<'_, [HashDigest]> {
        let Some(hashes) = &self.hashes else {
            return Cow::Borrowed(&[]);
        };
        let ResolutionGraphNode::Dist(distribution) = &self.resolution.graph[index] else {
            return Cow::Borrowed(&[]);
        };
        if self.omit.contains(&distribution.name) {
            return Cow::Borrowed(&[]);
        }
        if distribution.dist.requires_artifact_hashes() {
            let mut hashes = hashes
                .files
                .get(&index)
                .into_iter()
                .flatten()
                .flat_map(|file| {
                    file.hashes.iter().chain(
                        hashes
                            .computed
                            .get(&file.url)
                            .filter(|_| file.hashes.is_empty()),
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            hashes.sort_unstable();
            hashes.dedup();
            Cow::Owned(hashes)
        } else {
            Cow::Borrowed(distribution.hashes.as_slice())
        }
    }
}

/// Retained registry files and the hashes computed for files without advertised hashes.
///
/// Construction requires immutable installation pins and completes all missing hashes before
/// returning. Hashes remain associated with their files rather than a package-level digest list.
#[derive(Debug)]
struct HashedArtifacts<'a> {
    files: FxHashMap<NodeIndex, Vec<&'a File>>,
    computed: FxHashMap<&'a FileLocation, HashDigest>,
}

impl<'a> HashedArtifacts<'a> {
    async fn generate(
        pins: impl Iterator<Item = (NodeIndex, &'a PinnedDist)>,
        build_options: &BuildOptions,
        client: &RegistryClient,
        concurrency: usize,
    ) -> Result<Self, FileHashError> {
        let mut files = FxHashMap::default();
        let mut missing = FxHashSet::default();
        for (index, pin) in pins {
            if !pin.requires_artifact_hashes() {
                continue;
            }
            let ResolvedDist::Installable { dist, .. } = pin.as_ref() else {
                continue;
            };
            let (wheels, sdist) = match dist.as_ref() {
                Dist::Built(BuiltDist::Registry(dist)) => (&dist.wheels, dist.sdist.as_ref()),
                Dist::Source(SourceDist::Registry(dist)) => (&dist.wheels, Some(dist)),
                Dist::Built(
                    BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_),
                )
                | Dist::Source(
                    SourceDist::DirectUrl(_)
                    | SourceDist::GitDirectory(_)
                    | SourceDist::GitPath(_)
                    | SourceDist::Path(_)
                    | SourceDist::Directory(_),
                ) => continue,
            };
            let retained = wheels
                .iter()
                .filter(|_| !build_options.no_binary_package(dist.name()))
                .map(|wheel| wheel.file.as_ref())
                .chain(
                    sdist
                        .filter(|_| !build_options.no_build_package(dist.name()))
                        .into_iter()
                        .map(|source| source.file.as_ref()),
                )
                .collect::<Vec<_>>();
            for file in &retained {
                if file.hashes.is_empty() {
                    missing.insert(&file.url);
                }
            }
            files.insert(index, retained);
        }

        // Forks, extras, and groups can share a file; hash each location only once.
        let computed = futures::stream::iter(missing)
            .map(|location| async move {
                let hash = client.hash_file(&location.to_url()?).await?;
                Ok::<_, FileHashError>((location, hash))
            })
            .buffer_unordered(concurrency)
            .try_collect()
            .await?;
        Ok(Self { files, computed })
    }
}
