use uv_python::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Default, Clone)]
pub enum BuildIsolation {
    #[default]
    Isolated,
    Shared(PythonEnvironment),
}

impl BuildIsolation {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}
