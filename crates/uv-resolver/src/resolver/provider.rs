use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use url::Url;

use distribution_types::{Dist, IndexLocations};
use platform_tags::Tags;
use pypi_types::Metadata21;
use uv_client::{FlatIndex, RegistryClient};
use uv_distribution::DistributionDatabase;
use uv_normalize::PackageName;
use uv_traits::{BuildContext, NoBinary};

use crate::python_requirement::PythonRequirement;
use crate::version_map::VersionMap;

type PackageVersionsResult = Result<VersionsResponse, uv_client::Error>;
type WheelMetadataResult = Result<(Metadata21, Option<Url>), uv_distribution::Error>;

/// The response when requesting versions for a package
#[derive(Debug)]
pub enum VersionsResponse {
    /// The package was found in the registry with the included versions
    Found(VersionMap),
    /// The package was not found in the registry
    NotFound,
    /// The package was not found in the local registry
    NoIndex,
    /// The package was not found in the cache and the network is not available.
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
    /// Allow moving the parameters to `VersionMap::from_metadata` to a different thread.
    inner: Arc<DefaultResolverProviderInner>,
}

pub struct DefaultResolverProviderInner {
    /// The [`RegistryClient`] used to query the index.
    client: RegistryClient,
    /// These are the entries from `--find-links` that act as overrides for index responses.
    flat_index: FlatIndex,
    tags: Tags,
    python_requirement: PythonRequirement,
    exclude_newer: Option<DateTime<Utc>>,
    no_binary: NoBinary,
}

impl<'a, Context: BuildContext + Send + Sync> Deref for DefaultResolverProvider<'a, Context> {
    type Target = DefaultResolverProviderInner;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
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
        exclude_newer: Option<DateTime<Utc>>,
        no_binary: &'a NoBinary,
    ) -> Self {
        Self {
            fetcher,
            inner: Arc::new(DefaultResolverProviderInner {
                client: client.clone(),
                flat_index: flat_index.clone(),
                tags: tags.clone(),
                python_requirement,
                exclude_newer,
                no_binary: no_binary.clone(),
            }),
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
        let result = self.client.simple(package_name).await;

        // If the "Simple API" request was successful, convert to `VersionMap` on the Tokio
        // threadpool, since it can be slow.
        match result {
            Ok((index, metadata)) => {
                let self_send = self.inner.clone();
                let package_name_owned = package_name.clone();
                Ok(tokio::task::spawn_blocking(move || {
                    VersionsResponse::Found(VersionMap::from_metadata(
                        metadata,
                        &package_name_owned,
                        &index,
                        &self_send.tags,
                        &self_send.python_requirement,
                        self_send.exclude_newer.as_ref(),
                        self_send.flat_index.get(&package_name_owned).cloned(),
                        &self_send.no_binary,
                    ))
                })
                .await
                .expect("Tokio executor failed, was there a panic?"))
            }
            Err(err) => match err.into_kind() {
                uv_client::ErrorKind::PackageNotFound(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(VersionMap::from(flat_index)))
                    } else {
                        Ok(VersionsResponse::NotFound)
                    }
                }
                uv_client::ErrorKind::NoIndex(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(VersionMap::from(flat_index)))
                    } else if self.flat_index.offline() {
                        Ok(VersionsResponse::Offline)
                    } else {
                        Ok(VersionsResponse::NoIndex)
                    }
                }
                uv_client::ErrorKind::Offline(_) => {
                    if let Some(flat_index) = self.flat_index.get(package_name).cloned() {
                        Ok(VersionsResponse::Found(VersionMap::from(flat_index)))
                    } else {
                        Ok(VersionsResponse::Offline)
                    }
                }
                kind => Err(kind.into()),
            },
        }
    }

    async fn get_or_build_wheel_metadata<'io>(&'io self, dist: &'io Dist) -> WheelMetadataResult {
        self.fetcher.get_or_build_wheel_metadata(dist).await
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
