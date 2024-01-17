use std::env;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::{self, BufReader};
use tokio::io::{AsyncWriteExt, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{debug, error};

use crate::{BuildKind, Pep517Backend};
use pep508_rs::Requirement;
use puffin_interpreter::Virtualenv;
use thiserror::Error;

static HOOKD_SOURCE: &'static str = include_str!("hookd.py");

#[derive(Error, Debug)]
pub enum DaemonError {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Unknown response from build daemon: {0}")]
    UnknownResponse(String),
    #[error("Unexpected response from build daemon: {0:?}")]
    UnexpectedResponse(DaemonResponse),
    #[error("Unexpected empty response from build daemon")]
    EmptyResponse,
    #[error("Unknown error kind reported by build daemon: {0}")]
    UnknownErrorKind(String),
    #[error("Build daemon never reported ready")]
    NotReady,
    #[error("Build daemon died unexpectedly")]
    Closed,
    #[error("Build daemon crashed with fatal error. {kind}: {message}\n{traceback}")]
    Crashed {
        kind: String,
        message: String,
        traceback: String,
    },
    #[error("Build daemon encountered error running hook: {0}\n{1}")]
    HookError(String, String),
    #[error("Build daemon encountered error parsing hook result {0}: {1}")]
    InvalidResult(String, String),
}

/// Possible responses from the daemon
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DaemonResponse {
    Debug(String),
    Error(HookErrorKind, String),
    Traceback(String),
    Ok(String),
    Stderr(PathBuf),
    Stdout(PathBuf),
    Expect(String),
    Ready,
    Fatal(String, String),
    Shutdown,
}

impl DaemonResponse {
    fn try_from_str(line: &str) -> Result<Self, DaemonError> {
        // Split on the first two spaces
        let mut parts = line.splitn(3, ' ');
        if let Some(kind) = parts.next() {
            let response = match kind {
                "DEBUG" => Self::Debug(parts.collect::<Vec<&str>>().join(" ")),
                "EXPECT" => Self::Expect(parts.collect::<Vec<&str>>().join(" ")),
                "OK" => Self::Ok(parts.collect::<Vec<&str>>().join(" ")),
                "TRACEBACK" => Self::Traceback(
                    parts
                        .collect::<Vec<&str>>()
                        .join(" ")
                        .replace("\\n", "\n")
                        .replace("\n\n", "\n"),
                ),
                "ERROR" => Self::Error(
                    HookErrorKind::try_from_str(parts.next().unwrap())?,
                    parts.collect::<Vec<&str>>().join(" "),
                ),
                "STDOUT" => Self::Stdout(parts.next().unwrap().into()),
                "STDERR" => Self::Stderr(parts.next().unwrap().into()),
                "READY" => Self::Ready,
                "FATAL" => Self::Fatal(
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                ),
                "SHUTDOWN" => Self::Shutdown,
                _ => return Err(DaemonError::UnknownResponse(line.to_string())),
            };
            Ok(response)
        } else {
            Err(DaemonError::EmptyResponse)
        }
    }
}

/// Possible non-fatal error types from the
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum HookErrorKind {
    MissingBackendModule,
    MissingBackendAttribute,
    MalformedBackendName,
    BackendImportError,
    InvalidHookName,
    InvalidAction,
    UnsupportedHook,
    MalformedHookArgument,
    HookRuntimeError,
}

impl HookErrorKind {
    fn try_from_str(name: &str) -> Result<Self, DaemonError> {
        match name {
            "MissingBackendModule" => Ok(Self::MissingBackendModule),
            "MissingBackendAttribute" => Ok(Self::MissingBackendAttribute),
            "MalformedBackendName" => Ok(Self::MalformedBackendName),
            "BackendImportError" => Ok(Self::BackendImportError),
            "InvalidHookName" => Ok(Self::InvalidHookName),
            "InvalidAction" => Ok(Self::InvalidAction),
            "UnsupportedHook" => Ok(Self::UnsupportedHook),
            "MalformedHookArgument" => Ok(Self::MalformedHookArgument),
            "HookRuntimeError" => Ok(Self::HookRuntimeError),
            _ => Err(DaemonError::UnknownErrorKind(name.to_string())),
        }
    }
}

async fn read_stdout_into_channel(
    mut lines: Lines<BufReader<ChildStdout>>,
    sender: Sender<String>,
) -> () {
    // We do not handle any errors here. If there's an IO error or the receiver is dropped,
    // we will just exit. The receiver will report the daemon as closed if it attempts to
    // read after this channel exits.
    while let Ok(Some(line)) = lines.next_line().await {
        if let Err(_) = sender.send(line).await {
            return;
        }
    }
}
#[derive(Debug)]
struct DaemonIO {
    stdout: Receiver<String>,
    stdin: ChildStdin,
    handle: Child,
}

impl DaemonIO {
    async fn new(mut handle: Child) -> Self {
        // Create a buffered line reader for stdout
        let stdout = tokio::io::AsyncBufReadExt::lines(BufReader::new(
            handle.stdout.take().expect("stdout is available"),
        ));

        // Take standard input
        let stdin = handle.stdin.take().expect("stdin is available");

        // Create a channel to read standard output continuously
        let (sender, receiver) = tokio::sync::mpsc::channel(20);

        // We let this handle drop, as we don't care about its result
        tokio::spawn(read_stdout_into_channel(stdout, sender));

        Self {
            stdout: receiver,
            stdin,
            handle,
        }
    }

    fn exited(&mut self) -> Result<bool, io::Error> {
        Ok(self.handle.try_wait()?.is_some())
    }

    async fn send(&mut self, commands: Vec<&str>) -> Result<(), io::Error> {
        self.stdin.write_all(commands.join("\n").as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Option<String> {
        self.stdout.recv().await
    }

    async fn close(mut self) -> Result<Option<Output>, DaemonError> {
        if !self.exited()? {
            // Send a shutdown command if it's not closed yet
            self.stdin.write_all("shutdown\n".as_bytes()).await?;
        }
        Ok(Some(self.handle.wait_with_output().await?))
    }
}

#[derive(Debug)]
pub(crate) struct Pep517Daemon {
    script_path: PathBuf,
    venv: Virtualenv,
    source_tree: PathBuf,
    io: Option<DaemonIO>,
    closed: bool,
}

impl Pep517Daemon {
    pub(crate) async fn new(venv: &Virtualenv, source_tree: &Path) -> Result<Self, DaemonError> {
        // Write `hookd` to the virtual environment
        let script_path = venv.bin_dir().join("hookd");
        let mut file = File::create(&script_path).await?;
        file.write_all(HOOKD_SOURCE.as_bytes()).await?;

        Ok(Self {
            script_path,
            venv: venv.clone(),
            source_tree: source_tree.to_path_buf(),
            io: None,
            closed: false,
        })
    }

    /// Ensure the daemon is started and ready.
    /// If the daemon is not started, [`Self::start`] will be called.
    async fn ensure_started(&mut self) -> Result<&mut DaemonIO, DaemonError> {
        let started = {
            if let Some(io) = self.io.as_mut() {
                // Check if the process has exited
                !io.exited()?
            } else {
                false
            }
        };
        if !started {
            self.io = Some(self.start().await?);
        }

        // Wait until ready
        if self.receive_until_actionable().await? == DaemonResponse::Ready {
            Ok(self
                .io
                .as_mut()
                .expect("The daemon must be started, we just checked."))
        } else {
            Err(DaemonError::NotReady)
        }
    }

    /// Starts the daemon.
    async fn start(&mut self) -> Result<DaemonIO, DaemonError> {
        let mut new_path = self.venv.bin_dir().into_os_string();
        if let Some(path) = env::var_os("PATH") {
            new_path.push(":");
            new_path.push(path);
        };

        let handle = Command::new(self.venv.python_executable())
            .args([self.script_path.clone()])
            .current_dir(self.source_tree.clone())
            // Activate the venv
            .env("VIRTUAL_ENV", self.venv.root())
            .env("PATH", new_path)
            // Create pipes for communication
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Stderr doesn't have anything we need unless debugging
            .stderr(Stdio::null())
            .spawn()?;

        debug!(
            "Starting new hook daemon in virtualenv {}",
            self.venv.root().to_string_lossy()
        );
        Ok(DaemonIO::new(handle).await)
    }

    /// Reads a single response from the daemon.
    async fn receive_one(&mut self) -> Result<DaemonResponse, DaemonError> {
        if let Some(io) = self.io.as_mut() {
            if let Some(line) = io.recv().await {
                Ok(DaemonResponse::try_from_str(line.as_str())?)
            } else {
                self.close().await?;
                Err(DaemonError::Closed)
            }
        } else {
            Err(DaemonError::Closed)
        }
    }

    /// Reads from the daemon until an actionable response is seen.
    async fn receive_until_actionable(&mut self) -> Result<DaemonResponse, DaemonError> {
        loop {
            let next = self.receive_one().await?;
            match next {
                DaemonResponse::Debug(message) => debug!("{message}"),
                DaemonResponse::Expect(_) => continue,
                DaemonResponse::Fatal(kind, message) => {
                    let traceback = {
                        if let DaemonResponse::Traceback(traceback) = self.receive_one().await? {
                            traceback
                        } else {
                            "".to_string()
                        }
                    };
                    return Err(DaemonError::Crashed {
                        kind,
                        message,
                        traceback,
                    });
                }
                _ => return Ok(next),
            }
        }
    }

    /// Runs a hook to completion on the daemon.
    async fn run_hook(
        &mut self,
        backend: &Pep517Backend,
        hook_name: &str,
        mut args: Vec<&str>,
    ) -> Result<String, DaemonError> {
        let io = self.ensure_started().await?;

        // Always send run and the backend name
        let mut commands = vec!["run", backend.backend.as_str()];

        // Send backend paths
        if let Some(backend_paths) = backend.backend_path.as_ref() {
            for backend_path in backend_paths.iter() {
                commands.push(backend_path)
            }
        }
        commands.push("");

        // Specify the hook
        commands.push(hook_name);

        // Consume the arguments
        commands.append(&mut args);

        // Send a trailing newline
        commands.push("");

        io.send(commands).await?;

        // Read the responses
        loop {
            let next = self.receive_until_actionable().await?;
            match next {
                DaemonResponse::Stderr(_) => continue,
                DaemonResponse::Stdout(_) => continue,
                DaemonResponse::Ok(result) => return Ok(result),
                DaemonResponse::Error(_kind, message) => {
                    let traceback = {
                        if let DaemonResponse::Traceback(traceback) = self.receive_one().await? {
                            traceback
                        } else {
                            "".to_string()
                        }
                    };
                    return Err(DaemonError::HookError(message, traceback));
                }
                unexpected @ _ => return Err(DaemonError::UnexpectedResponse(unexpected)),
            }
        }
    }

    /// Run the hook to create a .dist-info directory for a package
    ///
    /// <https://peps.python.org/pep-0517/#prepare-metadata-for-build-wheel>
    /// <https://peps.python.org/pep-0660/#prepare-metadata-for-build-editable>
    pub(crate) async fn prepare_metadata_for_build(
        &mut self,
        backend: &Pep517Backend,
        kind: BuildKind,
        metadata_directory: PathBuf,
    ) -> Result<Option<PathBuf>, DaemonError> {
        let result = self
            .run_hook(
                backend,
                format!("prepare_metadata_for_build_{}", kind).as_str(),
                vec![metadata_directory.to_str().unwrap(), ""],
            )
            .await?;
        Ok(Some(PathBuf::from_str(result.as_str()).unwrap()))
    }

    /// Get the requirements for an editable or or wheel build.
    ///
    /// <https://peps.python.org/pep-0517/#get-requires-for-build-wheel>
    pub(crate) async fn get_requires_for_build(
        &mut self,
        backend: &Pep517Backend,
        kind: BuildKind,
    ) -> Result<Vec<Requirement>, DaemonError> {
        let result = self
            .run_hook(
                backend,
                format!("get_requires_for_build_{}", kind).as_str(),
                vec![""],
            )
            .await?;

        // TODO(zanieb): Improve the parsing of requirements lists
        let requirements: Result<Vec<Requirement>, _> = result
            .strip_prefix("[")
            .unwrap()
            .strip_suffix("]")
            .unwrap()
            .split(", ")
            .map(|item| {
                item.strip_prefix('\'')
                    .and_then(|item| item.strip_suffix('\''))
            })
            .filter(|item| item.is_some())
            .map(|item| item.unwrap())
            .filter(|item| !item.is_empty())
            .map(|item| {
                Requirement::from_str(item)
                    .map_err(|err| DaemonError::InvalidResult(item.to_string(), err.to_string()))
            })
            .collect();

        requirements
    }

    /// Run a wheel or editable build hook.
    ///
    /// Note the daemon also support the `build_sdist` hook but it is not supported by [`BuildKind`].
    ///
    /// <https://peps.python.org/pep-0517/#build-wheel>
    /// <https://peps.python.org/pep-0660/#build-editable>
    pub(crate) async fn build(
        &mut self,
        backend: &Pep517Backend,
        kind: BuildKind,
        wheel_directory: &Path,
        metadata_directory: Option<&Path>,
    ) -> Result<String, DaemonError> {
        let result = self
            .run_hook(
                backend,
                format!("build_{}", kind).as_str(),
                vec![
                    wheel_directory.to_string_lossy().deref(),
                    "",
                    metadata_directory
                        .unwrap_or(Path::new(""))
                        .to_string_lossy()
                        .deref(),
                ],
            )
            .await?;
        Ok(result)
    }

    /// Close the daemon, waiting for it to exit.
    /// If the daemon has already been closed, `None` will be returned.
    pub(crate) async fn close(&mut self) -> Result<Option<Output>, DaemonError> {
        // Mark `closed` before attempting to close
        // If there's an error on close, we should raise that instead of complaining it was never called
        self.closed = true;

        if let Some(io) = self.io.take() {
            io.close().await
        } else {
            Ok(None)
        }
    }
}

impl Drop for Pep517Daemon {
    fn drop(&mut self) {
        // On drop, we ensure `close` was called. Otherwise, we can leave behind a zombie process.
        if !self.closed {
            panic!("`Pep517Daemon::close()` not called before drop.");
        }
    }
}
