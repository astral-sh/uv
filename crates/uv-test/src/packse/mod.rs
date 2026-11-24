//! Local mock index for packse scenario tests.
//!
//! This module provides a [`PackseServer`] that reads packse scenario TOML definitions
//! and serves a PEP 691 Simple API + wheel/sdist downloads via a local wiremock server.
//! Each test gets its own server instance, so package names need no prefix mangling.
//!
//! To model a build graph, set `build_requires` on a package version:
//!
//! ```toml
//! [packages.example.versions."1.0"]
//! build_requires = ["hatchling", "generated-code>=2; python_version >= '3.12'"]
//! build_backend = "hatchling.build"
//! ```
//!
//! This changes only the sdist's `[build-system]`; `requires` still describes the installed
//! package's dependencies. The default is `["hatchling"]`, whereas `build_requires = []`
//! gives an empty build environment. The named backend must be importable to actually build.

pub mod scenario;
mod server;
mod wheel;

use std::path::{Path, PathBuf};

pub use server::{PackseServer, mount_mismatched_distribution};
pub use wheel::WheelBuilder;

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
