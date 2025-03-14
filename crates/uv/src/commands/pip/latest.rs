use tokio::sync::Semaphore;
use tracing::debug;
use uv_client::{RegistryClient, VersionFiles};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{IndexCapabilities, IndexUrl};
use uv_normalize::PackageName;
use uv_platform_tags::Tags;
use uv_resolver::{ExcludeNewer, PrereleaseMode, RequiresPython};
use uv_warnings::warn_user_once;

/// A client to fetch the latest version of a package from an index.
///
/// The returned distribution is guaranteed to be compatible with the provided tags and Python
/// requirement.
#[derive(Debug, Copy, Clone)]
pub(crate) struct LatestClient<'env> {
    pub(crate) client: &'env RegistryClient,
    pub(crate) capabilities: &'env IndexCapabilities,
    pub(crate) prerelease: PrereleaseMode,
    pub(crate) exclude_newer: Option<ExcludeNewer>,
    pub(crate) tags: Option<&'env Tags>,
    pub(crate) requires_python: &'env RequiresPython,
}

impl LatestClient<'_> {
    /// Find the latest version of a package from an index.
    pub(crate) async fn find_latest(
        &self,
        package: &PackageName,
        index: Option<&IndexUrl>,
        download_concurrency: &Semaphore,
    ) -> anyhow::Result<Option<DistFilename>, uv_client::Error> {
        debug!("Fetching latest version of: `{package}`");

        let archives = match self
            .client
            .simple(package, index, self.capabilities, download_concurrency)
            .await
        {
            Ok(archives) => archives,
            Err(err) => {
                return match err.into_kind() {
                    uv_client::ErrorKind::PackageNotFound(_) => Ok(None),
                    uv_client::ErrorKind::NoIndex(_) => Ok(None),
                    uv_client::ErrorKind::Offline(_) => Ok(None),
                    kind => Err(kind.into()),
                }
            }
        };

        let mut latest: Option<DistFilename> = None;
        for (_, archive) in archives {
            for datum in archive.iter().rev() {
                // Find the first compatible distribution.
                let files = rkyv::deserialize::<VersionFiles, rkyv::rancor::Error>(&datum.files)
                    .expect("archived version files always deserializes");

                // Determine whether there's a compatible wheel and/or source distribution.
                let mut best = None;

                for (filename, file) in files.all() {
                    // Skip distributions uploaded after the cutoff.
                    if let Some(exclude_newer) = self.exclude_newer {
                        match file.upload_time_utc_ms.as_ref() {
                            Some(&upload_time)
                                if upload_time >= exclude_newer.timestamp_millis() =>
                            {
                                continue;
                            }
                            None => {
                                warn_user_once!(
                                    "{} is missing an upload date, but user provided: {exclude_newer}",
                                    file.filename,
                                );
                            }
                            _ => {}
                        }
                    }

                    // Skip pre-release distributions.
                    if !filename.version().is_stable() {
                        if !matches!(self.prerelease, PrereleaseMode::Allow) {
                            continue;
                        }
                    }

                    // Skip distributions that are yanked.
                    if file.yanked.is_some_and(|yanked| yanked.is_yanked()) {
                        continue;
                    }

                    // Skip distributions that are incompatible with the Python requirement.
                    if file
                        .requires_python
                        .as_ref()
                        .is_some_and(|requires_python| {
                            !self.requires_python.is_contained_by(requires_python)
                        })
                    {
                        continue;
                    }

                    // Skip distributions that are incompatible with the current platform.
                    if let DistFilename::WheelFilename(filename) = &filename {
                        if self
                            .tags
                            .is_some_and(|tags| !filename.compatibility(tags).is_compatible())
                        {
                            continue;
                        }
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

                match (latest.as_ref(), best) {
                    (Some(current), Some(best)) => {
                        if best.version() > current.version() {
                            latest = Some(best);
                        }
                    }
                    (None, Some(best)) => {
                        latest = Some(best);
                    }
                    _ => {}
                }
            }
        }
        Ok(latest)
    }
}
