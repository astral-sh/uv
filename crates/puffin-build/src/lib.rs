//! Build wheels from source distributions
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

use anyhow::Context;
use flate2::read::GzDecoder;
use fs_err as fs;
use fs_err::{DirEntry, File};
use gourgeist::{InterpreterInfo, Venv};
use indoc::formatdoc;
use pep508_rs::Requirement;
use pyproject_toml::PyProjectToml;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tar::Archive;
use tempfile::tempdir;
use thiserror::Error;
use tracing::{debug, instrument};
use zip::ZipArchive;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to read zip file")]
    Zip(#[from] zip::result::ZipError),
    #[error("Unsupported archive format (extension not recognized): {0}")]
    UnsupportedArchiveType(String),
    #[error("Invalid source distribution: {0}")]
    InvalidSourceDistribution(String),
    #[error("Invalid pyproject.toml")]
    PyprojectTomlInvalid(#[from] toml::de::Error),
    #[error("Failed to install requirements")]
    RequirementsInstall(#[source] anyhow::Error),
    #[error("Failed to create temporary virtual environment")]
    Gourgeist(#[from] gourgeist::Error),
    #[error("Failed to run {0}")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    BuildBackend {
        message: String,
        stdout: String,
        stderr: String,
    },
}

impl Error {
    fn from_command_output(message: String, output: &Output) -> Self {
        Self::BuildBackend {
            message,
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }
}

#[instrument(skip_all)]
fn resolve_and_install(venv: impl AsRef<Path>, requirements: &[Requirement]) -> anyhow::Result<()> {
    debug!("Calling pip to install build dependencies");
    let python = Venv::new(venv.as_ref())?.python_interpreter();
    // No error handling because we want have to replace this with the real resolver and installer
    // anyway.
    let installation = Command::new(python)
        .args(["-m", "pip", "install"])
        .args(
            requirements
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>(),
        )
        .output()
        .context("pip install failed")?;
    if !installation.status.success() {
        anyhow::bail!("Installation failed :(")
    }
    Ok(())
}

/// Returns the directory with the `pyproject.toml`/`setup.py`
#[instrument(skip_all, fields(path))]
fn extract_archive(path: &Path, extracted: &PathBuf) -> Result<PathBuf, Error> {
    // TODO(konstin): Simplify this with camino paths?
    if path.extension().is_some_and(|extension| extension == "zip") {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        archive.extract(extracted)?;
        // .tar.gz
    } else if path.extension().is_some_and(|extension| extension == "gz")
        && path.file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|extension| extension == "tar")
        })
    {
        let mut archive = Archive::new(GzDecoder::new(File::open(path)?));
        archive.unpack(extracted)?;
    } else {
        return Err(Error::UnsupportedArchiveType(
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .to_string(),
        ));
    }

    // > A .tar.gz source distribution (sdist) contains a single top-level directory called
    // > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
    // TODO(konstin): Verify the name of the directory
    let top_level = fs::read_dir(extracted)?.collect::<io::Result<Vec<DirEntry>>>()?;
    let [root] = top_level.as_slice() else {
        return Err(Error::InvalidSourceDistribution(format!(
            "The top level of the archive must only contain a list directory, but it contains {top_level:?}"
        )));
    };
    Ok(root.path())
}

#[instrument(skip(script, root))]
fn run_python_script(
    python_interpreter: &PathBuf,
    script: &String,
    root: &Path,
) -> Result<Output, Error> {
    Command::new(python_interpreter)
        .args(["-c", script])
        .current_dir(root)
        .output()
        .map_err(|err| Error::CommandFailed(python_interpreter.clone(), err))
}

