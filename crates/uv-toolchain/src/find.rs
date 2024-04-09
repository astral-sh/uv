use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

pub static TOOLCHAIN_DIRECTORY: Lazy<PathBuf> = Lazy::new(|| {
    std::env::var_os("UV_BOOTSTRAP_DIR").map_or(
        Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .expect("CARGO_MANIFEST_DIR should be nested in workspace")
            .parent()
            .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
            .join("bin"),
        PathBuf::from,
    )
});
