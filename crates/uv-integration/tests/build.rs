//! Integration tests for uv build commands and caches.

#[expect(
    dead_code,
    reason = "The build tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/audit.rs"]
mod audit;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/build.rs"]
mod build;

#[cfg(feature = "test-python")]
#[path = "it/build_backend.rs"]
mod build_backend;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/cache.rs"]
mod cache;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/cache_clean.rs"]
mod cache_clean;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/cache_prune.rs"]
mod cache_prune;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/cache_size.rs"]
mod cache_size;

#[path = "it/extract.rs"]
mod extract;
