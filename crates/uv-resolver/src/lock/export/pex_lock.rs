//! PEX lock file format support.
//!
//! This module provides functionality to export UV lock files to the PEX lock format,
//! which is used by the PEX packaging tool and Pantsbuild for reproducible Python builds.
//!
//! The PEX lock format is a JSON-based format that includes:
//! - Package metadata and version constraints
//! - Platform-specific resolves with 3-component platform tags
//! - Artifact information with separate algorithm and hash fields
//! - Build and resolution configuration

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::lock::{Lock, LockError, WheelWireSource};

/// A PEX lock file representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexLock {
    /// Whether to allow building from source.
    pub allow_builds: bool,
    /// Whether to allow prereleases.
    pub allow_prereleases: bool,
    /// Whether to allow wheels.
    pub allow_wheels: bool,
    /// Whether to use build isolation.
    pub build_isolation: bool,
    /// Constraints applied during resolution.
    pub constraints: Vec<String>,
    /// Whether to elide unused requires_dist.
    pub elide_unused_requires_dist: bool,
    /// Excluded packages.
    pub excluded: Vec<String>,
    /// Locked resolved dependencies.
    pub locked_resolves: Vec<PexLockedResolve>,
    /// Only build packages.
    pub only_builds: Vec<String>,
    /// Only wheel packages.
    pub only_wheels: Vec<String>,
    /// Overridden packages.
    pub overridden: Vec<String>,
    /// Path mappings.
    pub path_mappings: serde_json::Map<String, serde_json::Value>,
    /// The PEX version used to generate this lock file.
    pub pex_version: String,
    /// The pip version used.
    pub pip_version: String,
    /// Whether to prefer older binary versions.
    pub prefer_older_binary: bool,
    /// Direct requirements.
    pub requirements: Vec<String>,
    /// The resolver version used.
    pub resolver_version: String,
    /// The style of resolution.
    pub style: String,
    /// Target systems.
    pub target_systems: Vec<String>,
    /// Whether to include transitive dependencies.
    pub transitive: bool,
    /// Whether to use PEP517 build backend.
    pub use_pep517: Option<bool>,
    /// Whether to use system time.
    pub use_system_time: bool,
}

/// A locked resolve entry in a PEX lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexLockedResolve {
    /// The locked requirements for this platform.
    pub locked_requirements: Vec<PexLockedRequirement>,
    /// The platform tag this resolve applies to (null for universal).
    pub platform_tag: Option<Vec<String>>,
}

/// A locked requirement in a PEX lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexLockedRequirement {
    /// Artifacts (wheels/sdists) for this requirement.
    pub artifacts: Vec<PexArtifact>,
    /// The project name.
    pub project_name: String,
    /// Dependencies of this requirement.
    pub requires_dists: Vec<String>,
    /// Python version requirement.
    pub requires_python: String,
    /// The version.
    pub version: String,
}

/// An artifact in a PEX lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexArtifact {
    /// The artifact URL.
    pub url: String,
    /// The filename.
    pub filename: String,
    /// Hash algorithm (e.g., "sha256").
    pub algorithm: String,
    /// Hash value.
    pub hash: String,
    /// Whether this is a wheel.
    pub is_wheel: bool,
}

impl PexLock {
    /// Default PEX version for generated lock files.
    const DEFAULT_PEX_VERSION: &'static str = "2.44.0";

    /// Default pip version.
    const DEFAULT_PIP_VERSION: &'static str = "24.2";

    /// Default hash algorithm when none is specified.
    const DEFAULT_HASH_ALGORITHM: &'static str = "sha256";

    /// Extract algorithm and hash from a hash string.
    fn parse_hash(hash_str: &str) -> (String, String) {
        if let Some(colon_pos) = hash_str.find(':') {
            let algorithm = hash_str[..colon_pos].to_string();
            let hash_value = hash_str[colon_pos + 1..].to_string();
            (algorithm, hash_value)
        } else {
            (
                Self::DEFAULT_HASH_ALGORITHM.to_string(),
                hash_str.to_string(),
            )
        }
    }

