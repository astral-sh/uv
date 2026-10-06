use uv_configuration::{BuildHashPolicy, BuildHashSources, Constraints};
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::{HashStrategy, HashStrategyError};

/// Hash verification and trusted hash sources for build dependencies.
///
/// Construct both from the same policy so build dispatch cannot pair a hash strategy with an
/// independently selected verification policy.
#[derive(Debug, Clone)]
pub struct BuildHashStrategy {
    hashes: HashStrategy,
    sources: BuildHashSources,
}

impl BuildHashStrategy {
    /// Disable hash verification for build dependencies.
    pub fn disabled() -> Self {
        Self {
            hashes: HashStrategy::default(),
            sources: BuildHashSources::AllRequirements,
        }
    }

    /// Read build constraints and select trusted hash sources from the resolved policy.
    pub fn from_constraints(
        constraints: &Constraints,
        marker_env: Option<&ResolverMarkerEnvironment>,
        policy: BuildHashPolicy,
    ) -> Result<Self, HashStrategyError> {
        let hashes = match policy.checking() {
            Some(mode) => HashStrategy::from_constraints(constraints, marker_env, mode)?,
            None => HashStrategy::default(),
        };
        Ok(Self {
            hashes,
            sources: policy.sources(),
        })
    }

    /// Also verify artifacts recorded in a lockfile, applying the build constraints to them.
    ///
    /// Required build hashes remain required, and the permitted sources for new hashes do not
    /// change when incorporating lockfile hashes.
    pub fn with_lockfile_hashes(mut self, hashes: HashStrategy) -> Result<Self, HashStrategyError> {
        self.hashes = hashes.with_constraint_hashes(&self.hashes)?;
        Ok(self)
    }

    /// Return the hashes to enforce when resolving and installing build dependencies.
    pub fn hashes(&self) -> &HashStrategy {
        &self.hashes
    }

    /// Return the requirement declarations that may contribute trusted hashes.
    pub fn sources(&self) -> BuildHashSources {
        self.sources
    }
}
