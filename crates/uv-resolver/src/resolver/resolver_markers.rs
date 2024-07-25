use pep508_rs::{MarkerEnvironment, MarkerTree};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
/// Whether we're solving for a specific environment, universally or for a specific fork.
pub enum ResolverMarkers {
    /// We're solving for this specific environment only.
    SpecificEnvironment(MarkerEnvironment),
    /// We're doing a universal resolution for all environments (a python version
    /// constraint is expressed separately).
    Universal,
    /// We're in a fork of the universal resolution solving only for specific markers.
    Fork(MarkerTree),
}

impl ResolverMarkers {
    /// Add the markers of an initial or subsequent fork to the current markers.
    pub(crate) fn and(self, other: MarkerTree) -> MarkerTree {
        match self {
            ResolverMarkers::Universal => other,
            ResolverMarkers::Fork(mut current) => {
                current.and(other);
                current
            }
            ResolverMarkers::SpecificEnvironment(_) => {
                unreachable!("Specific environment mode must not fork")
            }
        }
    }

    /// If solving for a specific environment, return this environment.
    pub fn marker_environment(&self) -> Option<&MarkerEnvironment> {
        match self {
            ResolverMarkers::Universal | ResolverMarkers::Fork(_) => None,
            ResolverMarkers::SpecificEnvironment(env) => Some(env),
        }
    }
    /// If solving a fork, return that fork's markers.
    pub fn fork_markers(&self) -> Option<&MarkerTree> {
        match self {
            ResolverMarkers::SpecificEnvironment(_) | ResolverMarkers::Universal => None,
            ResolverMarkers::Fork(markers) => Some(markers),
        }
    }
}

impl Display for ResolverMarkers {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverMarkers::Universal => f.write_str("universal"),
            ResolverMarkers::SpecificEnvironment(_) => f.write_str("specific environment"),
            ResolverMarkers::Fork(markers) => {
                write!(f, "({markers})")
            }
        }
    }
}
