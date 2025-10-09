use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use uv_distribution_types::{BuiltDist, Dist, InstalledDist, Name, ResolvedDist, SourceDist};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::{HashDigest, ResolutionMetadata};

/// A detailed installation report compatible with pip's `--report` format.
///
/// This report can be generated during `pip install` operations, particularly
/// when combined with `--dry-run` to resolve dependencies without installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    /// The version of the report format. Always "1" for the stable format.
    pub version: String,

    /// The version of uv that generated this report.
    pub pip_version: String,

    /// Array of packages that were (or would be) installed.
    pub install: Vec<InstallationReportItem>,

    /// Environment information describing where the report was generated.
    pub environment: BTreeMap<String, String>,
}

impl InstallReport {
    /// Create a new installation report with the current uv version.
    pub fn new(
        install: Vec<InstallationReportItem>,
        environment: BTreeMap<String, String>,
    ) -> Self {
        Self {
            version: "1".to_string(),
            pip_version: env!("CARGO_PKG_VERSION").to_string(),
            install,
            environment,
        }
    }
}

/// Information about a single package installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationReportItem {
    /// Package metadata.
    pub metadata: InstallationMetadata,

    /// Whether this is a direct dependency (as opposed to transitive).
    #[serde(default)]
    pub is_direct: bool,

    /// Whether the package version is yanked from the index.
    #[serde(default)]
    pub is_yanked: bool,

    /// Information about how/where the package was downloaded from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_info: Option<DownloadInfo>,

    /// Whether this package was explicitly requested by the user.
    #[serde(default)]
    pub requested: bool,

    /// List of extras that were requested for this package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_extras: Option<Vec<String>>,
}

/// Package metadata included in the installation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationMetadata {
    /// The package name.
    pub name: String,

    /// The package version.
    pub version: String,

    /// List of runtime dependencies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_dist: Option<Vec<String>>,

    /// Required Python version specifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_python: Option<String>,

    /// List of extras provided by this package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provides_extra: Option<Vec<String>>,
}

/// Information about where and how a package was downloaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    /// The URL from which the package was downloaded.
    pub url: String,

    /// Information about the downloaded archive (for wheels and sdists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_info: Option<ArchiveInfo>,

    /// Information about VCS sources (for Git dependencies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcs_info: Option<VcsInfo>,
}

/// Information about a downloaded archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    /// Primary hash in the format "algorithm=digest" (for pip compatibility).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,

    /// Hash digests for the downloaded archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<BTreeMap<String, String>>,
}

impl ArchiveInfo {
    /// Create archive info from a hash digest.
    pub fn from_hash(hash: &HashDigest) -> Self {
        let mut hashes = BTreeMap::new();
        hashes.insert(
            hash.algorithm.to_string(),
            hash.digest.to_string(),
        );
        let hash_string = format!("{}={}", hash.algorithm, hash.digest);
        Self {
            hash: Some(hash_string),
            hashes: if hashes.is_empty() {
                None
            } else {
                Some(hashes)
            },
        }
    }

    /// Create archive info from multiple hash digests.
    pub fn from_hashes(hash_list: &[HashDigest]) -> Self {
        let mut hashes = BTreeMap::new();
        let mut primary_hash = None;
        for (idx, hash) in hash_list.iter().enumerate() {
            hashes.insert(
                hash.algorithm.to_string(),
                hash.digest.to_string(),
            );
            // Use the first hash as the primary hash
            if idx == 0 {
                primary_hash = Some(format!("{}={}", hash.algorithm, hash.digest));
            }
        }
        Self {
            hash: primary_hash,
            hashes: if hashes.is_empty() {
                None
            } else {
                Some(hashes)
            },
        }
    }
}

/// Information about a VCS (version control system) source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsInfo {
    /// The type of VCS (e.g., "git").
    pub vcs: String,

    /// The requested revision (e.g., branch name, tag).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_revision: Option<String>,

    /// The actual commit ID that was resolved.
    pub commit_id: String,
}

impl InstallationReportItem {
    /// Create a report item from an installed distribution.
    pub fn from_installed_dist(
        dist: &InstalledDist,
        is_direct: bool,
        requested: bool,
        requested_extras: Option<Vec<String>>,
    ) -> Self {
        let metadata = InstallationMetadata {
            name: dist.name().to_string(),
            version: dist.version().to_string(),
            requires_dist: dist
                .read_metadata()
                .ok()
                .and_then(|m| {
                    let deps: Vec<_> = m
                        .requires_dist
                        .iter()
                        .map(|req| req.to_string())
                        .collect();
                    if deps.is_empty() {
                        None
                    } else {
                        Some(deps)
                    }
                }),
            requires_python: dist
                .read_metadata()
                .ok()
                .and_then(|m| m.requires_python.as_ref().map(|r| r.to_string())),
            provides_extra: dist
                .read_metadata()
                .ok()
                .and_then(|m| {
                    let extras: Vec<_> = m
                        .provides_extra
                        .iter()
                        .map(|e| e.to_string())
                        .collect();
                    if extras.is_empty() {
                        None
                    } else {
                        Some(extras)
                    }
                }),
        };

        Self {
            metadata,
            is_direct,
            is_yanked: false, // TODO: Track yanked status during resolution
            download_info: None, // TODO: Populate from distribution source
            requested,
            requested_extras,
        }
    }

