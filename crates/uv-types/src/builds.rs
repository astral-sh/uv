use uv_python::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Default, Copy, Clone)]
pub enum BuildIsolation<'a> {
    #[default]
    Isolated,
    Shared(&'a PythonEnvironment),
}

impl<'a> BuildIsolation<'a> {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}
