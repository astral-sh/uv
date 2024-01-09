use std::future::Future;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::TryFutureExt;
use rustc_hash::FxHashMap;
use url::Url;

use crate::pubgrub::PubGrubVersion;
use distribution_types::Dist;
use platform_tags::Tags;
use puffin_client::{FlatIndex, RegistryClient};
use puffin_distribution::{DistributionDatabase, DistributionDatabaseError};
use puffin_normalize::PackageName;
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::python_requirement::PythonRequirement;
use crate::version_map::VersionMap;
use crate::yanks::AllowedYanks;

type VersionMapResponse = Result<VersionMap, puffin_client::Error>;
type WheelMetadataResponse = Result<(Metadata21, Option<Url>), DistributionDatabaseError>;

pub trait ResolverProvider: Send + Sync {
    /// Get the version map for a package.
    fn get_version_map<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> impl Future<Output = VersionMapResponse> + Send + 'io;

    /// Get the metadata for a distribution.
    ///
    /// For a wheel, this is done by querying it's (remote) metadata, for a source dist we
    /// (fetch and) build the source distribution and return the metadata from the built
    /// distribution.
    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> impl Future<Output = WheelMetadataResponse> + Send + 'io;

    /// Set the [`puffin_distribution::Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl puffin_distribution::Reporter + 'static) -> Self;
}

/// The main IO backend for the resolver, which does cached requests network requests using the
/// [`RegistryClient`] and [`DistributionDatabase`].
pub struct DefaultResolverProvider<'a, Context: BuildContext + Send + Sync> {
    client: &'a RegistryClient,
    /// These are the entries from `--find-links` that act as overrides for index responses.
    flat_index: FxHashMap<PackageName, FlatIndex<PubGrubVersion>>,
    fetcher: DistributionDatabase<'a, Context>,
    tags: &'a Tags,
    python_requirement: PythonRequirement<'a>,
    exclude_newer: Option<DateTime<Utc>>,
    allowed_yanks: AllowedYanks,
}

impl<'a, Context: BuildContext + Send + Sync> DefaultResolverProvider<'a, Context> {
    /// Reads the flat index entries and builds the provider.
    pub fn new(
        client: &'a RegistryClient,
        fetcher: DistributionDatabase<'a, Context>,
        tags: &'a Tags,
        python_requirement: PythonRequirement<'a>,
        exclude_newer: Option<DateTime<Utc>>,
        allowed_yanks: AllowedYanks,
    ) -> Result<Self, puffin_client::Error> {
        let flat_index_dists = client.flat_index()?;
        let flat_index = FlatIndex::from_dists(flat_index_dists, tags);

        Ok(Self {
            client,
            flat_index,
            fetcher,
            tags,
            python_requirement,
            exclude_newer,
            allowed_yanks,
        })
    }
}

impl<'a, Context: BuildContext + Send + Sync> ResolverProvider
    for DefaultResolverProvider<'a, Context>
{
    fn get_version_map<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> impl Future<Output = VersionMapResponse> + Send + 'io {
        let flat_index_override = self.flat_index.get(package_name).cloned();
        self.client
            .simple(package_name)
            .map_ok(move |(index, metadata)| {
                VersionMap::from_metadata(
                    metadata,
                    package_name,
                    &index,
                    self.tags,
                    &self.python_requirement,
                    &self.allowed_yanks,
                    self.exclude_newer.as_ref(),
                    flat_index_override,
                )
            })
    }

    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> impl Future<Output = WheelMetadataResponse> + Send + 'io {
        self.fetcher.get_or_build_wheel_metadata(dist)
    }

    /// Set the [`puffin_distribution::Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl puffin_distribution::Reporter + 'static) -> Self {
        Self {
            fetcher: self.fetcher.with_reporter(reporter),
            ..self
        }
    }
}
