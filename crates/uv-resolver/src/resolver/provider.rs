use std::future::Future;
use std::sync::Arc;

use reqwest::StatusCode;

use uv_client::{IndexMetadataResponse, MetadataFormat};
use uv_configuration::BuildOptions;
use uv_distribution::{ArchiveMetadata, DistributionDatabase, Reporter};
use uv_distribution_types::{
    BuiltDist, Dist, IndexCapabilities, IndexLocations, IndexMetadata, IndexMetadataRef,
    IndexRoutes, InstalledDist, RequestedDist, RequiresPython, SourceDist,
};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::Tags;
use uv_static::EnvVars;
use uv_types::{BuildContext, HashStrategy};

use crate::ExcludeNewer;
use crate::flat_index::FlatIndex;
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
    /// The wheel metadata could not be fetched due to a network error.
    Network(StatusCode),
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
            Self::RequiresPython(..) | Self::Network(..) => None,
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
    available_version_cutoff: Option<jiff::Timestamp>,
    index_locations: &'a IndexLocations,
    build_options: &'a BuildOptions,
    capabilities: &'a IndexCapabilities,
}

/// Return whether a failed metadata request should be ignored for the physical index that served
/// the artifact.
fn ignores_metadata_error(
    dist: &Dist,
    index_locations: &IndexLocations,
    index_routes: &IndexRoutes,
    status: StatusCode,
) -> bool {
    let physical_index = match dist {
        Dist::Built(BuiltDist::Registry(dist)) => {
            let wheel = dist.best_wheel();
            Some(
                wheel
                    .proxy
                    .as_ref()
                    .unwrap_or_else(|| index_routes.route_for(&wheel.index).physical),
            )
        }
        Dist::Source(SourceDist::Registry(dist)) => Some(
            dist.proxy
                .as_ref()
                .unwrap_or_else(|| index_routes.route_for(&dist.index).physical),
        ),
        Dist::Built(BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_))
        | Dist::Source(
            SourceDist::DirectUrl(_)
            | SourceDist::GitDirectory(_)
            | SourceDist::GitPath(_)
            | SourceDist::Path(_)
            | SourceDist::Directory(_),
        ) => None,
    };
    physical_index.is_some_and(|index| index_locations.ignores_error_code_for(index, status))
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
        index_locations: &'a IndexLocations,
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
            available_version_cutoff: std::env::var(EnvVars::UV_TEST_AVAILABLE_VERSION_CUTOFF)
                .ok()
                .and_then(|value| value.parse().ok()),
            index_locations,
            build_options,
            capabilities,
        }
    }

    fn effective_exclude_newer(
        &self,
        package_name: &PackageName,
        index: &uv_distribution_types::IndexUrl,
    ) -> Option<jiff::Timestamp> {
        self.exclude_newer.exclude_newer_package_for_index(
            package_name,
            self.index_locations.exclude_newer_for(index),
        )
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
                client.simple_detail(
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
                    .map(|response| {
                        let IndexMetadataResponse {
                            index_route,
                            format,
                        } = response;
                        let included_version_cutoff =
                            self.effective_exclude_newer(package_name, index_route.canonical);
                        let available_version_cutoff = included_version_cutoff
                            .is_none()
                            .then_some(self.available_version_cutoff)
                            .flatten();

                        match format {
                            MetadataFormat::Simple(metadata) => VersionMap::from_simple_metadata(
                                metadata,
                                package_name,
                                index_route.canonical.clone(),
                                index_route.physical.clone(),
                                self.tags.clone(),
                                self.requires_python.clone(),
                                self.allowed_yanks.clone(),
                                self.hasher.clone(),
                                included_version_cutoff,
                                available_version_cutoff,
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
                        }
                    })
                    .collect(),
            )),
            Err(err) => match err.kind() {
                uv_client::ErrorKind::RemotePackageNotFound(_) => {
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
                    let duration = client.duration();
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
                        uv_client::ErrorKind::WrappedReqwestError(url, err) => {
                            let Some(status) = err.status().filter(|status| {
                                ignores_metadata_error(
                                    dist,
                                    self.index_locations,
                                    self.fetcher.client().unmanaged.index_routes(),
                                    *status,
                                )
                            }) else {
                                return Err(uv_client::Error::new(
                                    uv_client::ErrorKind::WrappedReqwestError(url, err),
                                    retries,
                                    duration,
                                )
                                .into());
                            };
                            Ok(MetadataResponse::Unavailable(MetadataUnavailable::Network(
                                status,
                            )))
                        }
                        kind => Err(uv_client::Error::new(kind, retries, duration).into()),
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

    /// Set the [`Reporter`] to use for this installer.
    fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            fetcher: self.fetcher.with_reporter(reporter),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_distribution_types::{
        File, FileLocation, Index, IndexReference, IndexUrl, ProxyIndex, RegistryBuiltDist,
        RegistryBuiltWheel, SerializableStatusCode,
    };
    use uv_pypi_types::HashDigests;
    use uv_small_str::SmallString;

    use super::*;

    #[test]
    fn ignored_metadata_error_uses_physical_proxy_index() -> Result<(), Box<dyn std::error::Error>>
    {
        let canonical = IndexUrl::from_str("https://canonical.example.com/simple")?;
        let proxy = IndexUrl::from_str("https://proxy.example.com/simple")?;
        let mut canonical_index = Index::from(canonical.clone());
        canonical_index.ignore_error_codes = Some(vec![serde_json::from_value::<
            SerializableStatusCode,
        >(serde_json::json!(403))?]);
        let mut proxy_index = Index::from(proxy.clone());
        proxy_index.ignore_error_codes = Some(vec![serde_json::from_value::<
            SerializableStatusCode,
        >(serde_json::json!(401))?]);
        let index_locations =
            IndexLocations::new(vec![canonical_index, proxy_index], Vec::new(), false)
                .with_proxy_indexes(vec![ProxyIndex {
                    index: IndexReference::Url(canonical.clone()),
                    url: proxy.clone(),
                }]);
        let index_routes = IndexRoutes::try_from(&index_locations)?;

        let filename = WheelFilename::from_str("example-1.0.0-py3-none-any.whl")?;
        let file_url =
            SmallString::from("https://files.example.com/packages/example-1.0.0-py3-none-any.whl");
        let dist = Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
            wheels: vec![RegistryBuiltWheel {
                filename: filename.clone(),
                file: Box::new(File {
                    dist_info_metadata: false,
                    filename: SmallString::from(filename.to_string()),
                    hashes: HashDigests::empty(),
                    requires_python: None,
                    size: None,
                    upload_time_utc_ms: None,
                    url: FileLocation::new(file_url.clone(), &file_url),
                    yanked: None,
                    zstd: None,
                }),
                index: canonical,
                proxy: Some(proxy),
            }],
            best_wheel_index: 0,
            sdist: None,
        }));
        let mut locked_dist = dist.clone();
        if let Dist::Built(BuiltDist::Registry(registry_dist)) = &mut locked_dist {
            for wheel in &mut registry_dist.wheels {
                wheel.proxy = None;
            }
        }

        assert!(ignores_metadata_error(
            &dist,
            &index_locations,
            &index_routes,
            StatusCode::UNAUTHORIZED
        ));
        assert!(!ignores_metadata_error(
            &dist,
            &index_locations,
            &index_routes,
            StatusCode::FORBIDDEN
        ));
        assert!(ignores_metadata_error(
            &locked_dist,
            &index_locations,
            &index_routes,
            StatusCode::UNAUTHORIZED
        ));
        assert!(!ignores_metadata_error(
            &locked_dist,
            &index_locations,
            &index_routes,
            StatusCode::FORBIDDEN
        ));
        Ok(())
    }
}
