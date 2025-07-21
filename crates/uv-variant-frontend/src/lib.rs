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
use tracing::{Instrument, debug, info_span};

pub use crate::error::Error;
use uv_configuration::BuildOutput;
use uv_distribution_types::Requirement;
use uv_fs::{PythonExt, Simplified};
use uv_preview::Preview;
use uv_python::{Interpreter, PythonEnvironment};
use uv_static::EnvVars;
use uv_types::{BuildContext, BuildStack, VariantsTrait};
use uv_variants::VariantProviderOutput;
use uv_variants::variants_json::{Provider, VariantPropertyType};
use uv_virtualenv::OnExisting;

pub struct VariantBuild {
    temp_dir: TempDir,
    /// The backend to use.
    backend_name: String,
    /// The backend to use.
    backend: Provider,
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

impl VariantsTrait for VariantBuild {
    async fn query(
        &self,
        known_properties: &[VariantPropertyType],
    ) -> anyhow::Result<VariantProviderOutput> {
        Ok(self.build(known_properties).await?)
    }
}

impl VariantBuild {
    /// Create a virtual environment in which to run a variant provider.
    pub async fn setup(
        backend_name: String,
        backend: &Provider,
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
            // This is a fresh temp dir
            OnExisting::Fail,
            false,
            false,
            false,
            Preview::default(), // TODO(konsti)
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
            .map_err(|err| {
                Error::RequirementsResolve("`variant.providers.requires`", err.into())
            })?;
        build_context
            .install(&resolved_requirements, &venv, &BuildStack::empty())
            .await
            .map_err(|err| {
                Error::RequirementsInstall("`variant.providers.requires`", err.into())
            })?;

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
            backend_name,
            backend: backend.clone(),
            venv,
            level,
            modified_path,
            environment_variables,
            runner,
        })
    }

    /// Run a variant provider to infer compatible variants.
    pub async fn build(
        &self,
        known_properties: &[VariantPropertyType],
    ) -> Result<VariantProviderOutput, Error> {
        // Write the hook output to a file so that we can read it back reliably.
        let out_file = self.temp_dir.path().join("output.json");
        let in_file = self.temp_dir.path().join("input.json");
        let in_writer = fs_err::File::create(&in_file)?;
        serde_json::to_writer(in_writer, known_properties)?;

        // Construct the appropriate build script based on the build kind.
        let script = formatdoc! {
            r#"
            {backend}

            import json

            if backend.dynamic:
                class VariantPropertyType:
                    namespace: str
                    feature: str
                    value: str

                    def __init__(self, namespace: str, feature: str, value: str):
                        self.namespace = namespace
                        self.feature = feature
                        self.value = value

                with open("{in_file}") as fp:
                    known_properties = json.load(fp)

                # Filter to the namespace of the plugin.
                filtered_properties = []
                for known_property in known_properties:
                    # We don't know the namespace ahead of time, so the frontend passes all properties.
                    if known_property["namespace"] != backend.namespace:
                        continue
                    filtered_properties.append(VariantPropertyType(**known_property))
                known_properties = frozenset(filtered_properties)
            else:
                known_properties = None

            configs = backend.get_supported_configs(known_properties)
            features = {{config.name: config.values for config in configs}}
            output = {{"namespace": backend.namespace, "features": features}}

            with open("{out_file}", "w") as fp:
                fp.write(json.dumps(output))
            "#,
            backend = self.backend.import(&self.backend_name),
            in_file = in_file.escape_for_python(),
            out_file = out_file.escape_for_python()
        };

        let span = info_span!(
            "run_variant_provider_script",
            backend_name = self.backend_name
        );
        let output = self
            .runner
            .run_script(
                &self.venv,
                &script,
                self.temp_dir.path(),
                &self.environment_variables,
                &self.modified_path,
            )
            .instrument(span)
            .await?;
        if !output.status.success() {
            return Err(Error::from_command_output(
                format!(
                    "Call to variant backend failed in `{}`",
                    self.backend
                        .plugin_api
                        .as_deref()
                        .unwrap_or(&self.backend_name)
                ),
                &output,
                self.level,
            ));
        }

        // Read as JSON.
        let json = fs::read(&out_file).map_err(|err| {
            Error::CommandFailed(self.venv.python_executable().to_path_buf(), err)
        })?;
        let output = serde_json::from_slice::<VariantProviderOutput>(&json).map_err(|err| {
            Error::CommandFailed(self.venv.python_executable().to_path_buf(), err.into())
        })?;

        Ok(output)
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
                ));
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
