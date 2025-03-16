//! Detect compatible variants from a variant provider.

mod error;

use std::ffi::OsString;
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::{env, iter};

use fs_err as fs;
use indoc::formatdoc;
use rustc_hash::FxHashMap;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::debug;

use uv_configuration::BuildOutput;
use uv_fs::{PythonExt, Simplified};
use uv_pypi_types::{Requirement, VariantProviderBackend};
use uv_python::{Interpreter, PythonEnvironment};
use uv_static::EnvVars;
use uv_types::{BuildContext, BuildStack};
use uv_variants::VariantProviderConfig;

pub use crate::error::Error;

pub struct VariantBuild {
    temp_dir: TempDir,
    /// The backend to use.
    backend: VariantProviderBackend,
    /// The virtual environment in which to build the source distribution.
    venv: PythonEnvironment,
    /// Whether to send build output to `stderr` or `tracing`, etc.
    level: BuildOutput,
    /// Modified PATH that contains the `venv_bin`, `user_path` and `system_path` variables in that
    /// order.
    modified_path: OsString,
    /// Environment variables to be passed in.
    environment_variables: FxHashMap<OsString, OsString>,
    /// Runner for Python scripts.
    runner: PythonRunner,
}

impl VariantBuild {
    /// Create a virtual environment in which to run a variant provider.
    pub async fn setup(
        backend: VariantProviderBackend,
        interpreter: &Interpreter,
        build_context: &impl BuildContext,
        mut environment_variables: FxHashMap<OsString, OsString>,
        level: BuildOutput,
        concurrent_builds: usize,
    ) -> Result<Self, Error> {
        let temp_dir = build_context.cache().venv_dir()?;

        // Create a virtual environment.
        let venv = uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter.clone(),
            uv_virtualenv::Prompt::None,
            false,
            false,
            false,
            false,
        )?;

        // Resolve and install the provider requirements.
        let requirements = backend
            .requires
            .iter()
            .cloned()
            .map(Requirement::from)
            .collect::<Vec<_>>();
        let resolved_requirements = build_context
            .resolve(&requirements, &BuildStack::empty())
            .await
            .map_err(|err| Error::RequirementsResolve("`build-system.requires`", err.into()))?;
        build_context
            .install(&resolved_requirements, &venv, &BuildStack::empty())
            .await
            .map_err(|err| Error::RequirementsInstall("`build-system.requires`", err.into()))?;

        // Figure out what the modified path should be, and remove the PATH variable from the
        // environment variables if it's there.
        let user_path = environment_variables.remove(&OsString::from(EnvVars::PATH));

        // See if there is an OS PATH variable.
        let os_path = env::var_os(EnvVars::PATH);

        // Prepend the user supplied PATH to the existing OS PATH.
        let modified_path = if let Some(user_path) = user_path {
            match os_path {
                // Prepend the user supplied PATH to the existing PATH.
                Some(env_path) => {
                    let user_path = PathBuf::from(user_path);
                    let new_path = env::split_paths(&user_path).chain(env::split_paths(&env_path));
                    Some(env::join_paths(new_path).map_err(Error::BuildScriptPath)?)
                }
                // Use the user supplied PATH.
                None => Some(user_path),
            }
        } else {
            os_path
        };

        // Prepend the venv bin directory to the modified path.
        let modified_path = if let Some(path) = modified_path {
            let venv_path = iter::once(venv.scripts().to_path_buf()).chain(env::split_paths(&path));
            env::join_paths(venv_path).map_err(Error::BuildScriptPath)?
        } else {
            OsString::from(venv.scripts())
        };

        let runner = PythonRunner::new(concurrent_builds, level);

        Ok(Self {
            temp_dir,
            backend,
            venv,
            level,
            modified_path,
            environment_variables,
            runner,
        })
    }

    /// Run a variant provider to infer compatible variants.
    pub async fn build(&self) -> Result<VariantProviderConfig, Error> {
        // Write the hook output to a file so that we can read it back reliably.
        let outfile = self.temp_dir.path().join("output.json");

        // Construct the appropriate build script based on the build kind.
        let script = formatdoc! {
            r#"
            {}

            with open("{}", "w") as fp:
                import json
                fp.write(json.dumps(backend()))
            "#,
            self.backend.import(),
            outfile.escape_for_python()
        };

        let output = self
            .runner
            .run_script(
                &self.venv,
                &script,
                self.temp_dir.path(),
                &self.environment_variables,
                &self.modified_path,
            )
            .await?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                format!(
                    "Call to variant backend failed in `{}`",
                    self.backend.backend
                ),
                &output,
                self.level,
            ));
        }

        // Read as JSON.
        let json = fs::read(&outfile).map_err(|err| {
            Error::CommandFailed(self.venv.python_executable().to_path_buf(), err)
        })?;
        let config = serde_json::from_slice::<VariantProviderConfig>(&json).map_err(|err| {
            Error::CommandFailed(self.venv.python_executable().to_path_buf(), err.into())
        })?;

        Ok(config)
    }
}

