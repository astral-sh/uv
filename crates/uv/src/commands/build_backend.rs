#![allow(clippy::print_stdout)]

use crate::commands::ExitStatus;
use anyhow::Result;
use std::env;
use std::path::Path;
use uv_build_backend::SourceDistSettings;

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
        uv_version::version(),
    )?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}

pub(crate) fn build_editable(
    wheel_directory: &Path,
    metadata_directory: Option<&Path>,
) -> Result<ExitStatus> {
    let filename = uv_build_backend::build_editable(
        &env::current_dir()?,
        wheel_directory,
        metadata_directory,
        uv_version::version(),
    )?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}

/// Not used from Python code, exists for symmetry with PEP 517.
pub(crate) fn get_requires_for_build_sdist() -> Result<ExitStatus> {
    unimplemented!("uv does not support extra requires")
}

/// Not used from Python code, exists for symmetry with PEP 517.
pub(crate) fn get_requires_for_build_wheel() -> Result<ExitStatus> {
    unimplemented!("uv does not support extra requires")
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

/// Not used from Python code, exists for symmetry with PEP 660.
pub(crate) fn get_requires_for_build_editable() -> Result<ExitStatus> {
    unimplemented!("uv does not support extra requires")
}

pub(crate) fn prepare_metadata_for_build_editable(metadata_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::metadata(
        &env::current_dir()?,
        metadata_directory,
        uv_version::version(),
    )?;
    println!("{filename}");
    Ok(ExitStatus::Success)
}
