use pep508_rs::Requirement;

use crate::prerelease_mode::PreReleaseMode;
use crate::resolution_mode::ResolutionMode;

/// A manifest of requirements, constraints, and preferences.
#[derive(Debug)]
pub struct Manifest {
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) constraints: Vec<Requirement>,
    pub(crate) preferences: Vec<Requirement>,
    pub(crate) resolution_mode: ResolutionMode,
    pub(crate) prerelease_mode: PreReleaseMode,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        preferences: Vec<Requirement>,
        resolution_mode: ResolutionMode,
        prerelease_mode: PreReleaseMode,
    ) -> Self {
        Self {
            requirements,
            constraints,
            preferences,
            resolution_mode,
            prerelease_mode,
        }
    }
}