    /// Create a new PEX lock from a UV lock file.
    pub fn from_lock(lock: &Lock) -> Result<Self, LockError> {
        let mut requirements = Vec::new();
        let mut locked_requirements = Vec::new();

        // Collect root requirements
        if let Some(root) = lock.root() {
            for dep in &root.dependencies {
                if let Some(version) = lock
                    .packages()
                    .iter()
                    .find(|pkg| pkg.id.name == dep.package_id.name)
                    .and_then(|pkg| pkg.id.version.as_ref())
                {
                    requirements.push(format!("{}=={}", dep.package_id.name, version));
                }
            }
        }

        // Process all packages for locked requirements
        for package in lock.packages() {
            // Create locked requirement
            let mut artifacts = Vec::new();

            // Add wheels
            for wheel in &package.wheels {
                let wheel_url = match &wheel.url {
                    WheelWireSource::Url { url } => url.to_string(),
                    WheelWireSource::Path { path } => format!("file://{}", path.to_string_lossy()),
                    WheelWireSource::Filename { filename } => filename.to_string(),
                };

                let (algorithm, hash) = if let Some(h) = wheel.hash.as_ref() {
                    Self::parse_hash(&h.to_string())
                } else {
                    continue;
                };

                artifacts.push(PexArtifact {
                    url: wheel_url,
                    filename: wheel.filename.to_string(),
                    algorithm,
                    hash,
                    is_wheel: true,
                });
            }

            // Add source distributions
            if let Some(sdist) = &package.sdist {
                let Some(sdist_url) = sdist.url().map(std::string::ToString::to_string) else {
                    continue;
                };
                let Some(sdist_filename) = sdist.filename().map(std::string::ToString::to_string) else {
                    continue;
                };

                let (algorithm, hash) = if let Some(h) = sdist.hash() {
                    Self::parse_hash(&h.to_string())
                } else {
                    continue;
                };

                artifacts.push(PexArtifact {
                    url: sdist_url,
                    filename: sdist_filename,
                    algorithm,
                    hash,
                    is_wheel: false,
                });
            }

            if let Some(version) = &package.id.version {
                // Collect dependencies for this package
                let mut requires_dists = Vec::new();
                for dep in &package.dependencies {
                    if let Some(dep_version) = lock
                        .packages()
                        .iter()
                        .find(|pkg| pkg.id.name == dep.package_id.name)
                        .and_then(|pkg| pkg.id.version.as_ref())
                    {
                        requires_dists.push(format!("{}=={}", dep.package_id.name, dep_version));
                    }
                }

                locked_requirements.push(PexLockedRequirement {
                    artifacts,
                    project_name: package.id.name.to_string(),
                    requires_dists,
                    requires_python: lock.requires_python().to_string(),
                    version: version.to_string(),
                });
            }
        }

        let locked_resolves = vec![PexLockedResolve {
            locked_requirements,
            platform_tag: None,
        }];

        Ok(PexLock {
            allow_builds: true,
            allow_prereleases: false,
            allow_wheels: true,
            build_isolation: true,
            constraints: Vec::new(),
            elide_unused_requires_dist: false,
            excluded: Vec::new(),
            locked_resolves,
            only_builds: Vec::new(),
            only_wheels: Vec::new(),
            overridden: Vec::new(),
            path_mappings: serde_json::Map::new(),
            pex_version: Self::DEFAULT_PEX_VERSION.to_string(),
            pip_version: Self::DEFAULT_PIP_VERSION.to_string(),
            prefer_older_binary: false,
            requirements,
            resolver_version: "pip-2020-resolver".to_string(),
            style: "universal".to_string(),
            target_systems: vec!["linux".to_string(), "mac".to_string()],
            transitive: true,
            use_pep517: None,
            use_system_time: false,
        })
    }

    /// Serialize the PEX lock to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl fmt::Display for PexLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_json() {
            Ok(json) => write!(f, "{json}"),
            Err(err) => write!(f, "Error serializing PEX lock: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pex_lock_serialization() {
        let pex_lock = PexLock {
            allow_builds: true,
            allow_prereleases: false,
            allow_wheels: true,
            build_isolation: true,
            constraints: vec![],
            elide_unused_requires_dist: false,
            excluded: vec![],
            locked_resolves: vec![],
            only_builds: vec![],
            only_wheels: vec![],
            overridden: vec![],
            path_mappings: serde_json::Map::new(),
            pex_version: PexLock::DEFAULT_PEX_VERSION.to_string(),
            pip_version: PexLock::DEFAULT_PIP_VERSION.to_string(),
            prefer_older_binary: false,
            requirements: vec!["requests==2.31.0".to_string()],
            resolver_version: "pip-2020-resolver".to_string(),
            style: "universal".to_string(),
            target_systems: vec!["linux".to_string(), "mac".to_string()],
            transitive: true,
            use_pep517: None,
            use_system_time: false,
        };

        let json = pex_lock.to_json().unwrap();
        assert!(json.contains("\"pex_version\": \"2.44.0\""));
        assert!(json.contains("\"allow_builds\": true"));
    }
}
