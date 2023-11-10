use std::collections::hash_map::RandomState;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{StreamExt, TryFutureExt};
use fxhash::FxHashSet;
use tracing::{debug, error, info};
use url::Url;
use waitmap::{Ref, WaitMap};

use distribution_filename::{SourceDistFilename, WheelFilename};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::{
    BuiltDist, DirectUrlSourceDist, Dist, GitSourceDist, Identifier, Metadata, SourceDist,
};
use puffin_normalize::PackageName;
use puffin_traits::BuildContext;
use pypi_types::{Metadata21, SimpleJson};

use crate::distribution::{BuiltDistFetcher, SourceDistFetcher};
use crate::file::{DistFile, SdistFile, WheelFile};
use crate::locks::Locks;
use crate::ResolveError;

pub(crate) type VersionMap = BTreeMap<Version, DistFile>;

pub(crate) struct Database<Context: BuildContext> {
    client: RegistryClient,
    tags: Tags,
    build_context: Context,
    index: Arc<Index>,
    locks: Arc<Locks>,
    in_flight: Arc<Mutex<InFlight>>,
}

impl<Context: BuildContext> Database<Context> {
    pub(crate) fn new(tags: Tags, client: RegistryClient, build_context: Context) -> Self {
        Self {
            client,
            tags,
            build_context,
            index: Arc::default(),
            locks: Arc::default(),
            in_flight: Arc::default(),
        }
    }

    pub(crate) async fn listen(
        &self,
        receiver: UnboundedReceiver<Request>,
    ) -> Result<(), ResolveError> {
        self.fetch(receiver).await
    }

    /// Emit a request to fetch the metadata for a registry-based package.
    pub(crate) fn fetch_package(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
    ) -> Result<bool, ResolveError> {
        Ok(
            if self.in_flight.lock().unwrap().insert_package(package_name) {
                sender.unbounded_send(Request::Package(package_name.clone()))?;
                true
            } else {
                false
            },
        )
    }

    /// Emit a request to fetch the metadata for a direct URL-based package.
    pub(crate) fn fetch_url(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
        url: &Url,
    ) -> Result<bool, ResolveError> {
        Ok(if self.in_flight.lock().unwrap().insert_url(url) {
            sender.unbounded_send(Request::Dist(Dist::from_url(
                package_name.clone(),
                url.clone(),
            )))?;
            true
        } else {
            false
        })
    }

    /// Emit a request to fetch the metadata for an individual distribution.
    pub(crate) fn fetch_file(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
        version: &Version,
        file: &DistFile,
    ) -> Result<bool, ResolveError> {
        Ok(if self.in_flight.lock().unwrap().insert_file(file) {
            let distribution =
                Dist::from_registry(package_name.clone(), version.clone(), file.clone().into());
            sender.unbounded_send(Request::Dist(distribution))?;
            true
        } else {
            false
        })
    }

    pub(crate) fn get_package(
        &self,
        package_name: &PackageName,
    ) -> Option<Ref<PackageName, VersionMap, RandomState>> {
        self.index.packages.get(package_name)
    }

    pub(crate) async fn wait_package(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
    ) -> Ref<PackageName, VersionMap, RandomState> {
        self.fetch_package(sender, package_name)
            .expect("Failed to emit request");
        self.index.packages.wait(package_name).await.unwrap()
    }

    pub(crate) async fn wait_url(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
        url: &Url,
    ) -> Ref<String, Metadata21, RandomState> {
        self.fetch_url(sender, package_name, url)
            .expect("Failed to emit request");
        self.index
            .distributions
            .wait(&url.distribution_id())
            .await
            .unwrap()
    }

    pub(crate) async fn wait_file(
        &self,
        sender: &UnboundedSender<Request>,
        package_name: &PackageName,
        version: &Version,
        file: &DistFile,
    ) -> Ref<String, Metadata21, RandomState> {
        self.fetch_file(sender, package_name, version, file)
            .expect("Failed to emit request");
        self.index
            .distributions
            .wait(&file.distribution_id())
            .await
            .unwrap()
    }

