//! PEP 508 markers implementations with validation and warnings
//!
//! Markers allow you to install dependencies only in specific environments (python version,
//! operating system, architecture, etc.) or when a specific feature is activated. E.g. you can
//! say `importlib-metadata ; python_version < "3.8"` or
//! `itsdangerous (>=1.1.0) ; extra == 'security'`. Unfortunately, the marker grammar has some
//! oversights (e.g. <https://github.com/pypa/packaging.python.org/pull/1181>) and
//! the design of comparisons (PEP 440 comparisons with lexicographic fallback) leads to confusing
//! outcomes. This implementation tries to carefully validate everything and emit warnings whenever
//! bogus comparisons with unintended semantics are made.

mod algebra;
mod environment;
mod lowering;
pub(crate) mod parse;
mod simplify;
mod tree;
mod variants;

pub use environment::{MarkerEnvironment, MarkerEnvironmentBuilder};
pub use lowering::{
    CanonicalMarkerValueExtra, CanonicalMarkerValueString, CanonicalMarkerValueVersion,
};
pub use tree::{
    ContainsMarkerTree, ExtraMarkerTree, ExtraOperator, InMarkerTree, MarkerExpression,
    MarkerOperator, MarkerTree, MarkerTreeContents, MarkerTreeDebugGraph, MarkerTreeKind,
    MarkerValue, MarkerValueExtra, MarkerValueList, MarkerValueString, MarkerValueVersion,
    MarkerVariantsEnvironment, MarkerVariantsUniversal, MarkerWarningKind, StringMarkerTree,
    StringVersion, VersionMarkerTree,
};
pub use variants::{VariantFeature, VariantNamespace, VariantParseError, VariantValue};

/// `serde` helpers for [`MarkerTree`].
pub mod ser {
    use super::MarkerTree;
    use serde::Serialize;

    /// A helper for `serde(skip_serializing_if)`.
    pub fn is_empty(marker: &MarkerTree) -> bool {
        marker.contents().is_none()
    }

    /// A helper for `serde(serialize_with)`.
    ///
    /// Note this will panic if `marker.contents()` is `None`, and so should be paired with `is_empty`.
    pub fn serialize<S>(marker: &MarkerTree, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        marker.contents().unwrap().serialize(s)
    }
}
