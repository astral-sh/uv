use tokio::sync::Semaphore;
use tracing::debug;

use uv_client::{FlatIndexEntry, MetadataFormat, RegistryClient, VersionFiles};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{File, IndexCapabilities, IndexMetadataRef, IndexUrl, RequiresPython};
use uv_normalize::PackageName;
use uv_platform_tags::Tags;
use uv_resolver::{ExcludeNewer, ExcludeNewerTimestamp, PrereleaseMode};
use uv_warnings::warn_user_once;

/// A client to fetch the latest version of a package from an index.
///
/// The returned distribution is guaranteed to be compatible with the provided tags and Python
/// requirement.
#[derive(Debug, Clone)]
pub(crate) struct LatestClient<'env> {
    pub(crate) client: &'env RegistryClient,
    pub(crate) capabilities: &'env IndexCapabilities,
    pub(crate) prerelease: PrereleaseMode,
    pub(crate) exclude_newer: &'env ExcludeNewer,
    pub(crate) tags: Option<&'env Tags>,
    pub(crate) requires_python: &'env RequiresPython,
}

impl LatestClient<'_> {
    fn consider_candidate(
        &self,
        filename: &DistFilename,
        file: &File,
        exclude_newer: Option<ExcludeNewerTimestamp>,
    ) -> bool {
        // Respect any exclude-newer cutoffs that were provided.
        if let Some(exclude_newer) = exclude_newer {
            match file.upload_time_utc_ms {
                Some(upload_time) if upload_time >= exclude_newer.timestamp_millis() => {
                    return false;
                }
                None => {
                    warn_user_once!(
                        "{} is missing an upload date, but user provided: {}",
                        file.filename,
                        self.exclude_newer
                    );
                }
                _ => {}
            }
        }

        // Unless explicitly allowed, skip pre-release artifacts.
        if !filename.version().is_stable() {
            if !matches!(self.prerelease, PrereleaseMode::Allow) {
                return false;
            }
        }

        // Avoid yanked or otherwise withdrawn files.
        if file
            .yanked
            .as_ref()
            .is_some_and(|yanked| yanked.is_yanked())
        {
            return false;
        }

        // Enforce the interpreter's `Requires-Python` constraints.
        if file
            .requires_python
            .as_ref()
            .is_some_and(|requires_python| !self.requires_python.is_contained_by(requires_python))
        {
            return false;
        }

        // Skip wheels that aren't compatible with the current platform.
        if let DistFilename::WheelFilename(filename) = filename {
            if self
                .tags
                .is_some_and(|tags| !filename.compatibility(tags).is_compatible())
            {
                return false;
            }
        }

        true
    }

    /// Find the latest version of a package from an index.
    pub(crate) async fn find_latest(
        &self,
        package: &PackageName,
        index: Option<&IndexUrl>,
        download_concurrency: &Semaphore,
    ) -> anyhow::Result<Option<DistFilename>, uv_client::Error> {
        debug!("Fetching latest version of: `{package}`");

        let exclude_newer = self.exclude_newer.exclude_newer_package(package);

        let mut latest: Option<DistFilename> = None;

        let mut update_latest = |candidate: DistFilename| {
            match latest.as_ref() {
                Some(current) => {
                    // Prefer higher versions, and prefer wheels over sdists at parity.
                    if candidate.version() > current.version()
                        || (candidate.version() == current.version()
                            && matches!(candidate, DistFilename::WheelFilename(_))
                            && matches!(current, DistFilename::SourceDistFilename(_)))
                    {
                        latest = Some(candidate);
                    }
                }
                None => {
                    latest = Some(candidate);
                }
            }
        };

        let archives = match self
            .client
            .simple_detail(
                package,
                index.map(IndexMetadataRef::from),
                self.capabilities,
                download_concurrency,
            )
            .await
        {
            Ok(archives) => archives,
            Err(err) => {
                return match err.kind() {
                    uv_client::ErrorKind::RemotePackageNotFound(_) => Ok(None),
                    uv_client::ErrorKind::NoIndex(_) => Ok(None),
                    uv_client::ErrorKind::Offline(_) => Ok(None),
                    _ => Err(err),
                };
            }
        };

        for (_, archive) in archives {
            match archive {
                MetadataFormat::Simple(archive) => {
                    for datum in archive.iter().rev() {
                        // Walk the available files, looking for the best (wheel-first) match.
                        let files =
                            rkyv::deserialize::<VersionFiles, rkyv::rancor::Error>(&datum.files)
                                .expect("archived version files always deserializes");

                        let mut best: Option<DistFilename> = None;
                        for (filename, file) in files.all() {
                            if !self.consider_candidate(&filename, &file, exclude_newer) {
                                continue;
                            }

                            match filename {
                                DistFilename::WheelFilename(_) => {
                                    best = Some(filename);
                                    break;
                                }
                                DistFilename::SourceDistFilename(_) => {
                                    if best.is_none() {
                                        best = Some(filename);
                                    }
                                }
                            }
                        }

                        if let Some(best) = best {
                            update_latest(best);
                        }
                    }
                }
                MetadataFormat::Flat(entries) => {
                    // Flat indexes are already de-duplicated; keep the best matching artifact.
                    for FlatIndexEntry { filename, file, .. } in entries {
                        if !self.consider_candidate(&filename, &file, exclude_newer) {
                            continue;
                        }

                        update_latest(filename);
                    }
                }
            }
        }

        Ok(latest)
    }
}
