#![allow(clippy::print_stdout)]

use crate::commands::ExitStatus;
use anyhow::Result;
use std::env;
use std::path::Path;
use uv_build_backend::{SourceDistSettings, WheelSettings};

pub(crate) fn build_sdist(sdist_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::build_source_dist(
        &env::current_dir()?,
        sdist_directory,
        SourceDistSettings::default(),
        uv_version::version(),
    )?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}
pub(crate) fn build_wheel(
    wheel_directory: &Path,
    metadata_directory: Option<&Path>,
) -> Result<ExitStatus> {
    let filename = uv_build_backend::build_wheel(
        &env::current_dir()?,
        wheel_directory,
        metadata_directory,
        WheelSettings::default(),
        uv_version::version(),
    )?;
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
    let filename = uv_build_backend::metadata(
        &env::current_dir()?,
        metadata_directory,
        uv_version::version(),
    )?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}

pub(crate) fn get_requires_for_build_editable() -> Result<ExitStatus> {
    todo!()
}

pub(crate) fn prepare_metadata_for_build_editable(_wheel_directory: &Path) -> Result<ExitStatus> {
    todo!()
}
