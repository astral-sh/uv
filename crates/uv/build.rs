use std::path::Path;

use uv_static::EnvVars;

fn main() {
    // The workspace root directory is not available without walking up the tree
    // https://github.com/rust-lang/cargo/issues/3946
    let workspace_root = Path::new(&std::env::var(EnvVars::CARGO_MANIFEST_DIR).unwrap())
        .parent()
        .expect("CARGO_MANIFEST_DIR should be nested in workspace")
        .parent()
        .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
        .to_path_buf();

    has_git(&workspace_root);
}

fn has_git(workspace_root: &Path) {
    let git_dir = workspace_root.join(".git");
    if git_dir.exists() {
        println!("cargo:rustc-env=UV_TEST_HAS_COMMIT_HASH=1");
    }
}
