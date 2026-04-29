use uv_distribution_types::{Requirement, Resolution};
use uv_normalize::ExtraName;

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

/// A set of requirements as requested by a parent requirement.
///
/// For example, given `flask[dotenv]`, the `RequestedRequirements` would include the `dotenv`
/// extra, along with all of the requirements that are included in the `flask` distribution
/// including their unevaluated markers.
#[derive(Debug, Clone)]
pub struct RequestedRequirements {
    /// The set of extras included on the originating requirement.
    extras: Box<[ExtraName]>,
    /// The set of requirements that were requested by the originating requirement.
    requirements: Box<[Requirement]>,
    /// Whether the dependencies were direct or transitive.
    direct: bool,
}

impl RequestedRequirements {
    /// Instantiate a [`RequestedRequirements`] with the given `extras` and `requirements`.
    pub fn new(extras: Box<[ExtraName]>, requirements: Box<[Requirement]>, direct: bool) -> Self {
        Self {
            extras,
            requirements,
            direct,
        }
    }

    /// Return the extras that were included on the originating requirement.
    pub fn extras(&self) -> &[ExtraName] {
        &self.extras
    }

    /// Return the requirements that were included on the originating requirement.
    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    /// Return whether the dependencies were direct or transitive.
    pub fn direct(&self) -> bool {
        self.direct
    }
}
