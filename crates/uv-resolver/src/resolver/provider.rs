use std::future::Future;

use anyhow::Result;
use chrono::{DateTime, Utc};

use distribution_types::{Dist, IndexLocations};
use platform_tags::Tags;

use uv_client::RegistryClient;
use uv_configuration::{NoBinary, NoBuild};
use uv_distribution::{ArchiveMetadata, DistributionDatabase};
use uv_normalize::PackageName;
use uv_types::{BuildContext, HashStrategy};

use crate::flat_index::FlatIndex;
use crate::python_requirement::PythonRequirement;
use crate::version_map::VersionMap;
use crate::yanks::AllowedYanks;

pub type PackageVersionsResult = Result<VersionsResponse, uv_client::Error>;
pub type WheelMetadataResult = Result<MetadataResponse, uv_distribution::Error>;

/// The response when requesting versions for a package
#[derive(Debug)]
pub enum VersionsResponse {
    /// The package was found in the registry with the included versions
    Found(Vec<VersionMap>),
    /// The package was not found in the registry
    NotFound,
    /// The package was not found in the local registry
    NoIndex,
    /// The package was not found in the cache and the network is not available.
    Offline,
}

#[derive(Debug)]
pub enum MetadataResponse {
    /// The wheel metadata was found and parsed successfully.
    Found(ArchiveMetadata),
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata(Box<pypi_types::MetadataError>),
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(Box<uv_distribution::Error>),
    /// The wheel has an invalid structure.
    InvalidStructure(Box<install_wheel_rs::Error>),
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
}

pub trait ResolverProvider: Send + Sync {
    /// Get the version map for a package.
    fn get_package_versions<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> impl Future<Output = PackageVersionsResult> + Send + 'io;

    /// Get the metadata for a distribution.
    ///
    /// For a wheel, this is done by querying it's (remote) metadata, for a source dist we
    /// (fetch and) build the source distribution and return the metadata from the built
    /// distribution.
    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> impl Future<Output = WheelMetadataResult> + Send + 'io;

    fn index_locations(&self) -> &IndexLocations;

    /// Set the [`uv_distribution::Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl uv_distribution::Reporter + 'static) -> Self;
}

/// The main IO backend for the resolver, which does cached requests network requests using the
/// [`RegistryClient`] and [`DistributionDatabase`].
pub struct DefaultResolverProvider<'a, Context: BuildContext + Send + Sync> {
    /// The [`DistributionDatabase`] used to build source distributions.
    fetcher: DistributionDatabase<'a, Context>,
    /// The [`RegistryClient`] used to query the index.
    client: RegistryClient,
    /// These are the entries from `--find-links` that act as overrides for index responses.
    flat_index: FlatIndex,
    tags: Tags,
    python_requirement: PythonRequirement,
    allowed_yanks: AllowedYanks,
    hasher: HashStrategy,
    exclude_newer: Option<DateTime<Utc>>,
    no_binary: NoBinary,
    no_build: NoBuild,
}

impl<'a, Context: BuildContext + Send + Sync> DefaultResolverProvider<'a, Context> {
    /// Reads the flat index entries and builds the provider.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: &'a RegistryClient,
        fetcher: DistributionDatabase<'a, Context>,
        flat_index: &'a FlatIndex,
        tags: &'a Tags,
        python_requirement: PythonRequirement,
        allowed_yanks: AllowedYanks,
        hasher: &'a HashStrategy,
        exclude_newer: Option<DateTime<Utc>>,
        no_binary: &'a NoBinary,
        no_build: &'a NoBuild,
    ) -> Self {
        Self {
            fetcher,
            client: client.clone(),
            flat_index: flat_index.clone(),
            tags: tags.clone(),
            python_requirement,
            allowed_yanks,
            hasher: hasher.clone(),
            exclude_newer,
            no_binary: no_binary.clone(),
            no_build: no_build.clone(),
        }
    }
}

impl<'a, Context: BuildContext + Send + Sync> ResolverProvider
    for DefaultResolverProvider<'a, Context>
{
    /// Make a "Simple API" request for the package and convert the result to a [`VersionMap`].
    async fn get_package_versions<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> PackageVersionsResult {
        match self.client.simple(package_name).await {
            Ok(results) => Ok(VersionsResponse::Found(
                results
                    .into_iter()
                    .map(|(index, metadata)| {
                        VersionMap::from_metadata(
                            metadata,
                            package_name,
                            &index,
                            &self.tags,
                            &self.python_requirement,
                            &self.allowed_yanks,
                            &self.hasher,
                            self.exclude_newer.as_ref(),
                            self.flat_index.get(package_name).cloned(),
                            &self.no_binary,
                            &self.no_build,
                        )
                    })
                    .collect(),
            )),
            Err(err) => match err.into_kind() {
                uv_client::ErrorKind::PackageNotFound(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else {
                        Ok(VersionsResponse::NotFound)
                    }
                }
                uv_client::ErrorKind::NoIndex(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else if self.flat_index.offline() {
                        Ok(VersionsResponse::Offline)
                    } else {
                        Ok(VersionsResponse::NoIndex)
                    }
                }
                uv_client::ErrorKind::Offline(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else {
                        Ok(VersionsResponse::Offline)
                    }
                }
                kind => Err(kind.into()),
            },
        }
    }

    /// Fetch the metadata for a distribution, building it if necessary.
    async fn get_or_build_wheel_metadata<'io>(&'io self, dist: &'io Dist) -> WheelMetadataResult {
        match self
            .fetcher
            .get_or_build_wheel_metadata(dist, self.hasher.get(dist))
            .await
        {
            Ok(metadata) => Ok(MetadataResponse::Found(metadata)),
            Err(err) => match err {
                uv_distribution::Error::Client(client) => match client.into_kind() {
                    uv_client::ErrorKind::Offline(_) => Ok(MetadataResponse::Offline),
                    uv_client::ErrorKind::MetadataParseError(_, _, err) => {
                        Ok(MetadataResponse::InvalidMetadata(err))
                    }
                    uv_client::ErrorKind::DistInfo(err) => {
                        Ok(MetadataResponse::InvalidStructure(Box::new(err)))
                    }
                    kind => Err(uv_client::Error::from(kind).into()),
                },
                uv_distribution::Error::VersionMismatch { .. } => {
                    Ok(MetadataResponse::InconsistentMetadata(Box::new(err)))
                }
                uv_distribution::Error::NameMismatch { .. } => {
                    Ok(MetadataResponse::InconsistentMetadata(Box::new(err)))
                }
                uv_distribution::Error::Metadata(err) => {
                    Ok(MetadataResponse::InvalidMetadata(Box::new(err)))
                }
                uv_distribution::Error::DistInfo(err) => {
                    Ok(MetadataResponse::InvalidStructure(Box::new(err)))
                }
                err => Err(err),
            },
        }
    }

    fn index_locations(&self) -> &IndexLocations {
        self.fetcher.index_locations()
    }

    /// Set the [`uv_distribution::Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: impl uv_distribution::Reporter + 'static) -> Self {
        Self {
            fetcher: self.fetcher.with_reporter(reporter),
            ..self
        }
    }
}
