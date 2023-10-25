//! Build wheels from source distributions
//!
//! <https://packaging.python.org/en/latest/specifications/source-distribution-format/>

use std::io;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str::FromStr;

use flate2::read::GzDecoder;
use fs_err as fs;
use fs_err::{DirEntry, File};
use indoc::formatdoc;
use pyproject_toml::PyProjectToml;
use tar::Archive;
use tempfile::{tempdir, TempDir};
use thiserror::Error;
use tracing::{debug, instrument};
use zip::ZipArchive;

use pep508_rs::Requirement;
use puffin_interpreter::{InterpreterInfo, Virtualenv};
use puffin_traits::BuildContext;

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
    InvalidPyprojectToml(#[from] toml::de::Error),
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] anyhow::Error),
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

/// `[build-backend]` from pyproject.toml
struct Pep517Backend {
    /// The build backend string such as `setuptools.build_meta:__legacy__` or `maturin` from
    /// `build-backend.backend` in pyproject.toml
    backend: String,
    /// `build-backend.requirements` in pyproject.toml
    requirements: Vec<Requirement>,
}

impl Pep517Backend {
    fn backend_import(&self) -> String {
        if let Some((path, object)) = self.backend.split_once(':') {
            format!("from {path} import {object}")
        } else {
            format!("import {}", self.backend)
        }
    }
}

/// Holds the state through a series of PEP 517 frontend to backend calls or a single setup.py
/// invocation.
///
/// This keeps both the temp dir and the result of a potential `prepare_metadata_for_build_wheel`
/// call which changes how we call `build_wheel`.
pub struct SourceDistributionBuilder {
    temp_dir: TempDir,
    source_tree: PathBuf,
    /// `Some` if this is a PEP 517 build
    pep517_backend: Option<Pep517Backend>,
    venv: Virtualenv,
    /// Populated if `prepare_metadata_for_build_wheel` was called.
    ///
    /// > If the build frontend has previously called prepare_metadata_for_build_wheel and depends
    /// > on the wheel resulting from this call to have metadata matching this earlier call, then
    /// > it should provide the path to the created .dist-info directory as the metadata_directory
    /// > argument. If this argument is provided, then build_wheel MUST produce a wheel with
    /// > identical metadata. The directory passed in by the build frontend MUST be identical to the
    /// > directory created by prepare_metadata_for_build_wheel, including any unrecognized files
    /// > it created.
    metadata_directory: Option<PathBuf>,
}

impl SourceDistributionBuilder {
    /// Extract the source distribution and create a venv with the required packages
    pub async fn setup(
        sdist: &Path,
        interpreter_info: &InterpreterInfo,
        build_context: &impl BuildContext,
    ) -> Result<SourceDistributionBuilder, Error> {
        let temp_dir = tempdir()?;

        // TODO(konstin): Parse and verify filenames
        debug!("Unpacking for build {}", sdist.display());
        let extracted = temp_dir.path().join("extracted");
        let source_tree = extract_archive(sdist, &extracted)?;

        // Check if we have a PEP 517 build, otherwise we'll fall back to setup.py
        let mut pep517 = None;
        if source_tree.join("pyproject.toml").is_file() {
            let pyproject_toml: PyProjectToml =
                toml::from_str(&fs::read_to_string(source_tree.join("pyproject.toml"))?)
                    .map_err(Error::InvalidPyprojectToml)?;
            // > If the pyproject.toml file is absent, or the build-backend key is missing, the
            // > source tree is not using this specification, and tools should revert to the legacy
            // > behaviour of running setup.py (either directly, or by implicitly invoking the
            // > setuptools.build_meta:__legacy__ backend).
            if let Some(backend) = pyproject_toml.build_system.build_backend {
                pep517 = Some(Pep517Backend {
                    backend,
                    requirements: pyproject_toml.build_system.requires,
                });
            };
            if pyproject_toml.build_system.backend_path.is_some() {
                todo!("backend-path is not supported yet")
            }
        }

        let venv = if let Some(pep517_backend) = &pep517 {
            create_pep517_build_environment(
                temp_dir.path(),
                &source_tree,
                interpreter_info,
                pep517_backend,
                build_context,
            )
            .await?
        } else {
            if !source_tree.join("setup.py").is_file() {
                return Err(Error::InvalidSourceDistribution(
                    "The archive contains neither a pyproject.toml or a setup.py at the top level"
                        .to_string(),
                ));
            }
            let venv = gourgeist::create_venv(
                temp_dir.path().join("venv"),
                build_context.base_python(),
                interpreter_info,
                true,
            )?;
            // TODO(konstin): Resolve those once globally and cache per puffin invocation
            let requirements = [
                Requirement::from_str("wheel").unwrap(),
                Requirement::from_str("setuptools").unwrap(),
                Requirement::from_str("pip").unwrap(),
            ];
            let resolved_requirements = build_context
                .resolve(&requirements)
                .await
                .map_err(|err| Error::RequirementsInstall("setup.py build", err))?;
            build_context
                .install(&resolved_requirements, &venv)
                .await
                .map_err(|err| Error::RequirementsInstall("setup.py build", err))?;
            venv
        };

        Ok(Self {
            temp_dir,
            source_tree,
            pep517_backend: pep517,
            venv,
            metadata_directory: None,
        })
    }

