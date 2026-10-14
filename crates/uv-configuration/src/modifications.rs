/// Whether an installation may retain unrelated packages.
#[derive(Debug, Clone, Copy)]
pub enum Modifications {
    /// Use `pip install` semantics, whereby existing installations are left as-is, unless they are
    /// marked for re-installation or upgrade.
    ///
    /// Ensures that the resulting environment is sufficient to meet the requirements, but without
    /// any unnecessary changes.
    Sufficient,
    /// Use `pip sync` semantics, whereby any existing, extraneous installations are removed.
    ///
    /// Ensures that the resulting environment is an exact match for the requirements, but may
    /// result in more changes than necessary.
    Exact,
}
