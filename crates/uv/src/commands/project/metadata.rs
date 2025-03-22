//! `uv metadata`

use std::cmp::Ordering;
use std::fmt::Write;
use std::str::FromStr;

use anyhow::Result;
use owo_colors::OwoColorize;

use uv_cli::VersionBump;
use uv_fs::CWD;
use uv_pep440::Version;
use uv_workspace::{
    pyproject_mut::{DependencyTarget, PyProjectTomlMut},
    DiscoveryOptions, Workspace, WorkspaceCache,
};

use crate::{commands::ExitStatus, printer::Printer};

/// Read or update project version (`uv metadata version`)
pub(crate) async fn metadata_version(
    value: Option<String>,
    bump: Option<VersionBump>,
    dry_run: bool,
    short: bool,
    cache: &WorkspaceCache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Read the metadata
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default(), cache).await?;
    let mut pyproject = PyProjectTomlMut::from_toml(
        &workspace.pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    let name = workspace
        .pyproject_toml()
        .project
        .as_ref()
        .map(|project| &project.name);
    let old_version = pyproject.version()?;

    // Figure out new metadata
    let new_version = if let Some(value) = value {
        Some(Version::from_str(&value)?)
    } else if let Some(bump) = bump {
        Some(bumped_version(&old_version, bump, printer)?)
    } else {
        None
    };

    // Apply the metadata
    if let Some(new_version) = &new_version {
        if !dry_run {
            pyproject.set_version(new_version)?;
            let pyproject_path = workspace.install_path().join("pyproject.toml");
            fs_err::write(pyproject_path, pyproject.to_string())?;
        }
    }

    // Report the results
    if let Some(name) = name {
        if !short {
            write!(printer.stdout(), "{name} ")?;
        }
    }
    if let Some(new_version) = new_version {
        if short {
            writeln!(printer.stdout(), "{}", new_version.cyan(),)?;
        } else {
            writeln!(
                printer.stdout(),
                "{} => {}",
                old_version.cyan(),
                new_version.cyan()
            )?;
        }
    } else {
        writeln!(printer.stdout(), "{}", old_version.cyan(),)?;
    }
    Ok(ExitStatus::Success)
}

fn bumped_version(from: &Version, bump: VersionBump, printer: Printer) -> Result<Version> {
    if from.is_dev() || from.is_post() {
        writeln!(
            printer.stderr(),
            "warning: dev or post versions will be bumped to release versions"
        )?;
    }

    let index = match bump {
        VersionBump::Major => 0,
        VersionBump::Minor => 1,
        VersionBump::Patch => 2,
    };

    // Use `max` here to try to do 0.2 => 0.3 instead of 0.2 => 0.3.0
    let old_parts = from.release();
    let len = old_parts.len().max(index + 1);
    let new_release_vec = (0..len)
        .map(|i| match i.cmp(&index) {
            // Everything before the bumped value is preserved (or is an implicit 0)
            Ordering::Less => old_parts.get(i).copied().unwrap_or(0),
            // This is the value to bump (could be implicit 0)
            Ordering::Equal => old_parts.get(i).copied().unwrap_or(0) + 1,
            // Everything after the bumped value becomes 0
            Ordering::Greater => 0,
        })
        .collect::<Vec<u64>>();
    Ok(Version::new(new_release_vec))
}
