//! Integration tests for uv synchronization and settings.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod centralized_envs;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod show_settings;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod sync;
