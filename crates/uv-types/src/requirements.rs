use pypi_types::Requirement;
use uv_normalize::ExtraName;

/// A set of requirements as requested by a parent requirement.
///
/// For example, given `flask[dotenv]`, the `RequestedRequirements` would include the `dotenv`
/// extra, along with all of the requirements that are included in the `flask` distribution
/// including their unevaluated markers.
#[derive(Debug, Clone)]
pub struct RequestedRequirements {
    /// The set of extras included on the originating requirement.
    extras: Vec<ExtraName>,
    /// The set of requirements that were requested by the originating requirement.
    requirements: Vec<Requirement>,
    /// Whether the dependencies were direct or transitive.
    direct: bool,
}

impl RequestedRequirements {
    /// Instantiate a [`RequestedRequirements`] with the given `extras` and `requirements`.
    pub fn new(extras: Vec<ExtraName>, requirements: Vec<Requirement>, direct: bool) -> Self {
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
