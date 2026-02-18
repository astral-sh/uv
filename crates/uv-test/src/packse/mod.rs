//! Local mock index for packse scenario tests.
//!
//! This module provides a [`PackseServer`] that reads packse scenario TOML definitions
//! and serves a PEP 691 Simple API + wheel/sdist downloads via a local wiremock server.
//! Each test gets its own server instance, so package names need no prefix mangling.

mod scenario;
mod server;
mod wheel;

use std::path::{Path, PathBuf};

pub use server::PackseServer;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("CARGO_MANIFEST_DIR should be nested under workspace root")
}

/// Base directory containing the vendored packse scenario TOML files.
fn scenarios_dir() -> PathBuf {
    workspace_root().join("test").join("scenarios")
}

/// Base directory containing vendored build-dependency wheels (hatchling, etc.).
fn vendor_dir() -> PathBuf {
    workspace_root().join("test").join("vendor")
}
