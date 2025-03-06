use crate::commands::ExitStatus;
use anyhow::{Context, Result};
use std::env;
use std::io::Write;
use std::path::Path;

/// PEP 517 hook to build a source distribution.
pub(crate) fn build_sdist(sdist_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::build_source_dist(
        &env::current_dir()?,
        sdist_directory,
        uv_version::version(),
    )?;
    // Tell the build frontend about the name of the artifact we built
    writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
    Ok(ExitStatus::Success)
}

/// PEP 517 hook to build a wheel.
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
    // Tell the build frontend about the name of the artifact we built
    writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
    Ok(ExitStatus::Success)
}

/// PEP 660 hook to build a wheel.
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
    // Tell the build frontend about the name of the artifact we built
    writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
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

/// PEP 517 hook to just emit metadata through `.dist-info`.
pub(crate) fn prepare_metadata_for_build_wheel(metadata_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::metadata(
        &env::current_dir()?,
        metadata_directory,
        uv_version::version(),
    )?;
    // Tell the build frontend about the name of the artifact we built
    writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
    Ok(ExitStatus::Success)
}

/// Not used from Python code, exists for symmetry with PEP 660.
pub(crate) fn get_requires_for_build_editable() -> Result<ExitStatus> {
    unimplemented!("uv does not support extra requires")
}

/// PEP 660 hook to just emit metadata through `.dist-info`.
pub(crate) fn prepare_metadata_for_build_editable(metadata_directory: &Path) -> Result<ExitStatus> {
    let filename = uv_build_backend::metadata(
        &env::current_dir()?,
        metadata_directory,
        uv_version::version(),
    )?;
    // Tell the build frontend about the name of the artifact we built
    writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
    Ok(ExitStatus::Success)
}
