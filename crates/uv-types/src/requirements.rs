use uv_distribution_types::Resolution;

use crate::HashStrategy;

/// A resolved set of requirements, along with the hash policy discovered while resolving them.
#[derive(Debug, Clone)]
pub struct ResolvedRequirements {
    /// The resolved distributions to install.
    resolution: Resolution,
    /// The hash policy to apply when installing the resolution.
    hasher: HashStrategy,
}

impl ResolvedRequirements {
    /// Instantiate a [`ResolvedRequirements`] with the given [`Resolution`] and [`HashStrategy`].
    pub fn new(resolution: Resolution, hasher: HashStrategy) -> Self {
        Self { resolution, hasher }
    }

    /// Return the resolved distributions to install.
    pub fn resolution(&self) -> &Resolution {
        &self.resolution
    }

    /// Return the hash policy to apply when installing the resolution.
    pub fn hasher(&self) -> &HashStrategy {
        &self.hasher
    }
}
