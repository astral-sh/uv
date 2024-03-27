use distribution_types::LocalEditable;
use pep508_rs::Requirement;
use pypi_types::Metadata23;
use uv_normalize::PackageName;

use crate::{preferences::Preference, Exclusions};

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) constraints: Vec<Requirement>,
    pub(crate) overrides: Vec<Requirement>,
    pub(crate) preferences: Vec<Preference>,
    pub(crate) project: Option<PackageName>,
    pub(crate) editables: Vec<(LocalEditable, Metadata23)>,
    pub(crate) exclusions: Exclusions,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
        preferences: Vec<Preference>,
        project: Option<PackageName>,
        editables: Vec<(LocalEditable, Metadata23)>,
        exclusions: Exclusions,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            preferences,
            project,
            editables,
            exclusions,
        }
    }

    pub fn simple(requirements: Vec<Requirement>) -> Self {
        Self {
            requirements,
            constraints: Vec::new(),
            overrides: Vec::new(),
            preferences: Vec::new(),
            project: None,
            editables: Vec::new(),
            exclusions: Exclusions::default(),
        }
    }
}