/// A runner that manages the execution of external python processes with a
/// concurrency limit.
#[derive(Debug)]
struct PythonRunner {
    control: Semaphore,
    level: BuildOutput,
}

#[derive(Debug)]
struct PythonRunnerOutput {
    stdout: Vec<String>,
    stderr: Vec<String>,
    status: ExitStatus,
}

impl PythonRunner {
    /// Create a `PythonRunner` with the provided concurrency limit and output level.
    fn new(concurrency: usize, level: BuildOutput) -> Self {
        Self {
            control: Semaphore::new(concurrency),
            level,
        }
    }

    /// Spawn a process that runs a python script in the provided environment.
    ///
    /// If the concurrency limit has been reached this method will wait until a pending
    /// script completes before spawning this one.
    ///
    /// Note: It is the caller's responsibility to create an informative span.
    async fn run_script(
        &self,
        venv: &PythonEnvironment,
        script: &str,
        source_tree: &Path,
        environment_variables: &FxHashMap<OsString, OsString>,
        modified_path: &OsString,
    ) -> Result<PythonRunnerOutput, Error> {
        /// Read lines from a reader and store them in a buffer.
        async fn read_from(
            mut reader: tokio::io::Split<tokio::io::BufReader<impl tokio::io::AsyncRead + Unpin>>,
            mut printer: Printer,
            buffer: &mut Vec<String>,
        ) -> io::Result<()> {
            loop {
                match reader.next_segment().await? {
                    Some(line_buf) => {
                        let line_buf = line_buf.strip_suffix(b"\r").unwrap_or(&line_buf);
                        let line = String::from_utf8_lossy(line_buf).into();
                        let _ = write!(printer, "{line}");
                        buffer.push(line);
                    }
                    None => return Ok(()),
                }
            }
        }

        let _permit = self.control.acquire().await.unwrap();

        let mut child = Command::new(venv.python_executable())
            .args(["-c", script])
            .current_dir(source_tree.simplified())
            .envs(environment_variables)
            .env(EnvVars::PATH, modified_path)
            .env(EnvVars::VIRTUAL_ENV, venv.root())
            .env(EnvVars::CLICOLOR_FORCE, "1")
            .env(EnvVars::PYTHONIOENCODING, "utf-8:backslashreplace")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))?;

        // Create buffers to capture `stdout` and `stderr`.
        let mut stdout_buf = Vec::with_capacity(1024);
        let mut stderr_buf = Vec::with_capacity(1024);

        // Create separate readers for `stdout` and `stderr`.
        let stdout_reader = tokio::io::BufReader::new(child.stdout.take().unwrap()).split(b'\n');
        let stderr_reader = tokio::io::BufReader::new(child.stderr.take().unwrap()).split(b'\n');

        // Asynchronously read from the in-memory pipes.
        let printer = Printer::from(self.level);
        let result = tokio::join!(
            read_from(stdout_reader, printer, &mut stdout_buf),
            read_from(stderr_reader, printer, &mut stderr_buf),
        );
        match result {
            (Ok(()), Ok(())) => {}
            (Err(err), _) | (_, Err(err)) => {
                return Err(Error::CommandFailed(
                    venv.python_executable().to_path_buf(),
                    err,
                ))
            }
        }

        // Wait for the child process to finish.
        let status = child
            .wait()
            .await
            .map_err(|err| Error::CommandFailed(venv.python_executable().to_path_buf(), err))?;

        Ok(PythonRunnerOutput {
            stdout: stdout_buf,
            stderr: stderr_buf,
            status,
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Printer {
    /// Send the provider output to `stderr`.
    Stderr,
    /// Send the provider output to `tracing`.
    Debug,
    /// Hide the provider output.
    Quiet,
}

impl From<BuildOutput> for Printer {
    fn from(output: BuildOutput) -> Self {
        match output {
            BuildOutput::Stderr => Self::Stderr,
            BuildOutput::Debug => Self::Debug,
            BuildOutput::Quiet => Self::Quiet,
        }
    }
}

impl Write for Printer {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self {
            Self::Stderr => {
                anstream::eprintln!("{s}");
            }
            Self::Debug => {
                debug!("{s}");
            }
            Self::Quiet => {}
        }
        Ok(())
    }
}