    /// Fetch the metadata for a stream of packages and versions.
    async fn fetch(&self, request_stream: UnboundedReceiver<Request>) -> Result<(), ResolveError> {
        let mut response_stream = request_stream
            .map(|request| self.process_request(request))
            .buffer_unordered(50);

        while let Some(response) = response_stream.next().await {
            match response? {
                Response::Package(package_name, metadata) => {
                    info!("Received package metadata for: {package_name}");

                    // Group the distributions by version and kind, discarding any incompatible
                    // distributions.
                    let mut version_map: VersionMap = BTreeMap::new();
                    for file in metadata.files {
                        if let Ok(filename) = WheelFilename::from_str(file.filename.as_str()) {
                            if filename.is_compatible(&self.tags) {
                                match version_map.entry(filename.version) {
                                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                                        if matches!(entry.get(), DistFile::Sdist(_)) {
                                            // Wheels get precedence over source distributions.
                                            entry.insert(DistFile::from(WheelFile(file)));
                                        }
                                    }
                                    std::collections::btree_map::Entry::Vacant(entry) => {
                                        entry.insert(DistFile::from(WheelFile(file)));
                                    }
                                }
                            }
                        } else if let Ok(filename) =
                            SourceDistFilename::parse(file.filename.as_str(), &package_name)
                        {
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                version_map.entry(filename.version)
                            {
                                entry.insert(DistFile::from(SdistFile(file)));
                            }
                        }
                    }

                    self.index
                        .packages
                        .insert(package_name.clone(), version_map);
                }
                Response::Dist(Dist::Built(distribution), metadata, ..) => {
                    info!("Received built distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .insert(distribution.distribution_id(), metadata);
                }
                Response::Dist(Dist::Source(distribution), metadata, precise) => {
                    info!("Received source distribution metadata for: {distribution}");
                    self.index
                        .distributions
                        .insert(distribution.distribution_id(), metadata);
                    if let Some(precise) = precise {
                        match distribution {
                            SourceDist::DirectUrl(sdist) => {
                                self.index.redirects.insert(sdist.url.clone(), precise);
                            }
                            SourceDist::Git(sdist) => {
                                self.index.redirects.insert(sdist.url.clone(), precise);
                            }
                            SourceDist::Registry(_) => {}
                        }
                    }
                }
            }
        }

        Ok::<(), ResolveError>(())
    }

    async fn process_request(&self, request: Request) -> Result<Response, ResolveError> {
        match request {
            // Fetch package metadata from the registry.
            Request::Package(package_name) => {
                info!("Fetching package metadata for: {package_name}");

                self.client
                    .simple(package_name.clone())
                    .map_ok(move |metadata| Response::Package(package_name, metadata))
                    .map_err(ResolveError::Client)
                    .await
            }

            // Fetch wheel metadata.
            Request::Dist(Dist::Built(distribution)) => {
                info!("Fetching built distribution metadata for: {distribution}");

                let metadata = match &distribution {
                    BuiltDist::Registry(wheel) => {
                        self.client
                            .wheel_metadata(wheel.file.clone())
                            .map_err(ResolveError::Client)
                            .await?
                    }
                    BuiltDist::DirectUrl(wheel) => {
                        let fetcher = BuiltDistFetcher::new(self.build_context.cache());
                        match fetcher.find_dist_info(wheel, &self.tags) {
                            Ok(Some(metadata)) => {
                                debug!("Found wheel metadata in cache: {wheel}");
                                metadata
                            }
                            Ok(None) => {
                                debug!("Downloading wheel: {wheel}");
                                fetcher.download_wheel(wheel, &self.client).await.map_err(
                                    |err| ResolveError::from_built_dist(distribution.clone(), err),
                                )?
                            }
                            Err(err) => {
                                error!("Failed to read wheel from cache: {err}");
                                fetcher.download_wheel(wheel, &self.client).await.map_err(
                                    |err| ResolveError::from_built_dist(distribution.clone(), err),
                                )?
                            }
                        }
                    }
                };

                if metadata.name != *distribution.name() {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: distribution.name().clone(),
                    });
                }

                Ok(Response::Dist(Dist::Built(distribution), metadata, None))
            }

