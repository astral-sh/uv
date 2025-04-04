use std::fmt::Write;
use std::str::FromStr;
use std::{cmp::Ordering, path::Path};

use anyhow::{anyhow, Result};
use owo_colors::OwoColorize;

use uv_cli::version::VersionInfo;
use uv_cli::{VersionBump, VersionFormat};
use uv_fs::Simplified;
use uv_pep440::Version;
use uv_workspace::pyproject_mut::Error;
use uv_workspace::{
    pyproject_mut::{DependencyTarget, PyProjectTomlMut},
    DiscoveryOptions, Workspace, WorkspaceCache,
};

use crate::{commands::ExitStatus, printer::Printer};

/// Display version information for uv itself
pub(crate) fn self_version(
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    let version_info = uv_cli::version::uv_self_version();
    print_version(version_info, None, short, output_format, printer)?;

    Ok(ExitStatus::Success)
}

/// Read or update project version (`uv metadata version`)
pub(crate) async fn project_version(
    project_dir: &Path,
    value: Option<String>,
    bump: Option<VersionBump>,
    dry_run: bool,
    short: bool,
    output_format: VersionFormat,
    strict: bool,
    cache: &WorkspaceCache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Read the metadata
    let workspace =
        match Workspace::discover(project_dir, &DiscoveryOptions::default(), cache).await {
            Ok(workspace) => workspace,
            Err(err) => {
                // If strict, hard bail on missing workspace
                if strict {
                    return Err(err)?;
                }
                // Otherwise, warn and provide fallback
                writeln!(
                    printer.stderr(),
                    r"warning: failed to read project: {err}
  running `uv self version` for compatibility with old `uv version` command.
  this fallback will be removed soon, pass `--preview` to make this an error.
"
                )?;
                return self_version(short, output_format, printer);
            }
        };

    let mut pyproject = PyProjectTomlMut::from_toml(
        &workspace.pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    let pyproject_path = workspace.install_path().join("pyproject.toml");
    let name = workspace
        .pyproject_toml()
        .project
        .as_ref()
        .map(|project| &project.name);
    let old_version = pyproject.version().map_err(|err| match err {
        Error::MalformedWorkspace => {
            if pyproject.has_dynamic_version() {
                anyhow!(
                    "We cannot get or set dynamic project versions in: {}",
                    pyproject_path.user_display()
                )
            } else {
                anyhow!(
                    "There is no 'project.version' field in: {}",
                    pyproject_path.user_display()
                )
            }
        }
        err => {
            anyhow!("{err}: {}", pyproject_path.user_display())
        }
    })?;

    // Figure out new metadata
    let new_version = if let Some(value) = value {
        match Version::from_str(&value) {
            Ok(version) => Some(version),
            Err(err) => match &*value {
                "major" | "minor" | "patch" => {
                    return Err(anyhow!(
                        "Invalid version `{value}`, did you mean to pass `--bump {value}`?"
                    ));
                }
                _ => {
                    return Err(err)?;
                }
            },
        }
    } else if let Some(bump) = bump {
        Some(bumped_version(&old_version, bump, printer)?)
    } else {
        None
    };

    // Apply the metadata
    if let Some(new_version) = &new_version {
        if !dry_run {
            pyproject.set_version(new_version)?;
            fs_err::write(pyproject_path, pyproject.to_string())?;
        }
    }

    // Report the results
    let old_version = VersionInfo::new(name, &old_version);
    let new_version = new_version.map(|version| VersionInfo::new(name, &version));
    print_version(old_version, new_version, short, output_format, printer)?;

    Ok(ExitStatus::Success)
}

fn print_version(
    old_version: VersionInfo,
    new_version: Option<VersionInfo>,
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<()> {
    match output_format {
        VersionFormat::Text => {
            if let Some(name) = &old_version.package_name {
                if !short {
                    write!(printer.stdout(), "{name} ")?;
                }
            }
            if let Some(new_version) = new_version {
                if short {
                    writeln!(printer.stdout(), "{}", new_version.cyan())?;
                } else {
                    writeln!(
                        printer.stdout(),
                        "{} => {}",
                        old_version.cyan(),
                        new_version.cyan()
                    )?;
                }
            } else {
                writeln!(printer.stdout(), "{}", old_version.cyan())?;
            }
        }
        VersionFormat::Json => {
            let final_version = new_version.unwrap_or(old_version);
            let string = serde_json::to_string_pretty(&final_version)?;
            writeln!(printer.stdout(), "{string}")?;
        }
    }
    Ok(())
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