    /// Try calling `prepare_metadata_for_build_wheel` to get the metadata without executing the
    /// actual build
    ///
    /// TODO(konstin): Return the actual metadata instead of the dist-info dir
    pub fn get_metadata_without_build(&mut self) -> Result<Option<&Path>, Error> {
        // setup.py builds don't support this
        let Some(pep517_backend) = &self.pep517_backend else {
            return Ok(None);
        };

        let metadata_directory = self.temp_dir.path().join("metadata_directory");
        fs::create_dir(&metadata_directory)?;

        debug!(
            "Calling `{}.prepare_metadata_for_build_wheel()`",
            pep517_backend.backend
        );
        let script = formatdoc! {
            r#"{} as backend
            import json
            
            if prepare_metadata_for_build_wheel := getattr(backend, "prepare_metadata_for_build_wheel", None):
                print(prepare_metadata_for_build_wheel("{}"))
            else:
                print()
            "#, pep517_backend.backend_import(), escape_path_for_python(&metadata_directory)
        };
        let output = run_python_script(&self.venv.python_executable(), &script, &self.source_tree)?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                "Build backend failed to determine metadata through `prepare_metadata_for_build_wheel`".to_string(),
                &output,
            ));
        }
        let message = output
            .stdout
            .lines()
            .last()
            // flatten is nightly only :/
            .transpose()
            .map_err(|err| err.to_string())
            .and_then(|last_line| last_line.ok_or("Missing message".to_string()))
            .map_err(|err| {
                Error::from_command_output(
                    format!(
                        "Build backend failed to return metadata directory with \
                        `prepare_metadata_for_build_wheel`: {err}"
                    ),
                    &output,
                )
            })?;
        if message.is_empty() {
            return Ok(None);
        }
        self.metadata_directory = Some(metadata_directory.join(message));
        return Ok(self.metadata_directory.as_deref());
    }

    /// Build a source distribution from an archive (`.zip` or `.tar.gz`), return the location of the
    /// built wheel.
    ///
    /// The location will be inside `temp_dir`, i.e. you must use the wheel before dropping the temp
    /// dir.
    ///
    /// <https://packaging.python.org/en/latest/specifications/source-distribution-format/>
    #[instrument(skip(self))]
    pub fn build(&self, wheel_dir: &Path) -> Result<String, Error> {
        // The build scripts run with the extracted root as cwd, so they need the absolute path
        let wheel_dir = fs::canonicalize(wheel_dir)?;

        if let Some(pep517_backend) = &self.pep517_backend {
            self.pep517_build_wheel(&wheel_dir, pep517_backend)
        } else {
            // We checked earlier that setup.py exists
            let python_interpreter = self.venv.python_executable();
            let output = Command::new(&python_interpreter)
                .args(["setup.py", "bdist_wheel"])
                .current_dir(&self.source_tree)
                .output()
                .map_err(|err| Error::CommandFailed(python_interpreter, err))?;
            if !output.status.success() {
                return Err(Error::from_command_output(
                    "Failed building wheel through setup.py".to_string(),
                    &output,
                ));
            }
            let dist = fs::read_dir(self.source_tree.join("dist"))?;
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
            fs::copy(dist_wheel.path(), wheel)?;
            // TODO(konstin): Check wheel filename
            Ok(dist_wheel.file_name().to_string_lossy().to_string())
        }
    }

    fn pep517_build_wheel(
        &self,
        wheel_dir: &Path,
        pep517_backend: &Pep517Backend,
    ) -> Result<String, Error> {
        let metadata_directory = self
            .metadata_directory
            .as_deref()
            .map_or("None".to_string(), |path| {
                format!(r#""{}""#, escape_path_for_python(path))
            });
        debug!(
            "Calling `{}.build_wheel(metadata_directory={})`",
            pep517_backend.backend, metadata_directory
        );
        let escaped_wheel_dir = escape_path_for_python(wheel_dir);
        let script = formatdoc! {
            r#"{} as backend
            print(backend.build_wheel("{}", metadata_directory={}))
            "#, pep517_backend.backend_import(), escaped_wheel_dir, metadata_directory 
        };
        let output = run_python_script(&self.venv.python_executable(), &script, &self.source_tree)?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                "Build backend failed to build wheel through `build_wheel()` ".to_string(),
                &output,
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let distribution_filename = stdout.lines().last();
        let Some(distribution_filename) =
            distribution_filename.filter(|wheel| wheel_dir.join(wheel).is_file())
        else {
            return Err(Error::from_command_output(
                "Build backend did not return the wheel filename through `build_wheel()`"
                    .to_string(),
                &output,
            ));
        };
        Ok(distribution_filename.to_string())
    }
}

