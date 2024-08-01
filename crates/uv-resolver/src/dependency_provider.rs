use std::convert::Infallible;

use pubgrub::{Dependencies, DependencyProvider, Range};

use pep440_rs::Version;

use crate::pubgrub::{PubGrubPackage, PubGrubPriority};
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

    fn prioritize(&self, _package: &Self::P, _range: &Self::VS) -> Self::Priority {
        unimplemented!()
    }
    type Priority = Option<PubGrubPriority>;

    type Err = Infallible;

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
