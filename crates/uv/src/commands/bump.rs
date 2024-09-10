use std::str::FromStr;

use anyhow::{anyhow, Error, Result};

use clap::ValueEnum;
use pep440_rs::Version;
use uv_cli::VersionFormat;
use uv_fs::CWD;
use uv_workspace::{pyproject::{self, PyProjectToml}, DiscoveryOptions, Workspace};

use super::{project, version};



#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum BumpType {
    Major,
    Minor,
    Patch,
}
impl BumpType{
    fn zero_or_one(&self, am_i: BumpType) -> u64 {
        if self == &am_i {
            1
        } else {
            0
        }
    }
}

enum BumpInstruction{
    Bump(BumpType),
    String(String),
}



/// Display version information
pub(crate) async fn bump(to: Option<BumpInstruction>, buffer: &mut dyn std::io::Write) -> Result<(), Error> {
    // Find the project version.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;
    let mut pyproject_toml = workspace.pyproject_toml();
    match pyproject_toml.project{
        Some(mut project) => {
                match project.version {
                Some(mut current_version) => {
                    match to {
                        Some(to) => {
                            let mut new_version;
                            match to {
                                BumpInstruction::Bump(bump_type) => {
                                    new_version = bumped_version(bump_type, &current_version, buffer)?;
                                }
                                BumpInstruction::String(version) => {
                                    new_version = Version::from_str(&version)?;
                                }
                            // Update the project version.
                            writeln!(buffer, "Updated version: {}", new_version.to_string())?;
                            }
                            project.version = Some(new_version);
                        }
                        None => {
                            // on no bump instruction, just display the current version
                            writeln!(buffer, "Current version: {}", current_version.to_string())?;
                        }
                    }
                }
                None => {
                    return Err(anyhow!("project version not set"));
                }
            }
        },
        None => {
            return Err(anyhow!("project version not set"));
        }
    };



    
    Ok(())
}


fn bumped_version(bump: BumpType, from: &Version, buffer: &mut dyn std::io::Write) -> Result<Version> {
    let mut ret = from.clone();
    if from.is_dev()  || from.is_post(){
        writeln!(buffer, "WARNING: dev or post versions will be bumped to release versions");  // TODO: how do they do warnings here?
    }
    let index = bump.clone() as usize;
    // minor / major / patch not exist set to 1/0 based on the 
    // bump type
    if from.release().get(index).is_none() {
        ret = Version::new([
            *from.release().get(0).unwrap_or(
                &(bump.zero_or_one(BumpType::Major)),
            ),
            *from.release().get(1).unwrap_or(
                &(bump.zero_or_one(BumpType::Minor)),
            ),
            *from.release().get(2).unwrap_or(
                &(bump.zero_or_one(BumpType::Patch)),
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



