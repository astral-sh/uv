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
    pub fn is_transitive(self) -> bool {
        match self {
            Self::Transitive => true,
            Self::Direct => false,
        }
    }

    /// Returns `true` if only direct dependencies should be included.
    pub fn is_direct(self) -> bool {
        match self {
            Self::Transitive => false,
            Self::Direct => true,
        }
    }
}
