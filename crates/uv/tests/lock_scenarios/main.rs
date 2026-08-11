//! Integration tests for lock scenarios and conflicts.

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-universal"
))]
mod lock_conflict;

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-universal"
))]
mod lock_exclude_newer_relative;

#[cfg(feature = "test-universal")]
mod lock_scenarios;
