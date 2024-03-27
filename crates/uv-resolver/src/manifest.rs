use distribution_types::LocalEditable;
use pep508_rs::Requirement;
use pypi_types::Metadata23;
use uv_normalize::PackageName;
use uv_types::RequestedRequirements;

use crate::preferences::Preference;

/// A manifest of requirements, constraints, and preferences.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The direct requirements for the project.
    pub(crate) requirements: Vec<Requirement>,

    /// The constraints for the project.
    pub(crate) constraints: Vec<Requirement>,

    /// The overrides for the project.
    pub(crate) overrides: Vec<Requirement>,

    /// The preferences for the project.
    ///
    /// These represent "preferred" versions of a given package. For example, they may be the
    /// versions that are already installed in the environment, or already pinned in an existing
    /// lockfile.
    pub(crate) preferences: Vec<Preference>,

    /// The name of the project.
    pub(crate) project: Option<PackageName>,

    /// The editable requirements for the project, which are built in advance.
    ///
    /// The requirements of the editables should be included in resolution as if they were
    /// direct requirements in their own right.
    pub(crate) editables: Vec<(LocalEditable, Metadata23)>,

    /// The lookahead requirements for the project.
    ///
    /// These represent transitive dependencies that should be incorporated when making
    /// determinations around "allowed" versions (for example, "allowed" URLs or "allowed"
    /// pre-release versions).
    pub(crate) lookaheads: Vec<RequestedRequirements>,
}

impl Manifest {
    pub fn new(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
        preferences: Vec<Preference>,
        project: Option<PackageName>,
        editables: Vec<(LocalEditable, Metadata23)>,
        lookaheads: Vec<RequestedRequirements>,
    ) -> Self {
        Self {
            requirements,
            constraints,
            overrides,
            preferences,
            project,
            editables,
            lookaheads,
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
            lookaheads: Vec::new(),
        }
    }
}
