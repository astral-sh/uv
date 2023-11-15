use chrono::{DateTime, Utc};
use pep508_rs::Requirement;
use puffin_normalize::PackageName;

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
    pub(crate) project: Option<PackageName>,
    pub(crate) exclude_newer: Option<DateTime<Utc>>,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        preferences: Vec<Requirement>,
        resolution_mode: ResolutionMode,
        prerelease_mode: PreReleaseMode,
        project: Option<PackageName>,
        exclude_newer: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            preferences,
            resolution_mode,
            prerelease_mode,
            project,
            exclude_newer,
        }
    }
}
