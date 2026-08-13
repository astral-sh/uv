use std::{
    path::{Path, PathBuf},
    process::Command,
};

use fs_err as fs;

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

    commit_info(&workspace_root);

    let target = std::env::var(EnvVars::TARGET).unwrap();
    println!("cargo:rustc-env=RUST_HOST_TARGET={target}");
}

fn commit_info(workspace_root: &Path) {
    // If not in a git repository, do not attempt to retrieve commit information
    let git_dir = workspace_root.join(".git");
    if !git_dir.exists() {
        return;
    }

    // If in a jj repository, do not attempt to retrieve commit information because they snapshot
    // frequently and will cause unnecessary rebuilds.
    let jj_dir = workspace_root.join(".jj");
    if jj_dir.exists() {
        return;
    }

    if let Some(git_head_path) = git_head(&git_dir) {
        println!("cargo:rerun-if-changed={}", git_head_path.display());

        let git_head_contents = fs::read_to_string(&git_head_path);
        if let Ok(git_head_contents) = git_head_contents {
            // The contents are either a commit or a reference in the following formats
            // - "<commit>" when the head is detached
            // - "ref: <ref>" when working on a branch
            // If a commit, checking if the HEAD file has changed is sufficient
            // If a ref, we also need to watch where Git stores its current commit
            let mut git_ref_parts = git_head_contents.split_whitespace();
            git_ref_parts.next();
            if let Some(git_ref) = git_ref_parts.next() {
                watch_git_ref(&git_head_path, git_ref);
            }
        }
    }

    let output = match Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--date=short")
        .arg("--abbrev=9")
        .arg("--format=%H %h %cd %(describe:tags)")
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return,
    };
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut parts = stdout.split_whitespace();
    let mut next = || parts.next().unwrap();
    println!("cargo:rustc-env={}={}", EnvVars::UV_COMMIT_HASH, next());
    println!(
        "cargo:rustc-env={}={}",
        EnvVars::UV_COMMIT_SHORT_HASH,
        next()
    );
    println!("cargo:rustc-env={}={}", EnvVars::UV_COMMIT_DATE, next());

    // Describe can fail for some commits
    // https://git-scm.com/docs/pretty-formats#Documentation/pretty-formats.txt-emdescribeoptionsem
    if let Some(describe) = parts.next() {
        let mut describe_parts = describe.split('-');
        println!(
            "cargo:rustc-env={}={}",
            EnvVars::UV_LAST_TAG,
            describe_parts.next().unwrap()
        );
        // If this is the tagged commit, this component will be missing
        println!(
            "cargo:rustc-env={}={}",
            EnvVars::UV_LAST_TAG_DISTANCE,
            describe_parts.next().unwrap_or("0")
        );
    }
}

fn git_head(git_dir: &Path) -> Option<PathBuf> {
    // The typical case is a standard git repository.
    if git_dir.is_dir() {
        return Some(git_dir.join("HEAD"));
    }
    if !git_dir.is_file() {
        return None;
    }
    // Watch the pointer in case the worktree's Git directory changes.
    println!("cargo:rerun-if-changed={}", git_dir.display());
    // A linked worktree has a `.git` file instead of a `.git` directory.
    // Its contents point to the worktree-specific Git directory, e.g.:
    //
    //     gitdir: /home/andrew/astral/uv/main/.git/worktrees/pr2
    //
    // And the HEAD file we want to watch will be at:
    //
    //     /home/andrew/astral/uv/main/.git/worktrees/pr2/HEAD
    let contents = fs::read_to_string(git_dir).ok()?;
    let (label, worktree_path) = contents.split_once(':')?;
    if label != "gitdir" {
        return None;
    }
    // Relative `gitdir:` paths are relative to the directory containing `.git`.
    let worktree_path = PathBuf::from(worktree_path.trim());
    let worktree_path = if worktree_path.is_absolute() {
        worktree_path
    } else {
        git_dir.parent()?.join(worktree_path)
    };
    Some(worktree_path.join("HEAD"))
}

/// Watch the loose or packed Git reference for the current branch.
fn watch_git_ref(git_head_path: &Path, git_ref: &str) {
    let Some(worktree_git_dir) = git_head_path.parent() else {
        return;
    };

    // Worktrees have their own HEAD, but branch refs live in the shared Git directory. Their
    // `commondir` file points to that directory, either absolutely or relative to this Git directory.
    let common_dir_path = worktree_git_dir.join("commondir");
    let common_git_dir = if let Ok(common_dir) = fs::read_to_string(&common_dir_path) {
        println!("cargo:rerun-if-changed={}", common_dir_path.display());
        let common_dir = PathBuf::from(common_dir.trim());
        if common_dir.is_absolute() {
            common_dir
        } else {
            worktree_git_dir.join(common_dir)
        }
    } else {
        worktree_git_dir.to_path_buf()
    };

    let git_ref_path = common_git_dir.join(git_ref);
    if git_ref_path.exists() {
        println!("cargo:rerun-if-changed={}", git_ref_path.display());
    } else {
        // A packed branch ref has no loose ref file. Watch `packed-refs` instead of the missing
        // loose ref, since Cargo would rebuild on every invocation for a nonexistent watched path.
        let packed_refs = common_git_dir.join("packed-refs");
        if packed_refs.exists() {
            println!("cargo:rerun-if-changed={}", packed_refs.display());
        }
        // A later commit can recreate the loose ref, even when its parent directories do not exist
        // yet. Watch the nearest existing ancestor so Cargo notices that transition. This can
        // also rebuild when another ref in that directory changes.
        if let Some(parent) = git_ref_path.ancestors().find(|parent| parent.is_dir()) {
            println!("cargo:rerun-if-changed={}", parent.display());
        }
    }
}
