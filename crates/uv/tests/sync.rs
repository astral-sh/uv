//! Integration tests for uv synchronization and settings.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/show_settings.rs"]
mod show_settings;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/sync.rs"]
mod sync;
