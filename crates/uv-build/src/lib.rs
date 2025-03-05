use std::path::PathBuf;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Warn if config settings are provided since they are not supported
#[allow(clippy::print_stderr)]
fn warn_config_settings(config_settings: Option<&Bound<'_, PyDict>>) {
    if config_settings.is_some() {
        eprintln!("Warning: config settings are not supported and will be ignored");
    }
}

/// Build a source distribution (sdist) for the package
///
/// PEP 517 hook `build_sdist`
#[pyfunction]
#[pyo3(signature = (sdist_directory, config_settings=None))]
fn build_sdist(
    sdist_directory: String,
    config_settings: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    warn_config_settings(config_settings);

    let sdist_dir = PathBuf::from(sdist_directory);
    let current_dir = std::env::current_dir()?;

    let filename =
        uv_build_backend::build_source_dist(&current_dir, &sdist_dir, uv_version::version())
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;

    Ok(filename.to_string())
}

/// Build a wheel distribution for the package
///
/// PEP 517 hook `build_wheel`
#[pyfunction]
#[pyo3(signature = (wheel_directory, config_settings=None, metadata_directory=None))]
fn build_wheel(
    wheel_directory: String,
    config_settings: Option<&Bound<'_, PyDict>>,
    metadata_directory: Option<String>,
) -> PyResult<String> {
    warn_config_settings(config_settings);

    let wheel_dir = PathBuf::from(wheel_directory);
    let metadata_dir = metadata_directory.map(PathBuf::from);
    let current_dir = std::env::current_dir()?;

    let filename = uv_build_backend::build_wheel(
        &current_dir,
        &wheel_dir,
        metadata_dir.as_deref(),
        uv_version::version(),
    )
    .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;

    Ok(filename.to_string())
}

/// Get build requirements for sdist
///
/// PEP 517 hook `get_requires_for_build_sdist`
#[pyfunction]
#[pyo3(signature = (config_settings=None))]
fn get_requires_for_build_sdist(config_settings: Option<&Bound<'_, PyDict>>) -> Vec<String> {
    warn_config_settings(config_settings);
    vec![]
}

/// Get build requirements for wheel
///
/// PEP 517 hook `get_requires_for_build_wheel`
#[pyfunction]
#[pyo3(signature = (config_settings=None))]
fn get_requires_for_build_wheel(config_settings: Option<&Bound<'_, PyDict>>) -> Vec<String> {
    warn_config_settings(config_settings);
    vec![]
}

/// Prepare metadata for wheel build
///
/// PEP 517 hook `prepare_metadata_for_build_wheel`
#[pyfunction]
#[pyo3(signature = (metadata_directory, config_settings=None))]
fn prepare_metadata_for_build_wheel(
    metadata_directory: String,
    config_settings: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    warn_config_settings(config_settings);

    let metadata_dir = PathBuf::from(metadata_directory);
    let current_dir = std::env::current_dir()?;

    let filename = uv_build_backend::metadata(&current_dir, &metadata_dir, uv_version::version())
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;

    Ok(filename)
}

/// Build an editable installation
///
/// PEP 660 hook `build_editable`
#[pyfunction]
#[pyo3(signature = (wheel_directory, config_settings=None, metadata_directory=None))]
fn build_editable(
    wheel_directory: String,
    config_settings: Option<&Bound<'_, PyDict>>,
    metadata_directory: Option<String>,
) -> PyResult<String> {
    warn_config_settings(config_settings);

    let wheel_dir = PathBuf::from(wheel_directory);
    let metadata_dir = metadata_directory.map(PathBuf::from);
    let current_dir = std::env::current_dir()?;

    let filename = uv_build_backend::build_editable(
        &current_dir,
        &wheel_dir,
        metadata_dir.as_deref(),
        uv_version::version(),
    )
    .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;

    Ok(filename.to_string())
}

/// Get build requirements for editable install
///
/// PEP 660 hook `get_requires_for_build_editable`
#[pyfunction]
#[pyo3(signature = (config_settings=None))]
fn get_requires_for_build_editable(config_settings: Option<&Bound<'_, PyDict>>) -> Vec<String> {
    warn_config_settings(config_settings);
    vec![]
}

/// Prepare metadata for editable build
///
/// PEP 660 hook `prepare_metadata_for_build_editable`
#[pyfunction]
#[pyo3(signature = (metadata_directory, config_settings=None))]
fn prepare_metadata_for_build_editable(
    metadata_directory: String,
    config_settings: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    warn_config_settings(config_settings);

    let metadata_dir = PathBuf::from(metadata_directory);
    let current_dir = std::env::current_dir()?;

    let filename = uv_build_backend::metadata(&current_dir, &metadata_dir, uv_version::version())
        .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;

    Ok(filename)
}

/// Python module implementing PEP 517 and PEP 660 build backend
#[pymodule]
fn uv_build(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_sdist, m)?)?;
    m.add_function(wrap_pyfunction!(build_wheel, m)?)?;
    m.add_function(wrap_pyfunction!(get_requires_for_build_sdist, m)?)?;
    m.add_function(wrap_pyfunction!(get_requires_for_build_wheel, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_metadata_for_build_wheel, m)?)?;
    m.add_function(wrap_pyfunction!(build_editable, m)?)?;
    m.add_function(wrap_pyfunction!(get_requires_for_build_editable, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_metadata_for_build_editable, m)?)?;
    m.add("__version__", uv_version::version())?;
    Ok(())
}
