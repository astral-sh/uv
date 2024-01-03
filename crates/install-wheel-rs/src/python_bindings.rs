#![allow(clippy::format_push_string)] // I will not replace clear and infallible with fallible, io looking code

use crate::{install_wheel, Error, InstallLocation, LockedDir};
use distribution_filename::WheelFilename;
use fs_err::File;
use pyo3::create_exception;
use pyo3::types::PyModule;
use pyo3::{pyclass, pymethods, pymodule, PyErr, PyResult, Python};
use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

create_exception!(
    install_wheel_rs,
    PyWheelInstallerError,
    pyo3::exceptions::PyException
);

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        let mut accumulator = format!("Failed to install wheels: {err}");

        let mut current_err: &dyn std::error::Error = &err;
        while let Some(cause) = current_err.source() {
            accumulator.push_str(&format!("\n  Caused by: {cause}"));
            current_err = cause;
        }
        PyWheelInstallerError::new_err(accumulator)
    }
}

#[pyclass]
struct LockedVenv {
    location: InstallLocation<LockedDir>,
}

#[pymethods]
impl LockedVenv {
    #[new]
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(py: Python, venv: PathBuf) -> PyResult<Self> {
        Ok(Self {
            location: InstallLocation::new(
                LockedDir::acquire(&venv)?,
                (py.version_info().major, py.version_info().minor),
            ),
        })
    }

    pub(crate) fn install_wheel(&self, py: Python, wheel: PathBuf) -> PyResult<()> {
        // Would be nicer through https://docs.python.org/3/c-api/init.html#c.Py_GetProgramFullPath
        let sys_executable: String = py.import("sys")?.getattr("executable")?.extract()?;

        // TODO: Pass those options on to the user
        py.allow_threads(|| {
            let filename = wheel
                .file_name()
                .ok_or_else(|| Error::InvalidWheel("Expected a file".to_string()))?
                .to_string_lossy();
            let filename = WheelFilename::from_str(&filename)?;
            install_wheel(
                &self.location,
                File::open(wheel)?,
                &filename,
                None,
                None,
                true,
                true,
                &[],
                Path::new(&sys_executable),
            )
        })?;
        Ok(())
    }
}

#[pymodule]
pub(crate) fn install_wheel_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    // Good enough for now
    if env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt::init();
    } else {
        let format = tracing_subscriber::fmt::format()
            .with_level(false)
            .with_target(false)
            .without_time()
            .compact();
        tracing_subscriber::fmt().event_format(format).init();
    }
    m.add_class::<LockedVenv>()?;
    Ok(())
}
