//! Local mock index for synthetic Python packages.
//!
//! [`PackseServer`] serves packse scenarios or supplied wheels through a local
//! PEP 691 Simple API, with distribution downloads.
//! Each test gets its own server instance, so package names need no prefix mangling.

pub mod scenario;
mod server;
mod wheel;

use std::path::{Path, PathBuf};

pub use server::{PackseServer, mount_mismatched_distribution};
pub use wheel::{generate_wheel, generate_wheel_with_files};

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
