use std::convert::Infallible;

use pubgrub::{Dependencies, DependencyProvider, PackageResolutionStatistics, Range};

use uv_pep440::Version;

use crate::pubgrub::{PubGrubPackage, PubGrubPriority, PubGrubTiebreaker};
use crate::resolver::UnavailableReason;

/// We don't use a dependency provider, we interact with state directly, but we still need this one
/// for type
#[derive(Clone)]
pub(crate) struct UvDependencyProvider;

impl DependencyProvider for UvDependencyProvider {
    type P = PubGrubPackage;
    type V = Version;
    type VS = Range<Version>;
    type M = UnavailableReason;
    /// Main priority and tiebreak for virtual packages.
    type Priority = (PubGrubPriority, PubGrubTiebreaker);
    type Err = Infallible;

    fn prioritize(
        &self,
        _package: &Self::P,
        _range: &Self::VS,
        _stats: &PackageResolutionStatistics,
    ) -> Self::Priority {
        unimplemented!()
    }

    fn choose_version(
        &self,
        _package: &Self::P,
        _range: &Self::VS,
    ) -> Result<Option<Self::V>, Self::Err> {
        unimplemented!()
    }

    fn get_dependencies(
        &self,
        _package: &Self::P,
        _version: &Self::V,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_size() {
        assert_eq!(
            size_of::<<UvDependencyProvider as DependencyProvider>::Priority>(),
            24
        );
    }
}
