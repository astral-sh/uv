use std::future::Future;

use anyhow::Result;
use futures::FutureExt;
use url::Url;

use distribution_types::{Dist, IndexUrl};
use puffin_client::{FlatDistributions, FlatIndex, RegistryClient, SimpleMetadata};
use puffin_distribution::{DistributionDatabase, DistributionDatabaseError};
use puffin_normalize::PackageName;
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

#[derive(Debug)]
pub enum PackageMetadata {
    /// The metadata came from an index, and may be accompanied by a list of distributions from
    /// `--find-links` that should override the index.
    Simple(IndexUrl, SimpleMetadata, Option<FlatDistributions>),
    /// The metadata wasn't present in the index, but was found in `--find-links`.
    FindLinks(FlatDistributions),
}

type PackageMetadataResponse = Result<PackageMetadata, puffin_client::Error>;
type WheelMetadataResponse = Result<(Metadata21, Option<Url>), DistributionDatabaseError>;

pub trait ResolverProvider: Send + Sync {
    /// Get the metadata for a package.
    fn get_package_metadata<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> impl Future<Output = PackageMetadataResponse> + Send + 'io;

    /// Get the metadata for a distribution.
    ///
    /// For a wheel, we query its (remote) metadata. For a source distribution, we fetch and build
    /// the distribution, then return the metadata from the built wheeel.
    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> impl Future<Output = WheelMetadataResponse> + Send + 'io;

    /// Set the [`puffin_distribution::Reporter`]..
    #[must_use]
    fn with_reporter(self, reporter: impl puffin_distribution::Reporter + 'static) -> Self;
}

/// The main IO backend for the resolver, which does cached requests network requests using the
/// [`RegistryClient`] and [`DistributionDatabase`].
pub struct DefaultResolverProvider<'a, Context: BuildContext + Send + Sync> {
    /// The [`RegistryClient`] used to query the index.
    client: &'a RegistryClient,
    /// The [`DistributionDatabase`] used to build source distributions.
    fetcher: DistributionDatabase<'a, Context>,
    /// These are the entries from `--find-links` that act as overrides for index responses.
    flat_index: &'a FlatIndex,
}

impl<'a, Context: BuildContext + Send + Sync> DefaultResolverProvider<'a, Context> {
    pub fn new(
        client: &'a RegistryClient,
        fetcher: DistributionDatabase<'a, Context>,
        flat_index: &'a FlatIndex,
    ) -> Self {
        Self {
            client,
            fetcher,
            flat_index,
        }
    }
}

impl<'a, Context: BuildContext + Send + Sync> ResolverProvider
    for DefaultResolverProvider<'a, Context>
{
    fn get_package_metadata<'io>(
        &'io self,
        package_name: &'io PackageName,
    ) -> impl Future<Output = PackageMetadataResponse> + Send + 'io {
        self.client
            .simple(package_name)
            .map(move |result| match result {
                Ok((index, metadata)) => Ok(PackageMetadata::Simple(
                    index,
                    metadata,
                    self.flat_index.get(package_name).cloned(),
                )),
                Err(
                    err @ (puffin_client::Error::PackageNotFound(_)
                    | puffin_client::Error::NoIndex(_)),
                ) => {
                    if let Some(distributions) = self.flat_index.get(package_name).cloned() {
                        Ok(PackageMetadata::FindLinks(distributions))
                    } else {
                        Err(err)
                    }
                }
                Err(err) => Err(err),
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
