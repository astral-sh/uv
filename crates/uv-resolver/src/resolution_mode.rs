use uv_configuration::ResolutionMode;

use crate::resolver::{ForkMap, ForkSet};
use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// Like [`ResolutionMode`], but with any additional information required to select a candidate,
/// like the set of direct dependencies.
#[derive(Debug, Clone)]
pub(crate) enum ResolutionStrategy {
    /// Resolve the highest compatible version of each package.
    Highest,
    /// Resolve the lowest compatible version of each package.
    Lowest,
    /// Resolve the lowest compatible version of any direct dependencies, and the highest
    /// compatible version of any transitive dependencies.
    LowestDirect(ForkSet),
}

impl ResolutionStrategy {
    pub(crate) fn from_mode(
        mode: ResolutionMode,
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        match mode {
            ResolutionMode::Highest => Self::Highest,
            ResolutionMode::Lowest => Self::Lowest,
            ResolutionMode::LowestDirect => {
                let mut first_party = ForkMap::default();
                for requirement in manifest.user_requirements(env, dependencies) {
                    first_party.add(&requirement, ());
                }
                Self::LowestDirect(first_party)
            }
        }
    }
}
