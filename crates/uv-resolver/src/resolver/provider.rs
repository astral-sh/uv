use std::future::Future;
use std::sync::Arc;
use uv_client::MetadataFormat;
use uv_configuration::BuildOptions;
use uv_distribution::{ArchiveMetadata, DistributionDatabase, Reporter};
use uv_distribution_types::{
    Dist, IndexCapabilities, IndexMetadata, IndexMetadataRef, InstalledDist, RegistryVariantsJson,
    RequestedDist, RequiresPython,
};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::Tags;
use uv_types::{BuildContext, HashStrategy};
use uv_variants::resolved_variants::ResolvedVariants;

use crate::ExcludeNewer;
use crate::flat_index::FlatIndex;
use crate::version_map::VersionMap;
use crate::yanks::AllowedYanks;

pub type PackageVersionsResult = Result<VersionsResponse, uv_client::Error>;
pub type WheelMetadataResult = Result<MetadataResponse, uv_distribution::Error>;
pub type VariantProviderResult = Result<ResolvedVariants, uv_distribution::Error>;

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
    /// A non-fatal error.
    Unavailable(MetadataUnavailable),
    /// The distribution could not be built or downloaded, a fatal error.
    Error(Box<RequestedDist>, Arc<uv_distribution::Error>),
}

/// Non-fatal metadata fetching error.
///
/// This is also the unavailability reasons for a package, while version unavailability is separate
/// in [`UnavailableVersion`].
#[derive(Debug, Clone)]
pub enum MetadataUnavailable {
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata(Arc<uv_pypi_types::MetadataError>),
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(Arc<uv_distribution::Error>),
    /// The wheel has an invalid structure.
    InvalidStructure(Arc<uv_metadata::Error>),
    /// The source distribution has a `requires-python` requirement that is not met by the installed
    /// Python version (and static metadata is not available).
    RequiresPython(VersionSpecifiers, Version),
}

impl MetadataUnavailable {
    /// Like [`std::error::Error::source`], but we don't want to derive the std error since our
    /// formatting system is more custom.
    pub(crate) fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Offline => None,
            Self::InvalidMetadata(err) => Some(err),
            Self::InconsistentMetadata(err) => Some(err),
            Self::InvalidStructure(err) => Some(err),
            Self::RequiresPython(_, _) => None,
        }
    }
}

pub trait ResolverProvider {
    /// Get the version map for a package.
    fn get_package_versions<'io>(
        &'io self,
        package_name: &'io PackageName,
        index: Option<&'io IndexMetadata>,
    ) -> impl Future<Output = PackageVersionsResult> + 'io;

    /// Get the metadata for a distribution.
    ///
    /// For a wheel, this is done by querying it (remote) metadata. For a source distribution, we
    /// (fetch and) build the source distribution and return the metadata from the built
    /// distribution.
    fn get_or_build_wheel_metadata<'io>(
        &'io self,
        dist: &'io Dist,
    ) -> impl Future<Output = WheelMetadataResult> + 'io;

    /// Get the metadata for an installed distribution.
    fn get_installed_metadata<'io>(
        &'io self,
        dist: &'io InstalledDist,
    ) -> impl Future<Output = WheelMetadataResult> + 'io;

    /// Fetch the variants for a distribution given the marker environment.
    fn fetch_and_query_variants<'io>(
        &'io self,
        variants_json: &'io RegistryVariantsJson,
        marker_env: &'io uv_pep508::MarkerEnvironment,
    ) -> impl Future<Output = VariantProviderResult> + 'io;

    /// Set the [`Reporter`] to use for this installer.
    #[must_use]
    fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self;
}

/// The main IO backend for the resolver, which does cached requests network requests using the
/// [`RegistryClient`] and [`DistributionDatabase`].
pub struct DefaultResolverProvider<'a, Context: BuildContext> {
    /// The [`DistributionDatabase`] used to build source distributions.
    fetcher: DistributionDatabase<'a, Context>,
    /// These are the entries from `--find-links` that act as overrides for index responses.
    flat_index: FlatIndex,
    tags: Option<Tags>,
    requires_python: RequiresPython,
    allowed_yanks: AllowedYanks,
    hasher: HashStrategy,
    exclude_newer: ExcludeNewer,
    build_options: &'a BuildOptions,
    capabilities: &'a IndexCapabilities,
}

impl<'a, Context: BuildContext> DefaultResolverProvider<'a, Context> {
    /// Reads the flat index entries and builds the provider.
    pub fn new(
        fetcher: DistributionDatabase<'a, Context>,
        flat_index: &'a FlatIndex,
        tags: Option<&'a Tags>,
        requires_python: &'a RequiresPython,
        allowed_yanks: AllowedYanks,
        hasher: &'a HashStrategy,
        exclude_newer: ExcludeNewer,
        build_options: &'a BuildOptions,
        capabilities: &'a IndexCapabilities,
    ) -> Self {
        Self {
            fetcher,
            flat_index: flat_index.clone(),
            tags: tags.cloned(),
            requires_python: requires_python.clone(),
            allowed_yanks,
            hasher: hasher.clone(),
            exclude_newer,
            build_options,
            capabilities,
        }
    }
}

