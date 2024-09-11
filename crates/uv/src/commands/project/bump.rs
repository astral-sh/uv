use anyhow::Result;
use owo_colors::OwoColorize;
use std::fmt::Write;
use std::str::FromStr;
use uv_cli::BumpType;

use uv_fs::CWD;

use crate::{commands::ExitStatus, printer::Printer};
use pep440_rs::Version;
use uv_workspace::{
    pyproject_mut::{DependencyTarget, PyProjectTomlMut},
    DiscoveryOptions, Workspace,
};

/// Display version information
pub(crate) async fn bump(to: Option<BumpInstruction>, printer: Printer) -> Result<ExitStatus> {
    // Find the project version.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;
    let mut pyproject = PyProjectTomlMut::from_toml(
        &workspace.pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    let current_version = pyproject.version()?;
    if let Some(bump) = to {
        let new_version = match bump {
            BumpInstruction::Bump(bump_type) => {
                bumped_version(&bump_type, &current_version, printer)?
            }
            BumpInstruction::String(version) => Version::from_str(&version)?,
        };
        pyproject.set_version(&new_version)?;
        let pyproject_path = workspace.install_path().join("pyproject.toml");
        fs_err::write(pyproject_path, pyproject.to_string())?;
        writeln!(
            printer.stdout(),
            "Bumped from {} to: {}",
            current_version.cyan(),
            new_version.cyan()
        )?;
    } else {
        writeln!(
            printer.stdout(),
            "Current version: {}",
            current_version.to_string().cyan()
        )?;
    }
    Ok(ExitStatus::Success)
}

pub(crate) enum BumpInstruction {
    Bump(BumpType),
    String(String),
}

fn bumped_version(bump: &BumpType, from: &Version, printer: Printer) -> Result<Version> {
    if from.is_dev() || from.is_post() {
        writeln!(
            printer.stdout(),
            "WARNING: dev or post versions will be bumped to release versions"
        )?;
    }
    let index = bump.clone() as usize;
    let new_release_vec = (0..3)
        .map(|i| {
            #[allow(clippy::comparison_chain)]
            if i == index {
                // we need to bump it
                return from.release().get(i).map_or(1, |v| v + 1);
            } else if i > index {
                // reset the values after the bump index
                return 0;
            } else {
                // get the value from the current version or default to 0
                return from.release().get(i).map_or(0, |v| *v);
            }
        })
        .collect::<Vec<u64>>();
    Ok(Version::new(new_release_vec))
}
