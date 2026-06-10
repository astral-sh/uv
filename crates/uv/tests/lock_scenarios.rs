//! Integration tests for lock scenarios and conflicts.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/lock_conflict.rs"]
mod lock_conflict;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/lock_exclude_newer_relative.rs"]
mod lock_exclude_newer_relative;

#[path = "it/lock_scenarios.rs"]
mod lock_scenarios;
