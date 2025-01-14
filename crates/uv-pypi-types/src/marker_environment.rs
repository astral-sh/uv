use tracing::debug;

use uv_pep508::MarkerEnvironment;

/// A wrapper type around [`MarkerEnvironment`] that ensures the Python version markers are
/// release-only, to match the resolver's semantics.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolverMarkerEnvironment(MarkerEnvironment);

impl ResolverMarkerEnvironment {
    /// Returns the underlying [`MarkerEnvironment`].
    pub fn markers(&self) -> &MarkerEnvironment {
        &self.0
    }
}

impl From<MarkerEnvironment> for ResolverMarkerEnvironment {
    fn from(value: MarkerEnvironment) -> Self {
        // Strip `python_version`.
        let python_version = value.python_version().only_release();
        let value = if python_version == **value.python_version() {
            value
        } else {
            debug!(
                "Stripping pre-release from `python_version`: {}",
                value.python_version()
            );
            value.with_python_version(python_version)
        };

        // Strip `python_full_version`.
        let python_full_version = value.python_full_version().only_release();
        let value = if python_full_version == **value.python_full_version() {
            value
        } else {
            debug!(
                "Stripping pre-release from `python_full_version`: {}",
                value.python_full_version()
            );
            value.with_python_full_version(python_full_version)
        };

        Self(value)
    }
}

impl std::ops::Deref for ResolverMarkerEnvironment {
    type Target = MarkerEnvironment;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
