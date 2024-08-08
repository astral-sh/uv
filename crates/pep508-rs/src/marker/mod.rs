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

mod environment;
pub(crate) mod parse;
mod tree;

pub use environment::{MarkerEnvironment, MarkerEnvironmentBuilder};
pub use tree::{
    ExtraOperator, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue, MarkerValueString,
    MarkerValueVersion, MarkerWarningKind, StringVersion,
};
