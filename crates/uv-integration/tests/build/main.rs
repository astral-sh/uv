//! Integration tests for uv build commands and caches.

use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod audit;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod build;

#[cfg(feature = "test-python")]
mod build_backend;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_clean;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_prune;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_size;

mod extract;
