#![allow(clippy::print_stdout)]

use crate::commands::ExitStatus;
use anyhow::Result;
use std::env;
use std::path::Path;

pub(crate) fn build_sdist(_sdist_directory: &Path) -> Result<ExitStatus> {
    todo!()
}
pub(crate) fn build_wheel(
    wheel_directory: &Path,
    metadata_directory: Option<&Path>,
) -> Result<ExitStatus> {
    let filename =
        uv_build_backend::build(&env::current_dir()?, wheel_directory, metadata_directory)?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}

pub(crate) fn build_editable(
    _wheel_directory: &Path,
    _metadata_directory: Option<&Path>,
) -> Result<ExitStatus> {
    todo!()
}

pub(crate) fn get_requires_for_build_sdist() -> Result<ExitStatus> {
    todo!()
}

pub(crate) fn get_requires_for_build_wheel() -> Result<ExitStatus> {
    todo!()
}
pub(crate) fn prepare_metadata_for_build_wheel(metadata_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::metadata(&env::current_dir()?, metadata_directory)?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}

pub(crate) fn get_requires_for_build_editable() -> Result<ExitStatus> {
    todo!()
}

pub(crate) fn prepare_metadata_for_build_editable(_wheel_directory: &Path) -> Result<ExitStatus> {
    todo!()
}
