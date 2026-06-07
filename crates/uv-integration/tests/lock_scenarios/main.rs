//! Integration tests for lock scenarios and conflicts.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_conflict;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_exclude_newer_relative;

mod lock_scenarios;