impl<Context: BuildContext> ResolverProvider for DefaultResolverProvider<'_, Context> {
    /// Make a "Simple API" request for the package and convert the result to a [`VersionMap`].
    async fn get_package_versions<'io>(
        &'io self,
        package_name: &'io PackageName,
        index: Option<&'io IndexMetadata>,
    ) -> PackageVersionsResult {
        let result = self
            .fetcher
            .client()
            .manual(|client, semaphore| {
                client.package_metadata(
                    package_name,
                    index.map(IndexMetadataRef::from),
                    self.capabilities,
                    semaphore,
                )
            })
            .await;

        // If a package is pinned to an explicit index, ignore any `--find-links` entries.
        let flat_index = index.is_none().then_some(&self.flat_index);

        match result {
            Ok(results) => Ok(VersionsResponse::Found(
                results
                    .into_iter()
                    .map(|(index, metadata)| match metadata {
                        MetadataFormat::Simple(metadata) => VersionMap::from_simple_metadata(
                            metadata,
                            package_name,
                            index,
                            self.tags.as_ref(),
                            &self.requires_python,
                            &self.allowed_yanks,
                            &self.hasher,
                            Some(&self.exclude_newer),
                            flat_index
                                .and_then(|flat_index| flat_index.get(package_name))
                                .cloned(),
                            self.build_options,
                        ),
                        MetadataFormat::Flat(metadata) => VersionMap::from_flat_metadata(
                            metadata,
                            self.tags.as_ref(),
                            &self.hasher,
                            self.build_options,
                        ),
                    })
                    .collect(),
            )),
            Err(err) => match err.kind() {
                uv_client::ErrorKind::PackageNotFound(_) => {
                    if let Some(flat_index) = flat_index
                        .and_then(|flat_index| flat_index.get(package_name))
                        .cloned()
                    {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else {
                        Ok(VersionsResponse::NotFound)
                    }
                }
                uv_client::ErrorKind::NoIndex(_) => {
                    if let Some(flat_index) = flat_index
                        .and_then(|flat_index| flat_index.get(package_name))
                        .cloned()
                    {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else if flat_index.is_some_and(FlatIndex::offline) {
                        Ok(VersionsResponse::Offline)
                    } else {
                        Ok(VersionsResponse::NoIndex)
                    }
                }
                uv_client::ErrorKind::Offline(_) => {
                    if let Some(flat_index) = flat_index
                        .and_then(|flat_index| flat_index.get(package_name))
                        .cloned()
                    {
                        Ok(VersionsResponse::Found(vec![VersionMap::from(flat_index)]))
                    } else {
                        Ok(VersionsResponse::Offline)
                    }
                }
                _ => Err(err),
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
                uv_distribution::Error::Client(client) => {
                    let retries = client.retries();
                    match client.into_kind() {
                        uv_client::ErrorKind::Offline(_) => {
                            Ok(MetadataResponse::Unavailable(MetadataUnavailable::Offline))
                        }
                        uv_client::ErrorKind::MetadataParseError(_, _, err) => {
                            Ok(MetadataResponse::Unavailable(
                                MetadataUnavailable::InvalidMetadata(Arc::new(*err)),
                            ))
                        }
                        uv_client::ErrorKind::Metadata(_, err) => {
                            Ok(MetadataResponse::Unavailable(
                                MetadataUnavailable::InvalidStructure(Arc::new(err)),
                            ))
                        }
                        kind => Err(uv_client::Error::new(kind, retries).into()),
                    }
                }
                uv_distribution::Error::WheelMetadataVersionMismatch { .. } => {
                    Ok(MetadataResponse::Unavailable(
                        MetadataUnavailable::InconsistentMetadata(Arc::new(err)),
                    ))
                }
                uv_distribution::Error::WheelMetadataNameMismatch { .. } => {
                    Ok(MetadataResponse::Unavailable(
                        MetadataUnavailable::InconsistentMetadata(Arc::new(err)),
                    ))
                }
                uv_distribution::Error::Metadata(err) => Ok(MetadataResponse::Unavailable(
                    MetadataUnavailable::InvalidMetadata(Arc::new(err)),
                )),
                uv_distribution::Error::WheelMetadata(_, err) => Ok(MetadataResponse::Unavailable(
                    MetadataUnavailable::InvalidStructure(Arc::new(*err)),
                )),
                uv_distribution::Error::RequiresPython(requires_python, version) => {
                    Ok(MetadataResponse::Unavailable(
                        MetadataUnavailable::RequiresPython(requires_python, version),
                    ))
                }
                err => Ok(MetadataResponse::Error(
                    Box::new(RequestedDist::Installable(dist.clone())),
                    Arc::new(err),
                )),
            },
        }
    }

    /// Return the metadata for an installed distribution.
    async fn get_installed_metadata<'io>(
        &'io self,
        dist: &'io InstalledDist,
    ) -> WheelMetadataResult {
        match self.fetcher.get_installed_metadata(dist).await {
            Ok(metadata) => Ok(MetadataResponse::Found(metadata)),
            Err(err) => Ok(MetadataResponse::Error(
                Box::new(RequestedDist::Installed(dist.clone())),
                Arc::new(err),
            )),
        }
    }

    /// Fetch the variants for a distribution given the marker environment.
    async fn fetch_and_query_variants<'io>(
        &'io self,
        variants_json: &'io RegistryVariantsJson,
        marker_env: &'io uv_pep508::MarkerEnvironment,
    ) -> VariantProviderResult {
        self.fetcher
            .fetch_and_query_variants(variants_json, marker_env)
            .await
    }

    /// Set the [`Reporter`] to use for this installer.
    fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            fetcher: self.fetcher.with_reporter(reporter),
            ..self
        }
    }
}