/// Returns `Ok(None)` if this is not a pyproject.toml build
fn pep517_build(
    wheel_dir: &Path,
    root: &Path,
    temp_dir: &Path,
    base_python: &Path,
    data: &InterpreterInfo,
) -> Result<Option<PathBuf>, Error> {
    if !root.join("pyproject.toml").is_file() {
        // We'll try setup.py instead
        return Ok(None);
    }
    // TODO(konstin): Create bare venvs when we don't need pip anymore
    let venv = gourgeist::create_venv(temp_dir.join("venv"), base_python, data, false)?;
    let pyproject_toml: PyProjectToml =
        toml::from_str(&fs::read_to_string(root.join("pyproject.toml"))?)
            .map_err(Error::PyprojectTomlInvalid)?;
    let mut requirements = pyproject_toml.build_system.requires;
    resolve_and_install(venv.deref().as_std_path(), &requirements)
        .map_err(Error::RequirementsInstall)?;
    let Some(backend) = &pyproject_toml.build_system.build_backend else {
        // > If the pyproject.toml file is absent, or the build-backend key is missing, the
        // > source tree is not using this specification, and tools should revert to the legacy
        // > behaviour of running setup.py (either directly, or by implicitly invoking the
        // > setuptools.build_meta:__legacy__ backend).
        return Ok(None);
    };
    let backend_import = if let Some((path, object)) = backend.split_once(':') {
        format!("from {path} import {object}")
    } else {
        format!("import {backend}")
    };

    debug!("Calling `{}.get_requires_for_build_wheel()`", backend);
    let script = formatdoc! {
        r#"{} as backend
            import json
            
            if get_requires_for_build_wheel := getattr(backend, "get_requires_for_build_wheel", None):
                requires = get_requires_for_build_wheel()
            else:
                requires = []
            print(json.dumps(requires))
            "#, backend_import
    };
    let python_interpreter = venv.python_interpreter();
    let output = run_python_script(&python_interpreter, &script, root)?;
    if !output.status.success() {
        return Err(Error::from_command_output(
            "Build backend failed to determine extras requires with `get_requires_for_build_wheel`"
                .to_string(),
            &output,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let extra_requires: Vec<Requirement> =
        serde_json::from_str(stdout.lines().last().unwrap_or_default()).map_err(|err| {
            Error::from_command_output(
                format!(
                    "Build backend failed to return extras requires with \
                        `get_requires_for_build_wheel`: {err}"
                ),
                &output,
            )
        })?;
    // Some packages (such as tqdm 4.66.1) list only extra requires that have already been part of
    // the pyproject.toml requires (in this case, `wheel`). We can skip doing the whole resolution
    // and installation again.
    // TODO(konstin): Do we still need this when we have a fast resolver?
    if !extra_requires.is_empty() && !extra_requires.iter().all(|req| requirements.contains(req)) {
        debug!("Installing extra requirements for build backend");
        // TODO(konstin): Do we need to resolve them together?
        requirements.extend(extra_requires);
        resolve_and_install(&*venv, &requirements).map_err(Error::RequirementsInstall)?;
    }

    debug!("Calling `{}.build_wheel()`", backend);
    let escaped_wheel_dir = wheel_dir
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let script = formatdoc! {
        r#"{} as backend
            print(backend.build_wheel("{}"))
            "#, backend_import, escaped_wheel_dir
    };
    let output = run_python_script(&python_interpreter, &script, root)?;
    if !output.status.success() {
        return Err(Error::from_command_output(
            "Build backend failed to build wheel through `build_wheel()` ".to_string(),
            &output,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let wheel = stdout
        .lines()
        .last()
        .map(|wheel_filename| wheel_dir.join(wheel_filename));
    let Some(wheel) = wheel.filter(|wheel| wheel.is_file()) else {
        return Err(Error::from_command_output(
            "Build backend did not return the wheel filename through `build_wheel()`".to_string(),
            &output,
        ));
    };
    Ok(Some(wheel))
}

/// Build a source distribution from an archive (`.zip` or `.tar.gz`), return the location of the
/// built wheel.
///
/// The location will be inside `temp_dir`, i.e. you must use the wheel before dropping the temp
/// dir.
///
/// <https://packaging.python.org/en/latest/specifications/source-distribution-format/>
#[instrument(skip(wheel_dir, interpreter_info))]
pub fn build_sdist(
    path: &Path,
    wheel_dir: &Path,
    base_python: &Path,
    interpreter_info: &InterpreterInfo,
) -> Result<PathBuf, Error> {
    debug!("Building {}", path.display());
    // TODO(konstin): Parse and verify filenames
    let temp_dir = tempdir()?;
    let temp_dir = temp_dir.path();
    // The build scripts run with the extracted root as cwd, so they need the absolute path
    let wheel_dir = fs::canonicalize(wheel_dir)?;

    let extracted = temp_dir.join("extracted");
    let root = extract_archive(path, &extracted)?;

    let wheel = pep517_build(&wheel_dir, &root, temp_dir, base_python, interpreter_info)?;

    if let Some(wheel) = wheel {
        Ok(wheel)
    } else if root.join("setup.py").is_file() {
        let venv =
            gourgeist::create_venv(temp_dir.join("venv"), base_python, interpreter_info, false)?;
        let python_interpreter = venv.python_interpreter();
        let output = Command::new(&python_interpreter)
            .args(["setup.py", "bdist_wheel"])
            .current_dir(&root)
            .output()
            .map_err(|err| Error::CommandFailed(python_interpreter.clone(), err))?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                "Failed building wheel through setup.py".to_string(),
                &output,
            ));
        }
        let dist = fs::read_dir(root.join("dist"))?;
        let dist_dir = dist.collect::<io::Result<Vec<DirEntry>>>()?;
        let [dist_wheel] = dist_dir.as_slice() else {
            return Err(Error::from_command_output(
                format!(
                    "Expected exactly wheel in `dist/` after invoking setup.py, found {dist_dir:?}"
                ),
                &output,
            ));
        };
        // TODO(konstin): Faster copy such as reflink? Or maybe don't really let the user pick the target dir
        let wheel = wheel_dir.join(dist_wheel.file_name());
        fs::copy(dist_wheel.path(), &wheel)?;
        // TODO(konstin): Check wheel filename
        Ok(wheel)
    } else {
        Err(Error::InvalidSourceDistribution(
            "The archive contains neither a pyproject.toml or a setup.py at the top level"
                .to_string(),
        ))
    }
}