            // Fetch source distribution metadata.
            Request::Dist(Dist::Source(sdist)) => {
                info!("Fetching source distribution metadata for: {sdist}");

                let lock = self.locks.acquire(&sdist).await;
                let _guard = lock.lock().await;

                let fetcher = SourceDistFetcher::new(&self.build_context);

                let precise = fetcher
                    .precise(&sdist)
                    .await
                    .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?;

                let metadata = {
                    // Insert the `precise`, if it exists.
                    let sdist = match sdist.clone() {
                        SourceDist::DirectUrl(sdist) => {
                            SourceDist::DirectUrl(DirectUrlSourceDist {
                                url: precise.clone().unwrap_or_else(|| sdist.url.clone()),
                                ..sdist
                            })
                        }
                        SourceDist::Git(sdist) => SourceDist::Git(GitSourceDist {
                            url: precise.clone().unwrap_or_else(|| sdist.url.clone()),
                            ..sdist
                        }),
                        sdist @ SourceDist::Registry(_) => sdist,
                    };

                    match fetcher.find_dist_info(&sdist, &self.tags) {
                        Ok(Some(metadata)) => {
                            debug!("Found source distribution metadata in cache: {sdist}");
                            metadata
                        }
                        Ok(None) => {
                            debug!("Downloading source distribution: {sdist}");
                            fetcher
                                .download_and_build_sdist(&sdist, &self.client)
                                .await
                                .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?
                        }
                        Err(err) => {
                            error!("Failed to read source distribution from cache: {err}",);
                            fetcher
                                .download_and_build_sdist(&sdist, &self.client)
                                .await
                                .map_err(|err| ResolveError::from_source_dist(sdist.clone(), err))?
                        }
                    }
                };

                if metadata.name != *sdist.name() {
                    return Err(ResolveError::NameMismatch {
                        metadata: metadata.name,
                        given: sdist.name().clone(),
                    });
                }

                Ok(Response::Dist(Dist::Source(sdist), metadata, precise))
            }
        }
    }
}

/// Fetch the metadata for an item
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Request {
    /// A request to fetch the metadata for a package.
    Package(PackageName),
    /// A request to fetch the metadata for a built or source distribution.
    Dist(Dist),
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Response {
    /// The returned metadata for a package hosted on a registry.
    Package(PackageName, SimpleJson),
    /// The returned metadata for a distribution.
    Dist(Dist, Metadata21, Option<Url>),
}

/// In-memory index of in-flight network requests. Any request in an [`InFlight`] state will be
/// eventually be inserted into an [`Index`].
#[derive(Debug, Default)]
struct InFlight {
    /// The set of requested [`PackageName`]s.
    packages: FxHashSet<PackageName>,
    /// The set of requested registry-based files, represented by their SHAs.
    files: FxHashSet<String>,
    /// The set of requested URLs.
    urls: FxHashSet<Url>,
}

impl InFlight {
    fn insert_package(&mut self, package_name: &PackageName) -> bool {
        self.packages.insert(package_name.clone())
    }

    fn insert_file(&mut self, file: &DistFile) -> bool {
        match file {
            DistFile::Wheel(file) => self.files.insert(file.hashes.sha256.clone()),
            DistFile::Sdist(file) => self.files.insert(file.hashes.sha256.clone()),
        }
    }

    fn insert_url(&mut self, url: &Url) -> bool {
        self.urls.insert(url.clone())
    }
}

/// In-memory index of package metadata.
struct Index {
    /// A map from package name to the metadata for that package.
    packages: WaitMap<PackageName, VersionMap>,

    /// A map from distribution SHA to metadata for that distribution.
    distributions: WaitMap<String, Metadata21>,

    /// A map from source URL to precise URL.
    redirects: WaitMap<Url, Url>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            packages: WaitMap::new(),
            distributions: WaitMap::new(),
            redirects: WaitMap::new(),
        }
    }
}