    /// Create a report item from a resolved distribution with hashes.
    pub fn from_resolved_dist(
        dist: &ResolvedDist,
        hashes: &[HashDigest],
        metadata: Option<&ResolutionMetadata>,
        is_direct: bool,
        requested: bool,
    ) -> Option<Self> {
        let version = dist.version()?;
        let name = dist.name();

        // Determine if this is a yanked version
        let is_yanked = dist.yanked().map(|y| y.is_yanked()).unwrap_or(false);

        // Extract download information from the distribution
        let download_info = match dist {
            ResolvedDist::Installable { dist: inner_dist, .. } => {
                Self::extract_download_info(inner_dist.as_ref(), hashes)
            }
            ResolvedDist::Installed { .. } => None,
        };

        // Extract dependency metadata if available
        let (requires_dist, requires_python, provides_extra) = if let Some(meta) = metadata {
            let deps: Vec<_> = meta
                .requires_dist
                .iter()
                .map(|req| req.to_string())
                .collect();
            let requires_dist = if deps.is_empty() { None } else { Some(deps) };

            let requires_python = meta.requires_python.as_ref().map(|r| r.to_string());

            let extras: Vec<_> = meta
                .provides_extra
                .iter()
                .map(|e| e.to_string())
                .collect();
            let provides_extra = if extras.is_empty() {
                None
            } else {
                Some(extras)
            };

            (requires_dist, requires_python, provides_extra)
        } else {
            (None, None, None)
        };

        Some(Self {
            metadata: InstallationMetadata {
                name: name.to_string(),
                version: version.to_string(),
                requires_dist,
                requires_python,
                provides_extra,
            },
            is_direct,
            is_yanked,
            download_info,
            requested,
            requested_extras: None,
        })
    }

    /// Extract download information from a distribution.
    fn extract_download_info(dist: &Dist, hashes: &[HashDigest]) -> Option<DownloadInfo> {
        match dist {
            Dist::Built(BuiltDist::Registry(wheel)) => {
                let best = wheel.best_wheel();
                let url = best.file.url.to_string();
                // Use hashes from the file itself, not from the Resolution
                let file_hashes = best.file.hashes.as_slice();
                let archive_info = if file_hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(file_hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                let url = wheel.url.to_string();
                let archive_info = if hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Built(BuiltDist::Path(wheel)) => {
                let url = wheel.url.to_string();
                let archive_info = if hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Source(SourceDist::Registry(sdist)) => {
                let url = sdist.file.url.to_string();
                // Use hashes from the file itself, not from the Resolution
                let file_hashes = sdist.file.hashes.as_slice();
                let archive_info = if file_hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(file_hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Source(SourceDist::DirectUrl(sdist)) => {
                let url = sdist.url.to_string();
                let archive_info = if hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Source(SourceDist::Git(sdist)) => {
                let url = sdist.url.to_string();
                let vcs_info = Some(VcsInfo {
                    vcs: "git".to_string(),
                    requested_revision: sdist.git.reference().as_str().map(|s| s.to_string()),
                    commit_id: sdist
                        .git
                        .precise()
                        .map(|oid| oid.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                });
                Some(DownloadInfo {
                    url,
                    archive_info: None,
                    vcs_info,
                })
            }
            Dist::Source(SourceDist::Path(sdist)) => {
                let url = sdist.url.to_string();
                let archive_info = if hashes.is_empty() {
                    None
                } else {
                    Some(ArchiveInfo::from_hashes(hashes))
                };
                Some(DownloadInfo {
                    url,
                    archive_info,
                    vcs_info: None,
                })
            }
            Dist::Source(SourceDist::Directory(sdist)) => {
                let url = sdist.url.to_string();
                Some(DownloadInfo {
                    url,
                    archive_info: None,
                    vcs_info: None,
                })
            }
        }
    }

    /// Create a minimal report item with just name and version.
    pub fn minimal(name: &PackageName, version: &Version) -> Self {
        Self {
            metadata: InstallationMetadata {
                name: name.to_string(),
                version: version.to_string(),
                requires_dist: None,
                requires_python: None,
                provides_extra: None,
            },
            is_direct: false,
            is_yanked: false,
            download_info: None,
            requested: false,
            requested_extras: None,
        }
    }
}
