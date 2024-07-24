use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::Deserialize;

use uv_fs::Simplified;
use uv_warnings::warn_user;

/// The version control system to use.
#[derive(Clone, Copy, Debug, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum VersionControl {
    /// Use Git for version control.
    #[default]
    Git,

    /// Do not use version control.
    None,
}

impl VersionControl {
    /// Initializes the VCS system based on the provided path.
    pub fn init(&self, path: &PathBuf) -> Result<()> {
        match self {
            VersionControl::None => Ok(()),
            VersionControl::Git => Self::init_git(path),
        }
    }

    fn init_git(path: &PathBuf) -> Result<()> {
        let Ok(git) = which::which("git") else {
            anyhow::bail!("could not find `git` in PATH");
        };

        if path.join(".git").try_exists()? {
            warn_user!(
                "Git repository already exists at `{}`",
                path.simplified_display()
            );
        } else {
            if !Command::new(git)
                .arg("init")
                .current_dir(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("failed to run `git init`")?
                .success()
            {
                anyhow::bail!("`git init` failed at `{}`", path.simplified_display());
            }
        }

        // Create the `.gitignore` if it does not already exist.
        let gitignore = path.join(".gitignore");
        if !gitignore.try_exists()? {
            fs_err::write(gitignore, gitignore_content())?;
        };

        Ok(())
    }
}

/// The default content for a `.gitignore` file in a Python project.
fn gitignore_content() -> &'static str {
    indoc::indoc! {r"
        # Python generated files
        __pycache__/
        *.py[oc]
        build/
        dist/
        wheels/
        *.egg-info

        # venv
        .venv
    "}
}

impl FromStr for VersionControl {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "git" => Ok(VersionControl::Git),
            "none" => Ok(VersionControl::None),
            other => Err(format!("unknown vcs specification: `{other}`")),
        }
    }
}

impl std::fmt::Display for VersionControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionControl::Git => write!(f, "git"),
            VersionControl::None => write!(f, "none"),
        }
    }
}

/// Check if the path is inside a VCS repository.
///
/// Currently only supports Git.
pub fn existing_vcs_repo(dir: &Path) -> bool {
    is_inside_git_work_tree(dir)
}

/// Check if the path is inside a Git work tree.
fn is_inside_git_work_tree(dir: &Path) -> bool {
    Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
