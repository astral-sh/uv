use distribution_types::LocalEditable;
use pep508_rs::Requirement;
use pypi_types::Metadata21;
use uv_normalize::PackageName;

/// A manifest of requirements, constraints, and preferences.
#[derive(Debug)]
pub struct Manifest {
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) constraints: Vec<Requirement>,
    pub(crate) overrides: Vec<Requirement>,
    pub(crate) preferences: Vec<Requirement>,
    pub(crate) project: Option<PackageName>,
    pub(crate) editables: Vec<(LocalEditable, Metadata21)>,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
        preferences: Vec<Requirement>,
        project: Option<PackageName>,
        editables: Vec<(LocalEditable, Metadata21)>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            preferences,
            project,
            editables,
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
        }
    }
}
