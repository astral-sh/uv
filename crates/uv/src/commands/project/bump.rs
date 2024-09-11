use std::str::FromStr;
use anyhow::Result;
use uv_cli::BumpType;
use std::fmt::Write;

use uv_fs::CWD;

use crate::{commands::ExitStatus, printer::Printer};
use pep440_rs::Version;
use uv_workspace::{pyproject_mut::{DependencyTarget, PyProjectTomlMut}, DiscoveryOptions, Workspace};


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
        if current_version.is_dev() || current_version.is_post() {
            writeln!(printer.stdout(), "WARNING: dev or post versions will be bumped to release versions")?;
        }
        let new_version;
        match bump {
            BumpInstruction::Bump(bump_type) => {
                new_version = bumped_version(bump_type, &current_version, printer)?;
            }
            BumpInstruction::String(version) => {
                new_version = Version::from_str(&version)?;
            }
        }
        pyproject.set_version(new_version)?;
        return Ok(ExitStatus::Success);
    }
    writeln!(printer.stdout(), "Current version: {}", current_version.to_string())?;
    Ok(ExitStatus::Success)
}


fn zero_or_one(i_am: &BumpType, expected: BumpType) -> u64 {
    if *i_am == expected {
        1
    } else {
        0
    }
}

pub(crate) enum BumpInstruction{
    Bump(BumpType),
    String(String),
}



fn bumped_version(bump: BumpType, from: &Version, printer: Printer) -> Result<Version> {
    let mut ret = from.clone();
    if from.is_dev()  || from.is_post(){
        writeln!(printer.stdout(), "WARNING: dev or post versions will be bumped to release versions")?;        
    }
    let index = bump.clone() as usize;
    // minor / major / patch not exist set to 1/0 based on the 
    // bump type
    if from.release().get(index).is_none() {
        ret = Version::new([
            *from.release().get(0).unwrap_or(
                &(zero_or_one(&bump, BumpType::Major)),
            ),
            *from.release().get(1).unwrap_or(
                &(zero_or_one(&bump, BumpType::Minor)),
            ),
            *from.release().get(2).unwrap_or(
                &(zero_or_one(&bump, BumpType::Patch)),
            ),
        ]);
    }

    let new_release_vec = (0..from.release().len()).map(|i| {
        if i == index {
            from.release()[i] + 1
        } else {
            from.release()[i]
        }
    }).collect::<Vec<u64>>();
    Ok(ret.with_release(new_release_vec).only_release())
}



