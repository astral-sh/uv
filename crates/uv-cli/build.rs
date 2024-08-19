use fs_err as fs;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    // The workspace root directory is not available without walking up the tree
    // https://github.com/rust-lang/cargo/issues/3946
    let workspace_root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .expect("CARGO_MANIFEST_DIR should be nested in workspace")
        .parent()
        .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
        .to_path_buf();

    commit_info(&workspace_root);

    #[allow(clippy::disallowed_methods)]
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-env=RUST_HOST_TARGET={target}");
}

fn commit_info(workspace_root: &Path) {
    // If not in a git repository, do not attempt to retrieve commit information
    let mut git_dir = workspace_root.join(".git");
    if !git_dir.exists() {
        return;
    }

    let mut git_head_path = git_dir.join("HEAD");

    // Set correct path for worktree
    if git_dir.is_file() {
        if let Some((Some(worktree_git_dir), worktree_git_head_path)) = fs::read_to_string(&git_dir)
            .ok()
            .and_then(|content| content.split_whitespace().last().map(PathBuf::from))
            .map(|worktree_gitdir| {
                let git_head_path = worktree_gitdir.join("HEAD");
                let git_dir = fs::read_to_string(worktree_gitdir.join("commondir"))
                    .ok()
                    .and_then(|content| worktree_gitdir.join(content.trim()).canonicalize().ok());
                (git_dir, git_head_path)
            })
        {
            git_dir = worktree_git_dir;
            git_head_path = worktree_git_head_path;
        }
    }

    if git_head_path.exists() {
        println!(
            "cargo:rerun-if-changed={}",
            git_head_path.as_path().display()
        );

        let git_head_contents = fs::read_to_string(git_head_path);
        if let Ok(git_head_contents) = git_head_contents {
            // The contents are either a commit or a reference in the following formats
            // - "<commit>" when the head is detached
            // - "ref <ref>" when working on a branch
            // If a commit, checking if the HEAD file has changed is sufficient
            // If a ref, we need to add the head file for that ref to rebuild on commit
            let mut git_ref_parts = git_head_contents.split_whitespace();
            git_ref_parts.next();
            if let Some(git_ref) = git_ref_parts.next() {
                let git_ref_path = git_dir.join(git_ref);
                println!(
                    "cargo:rerun-if-changed={}",
                    git_ref_path.as_path().display()
                );
            }
        }
    }

    let output = match Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--date=short")
        .arg("--abbrev=9")
        .arg("--format=%H %h %cd %(describe)")
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return,
    };
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut parts = stdout.split_whitespace();
    let mut next = || parts.next().unwrap();
    println!("cargo:rustc-env=UV_COMMIT_HASH={}", next());
    println!("cargo:rustc-env=UV_COMMIT_SHORT_HASH={}", next());
    println!("cargo:rustc-env=UV_COMMIT_DATE={}", next());

    // Describe can fail for some commits
    // https://git-scm.com/docs/pretty-formats#Documentation/pretty-formats.txt-emdescribeoptionsem
    if let Some(describe) = parts.next() {
        let mut describe_parts = describe.split('-');
        println!(
            "cargo:rustc-env=UV_LAST_TAG={}",
            describe_parts.next().unwrap()
        );
        // If this is the tagged commit, this component will be missing
        println!(
            "cargo:rustc-env=UV_LAST_TAG_DISTANCE={}",
            describe_parts.next().unwrap_or("0")
        );
    }
}
