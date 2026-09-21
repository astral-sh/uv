#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum DependencyMode {
    /// Include all dependencies, whether direct or transitive.
    #[default]
    Transitive,
    /// Exclude transitive dependencies, only resolving the root package's immediate dependencies.
    Direct,
}

impl DependencyMode {
    /// Returns `true` if transitive dependencies should be included.
    pub(crate) fn is_transitive(self) -> bool {
        matches!(self, Self::Transitive)
    }

    /// Returns `true` if (only) direct dependencies should be excluded.
    pub(crate) fn is_direct(self) -> bool {
        matches!(self, Self::Direct)
    }
}
