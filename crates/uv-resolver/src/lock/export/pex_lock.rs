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
    /// The PEX version used to generate this lock file.
    pub pex_version: String,
    /// Whether to allow building from source.
    pub allow_builds: bool,
    /// Whether to allow prereleases.
    pub allow_prereleases: bool,
    /// Whether to allow wheels.
    pub allow_wheels: bool,
    /// Whether to use build isolation.
    pub build_isolation: bool,
    /// Whether to prefer older binary versions.
    pub prefer_older_binary: bool,
    /// Whether to use PEP517 build backend.
    pub use_pep517: Option<bool>,
    /// The resolver version used.
    pub resolver_version: String,
    /// The style of resolution.
    pub style: String,
    /// Whether to include transitive dependencies.
    pub transitive: bool,
    /// Python version requirements.
    pub requires_python: Vec<String>,
    /// Direct requirements.
    pub requirements: Vec<String>,
    /// Constraints applied during resolution.
    pub constraints: Vec<String>,
    /// Locked resolved dependencies.
    pub locked_resolves: Vec<PexLockedResolve>,
}

/// A locked resolve entry in a PEX lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexLockedResolve {
    /// The platform tag this resolve applies to (3 components: [interpreter, abi, platform]).
    pub platform_tag: Vec<String>,
    /// The locked requirements for this platform.
    pub locked_requirements: Vec<PexLockedRequirement>,
}

/// A locked requirement in a PEX lock file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexLockedRequirement {
    /// The project name.
    pub project_name: String,
    /// The version.
    pub version: String,
    /// The requirement specifier.
    pub requirement: String,
    /// Artifacts (wheels/sdists) for this requirement.
    pub artifacts: Vec<PexArtifact>,
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

    /// Default hash algorithm when none is specified.
    const DEFAULT_HASH_ALGORITHM: &'static str = "sha256";

    /// Universal platform tag components: [interpreter, abi, platform].
    const UNIVERSAL_PLATFORM_TAG: [&'static str; 3] = ["py", "none", "any"];

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
                let Some(sdist_url) = sdist.url().map(|u| u.to_string()) else {
                    continue;
                };
                let Some(sdist_filename) = sdist.filename().map(|f| f.to_string()) else {
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
                locked_requirements.push(PexLockedRequirement {
                    project_name: package.id.name.to_string(),
                    version: version.to_string(),
                    requirement: format!("{}=={}", package.id.name, version),
                    artifacts,
                });
            }
        }

        let locked_resolves = vec![PexLockedResolve {
            platform_tag: Self::UNIVERSAL_PLATFORM_TAG
                .iter()
                .map(|s| s.to_string())
                .collect(),
            locked_requirements,
        }];

        Ok(PexLock {
            pex_version: Self::DEFAULT_PEX_VERSION.to_string(),
            allow_builds: true,
            allow_prereleases: false,
            allow_wheels: true,
            build_isolation: true,
            prefer_older_binary: false,
            use_pep517: None,
            resolver_version: Self::DEFAULT_PEX_VERSION.to_string(),
            style: "universal".to_string(),
            transitive: true,
            requires_python: vec![lock.requires_python().to_string()],
            requirements,
            constraints: Vec::new(),
            locked_resolves,
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
            Ok(json) => write!(f, "{}", json),
            Err(err) => write!(f, "Error serializing PEX lock: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pex_lock_serialization() {
        let pex_lock = PexLock {
            pex_version: PexLock::DEFAULT_PEX_VERSION.to_string(),
            allow_builds: true,
            allow_prereleases: false,
            allow_wheels: true,
            build_isolation: true,
            prefer_older_binary: false,
            use_pep517: None,
            resolver_version: PexLock::DEFAULT_PEX_VERSION.to_string(),
            style: "universal".to_string(),
            transitive: true,
            requires_python: vec![">=3.8".to_string()],
            requirements: vec!["requests==2.31.0".to_string()],
            constraints: vec![],
            locked_resolves: vec![],
        };

        let json = pex_lock.to_json().unwrap();
        assert!(json.contains("\"pex_version\": \"2.44.0\""));
        assert!(json.contains("\"allow_builds\": true"));
    }
}
