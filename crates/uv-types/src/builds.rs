use uv_interpreter::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Copy, Clone)]
pub enum BuildIsolation<'a> {
    Isolated,
    Shared(&'a PythonEnvironment),
}

impl<'a> BuildIsolation<'a> {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}