fn escape_path_for_python(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Not a method because we call it before the builder is completely initialized
async fn create_pep517_build_environment(
    root: &Path,
    source_tree: &Path,
    data: &InterpreterInfo,
    pep517_backend: &Pep517Backend,
    build_context: &impl BuildContext,
) -> Result<Virtualenv, Error> {
    let venv = gourgeist::create_venv(root.join(".venv"), build_context.base_python(), data, true)?;
    let resolved_requirements = build_context
        .resolve(&pep517_backend.requirements)
        .await
        .map_err(|err| Error::RequirementsInstall("get_requires_for_build_wheel", err))?;
    build_context
        .install(&resolved_requirements, &venv)
        .await
        .map_err(|err| Error::RequirementsInstall("get_requires_for_build_wheel", err))?;

    debug!(
        "Calling `{}.get_requires_for_build_wheel()`",
        pep517_backend.backend
    );
    let script = formatdoc! {
        r#"{} as backend
            import json
            
            if get_requires_for_build_wheel := getattr(backend, "get_requires_for_build_wheel", None):
                requires = get_requires_for_build_wheel()
            else:
                requires = []
            print(json.dumps(requires))
        "#, pep517_backend.backend_import()
    };
    let output = run_python_script(&venv.python_executable(), &script, source_tree)?;
    if !output.status.success() {
        return Err(Error::from_command_output(
            "Build backend failed to determine extras requires with `get_requires_for_build_wheel`"
                .to_string(),
            &output,
        ));
    }
    let extra_requires = output
        .stdout
        .lines()
        .last()
        // flatten is nightly only :/
        .transpose()
        .map_err(|err| err.to_string())
        .and_then(|last_line| last_line.ok_or("Missing message".to_string()))
        .and_then(|message| serde_json::from_str(&message).map_err(|err| err.to_string()));
    let extra_requires: Vec<Requirement> = extra_requires.map_err(|err| {
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
    if !extra_requires.is_empty()
        && !extra_requires
            .iter()
            .all(|req| pep517_backend.requirements.contains(req))
    {
        debug!("Installing extra requirements for build backend");
        // TODO(konstin): Do we need to resolve them together?
        let requirements: Vec<Requirement> = pep517_backend
            .requirements
            .iter()
            .cloned()
            .chain(extra_requires)
            .collect();
        let resolved_requirements = build_context
            .resolve(&requirements)
            .await
            .map_err(|err| Error::RequirementsInstall("build-system.requires", err))?;

        build_context
            .install(&resolved_requirements, &venv)
            .await
            .map_err(|err| Error::RequirementsInstall("build-system.requires", err))?;
    }
    Ok(venv)
}
/// Returns the directory with the `pyproject.toml`/`setup.py`
#[instrument(skip_all, fields(path))]
fn extract_archive(sdist: &Path, extracted: &PathBuf) -> Result<PathBuf, Error> {
    // TODO(konstin): Simplify this with camino paths?
    if sdist
        .extension()
        .is_some_and(|extension| extension == "zip")
    {
        let mut archive = ZipArchive::new(File::open(sdist)?)?;
        archive.extract(extracted)?;
        // .tar.gz
    } else if sdist.extension().is_some_and(|extension| extension == "gz")
        && sdist.file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|extension| extension == "tar")
        })
    {
        let mut archive = Archive::new(GzDecoder::new(File::open(sdist)?));
        archive.unpack(extracted)?;
    } else {
        return Err(Error::UnsupportedArchiveType(
            sdist
                .file_name()
                .unwrap_or(sdist.as_os_str())
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

#[instrument(skip(script, source_tree))]
fn run_python_script(
    python_interpreter: &Path,
    script: &str,
    source_tree: &Path,
) -> Result<Output, Error> {
    Command::new(python_interpreter)
        .args(["-c", script])
        .current_dir(source_tree)
        .output()
        .map_err(|err| Error::CommandFailed(python_interpreter.to_path_buf(), err))
}
