use std::env;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::BufReader;
use tokio::io::{AsyncWriteExt, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, error};

use crate::{BuildKind, Error, Pep517Backend};
use pep508_rs::Requirement;
use puffin_interpreter::Virtualenv;
use puffin_traits::SourceBuildTrait;

static HOOKD_SOURCE: &'static str = include_str!("hookd.py");

#[derive(Debug, Eq, PartialEq, Clone)]
enum Pep517DaemonResponse {
    Debug(String),
    Error(Pep517DaemonErrorKind, String),
    Traceback(String),
    Ok(String),
    Stderr(PathBuf),
    Stdout(PathBuf),
    Expect(String),
    Ready,
    Fatal(String, String),
    Shutdown,
}

impl Pep517DaemonResponse {
    fn from_line(line: &str) -> Result<Self, Error> {
        // Split on the first two spaces
        let mut parts = line.splitn(3, ' ');
        if let Some(kind) = parts.next() {
            let response = match kind {
                "DEBUG" => Self::Debug(parts.collect::<Vec<&str>>().join(" ")),
                "EXPECT" => Self::Expect(parts.collect::<Vec<&str>>().join(" ")),
                "OK" => Self::Ok(parts.collect::<Vec<&str>>().join(" ")),
                "TRACEBACK" => Self::Traceback(parts.collect::<Vec<&str>>().join(" ")),
                "ERROR" => Self::Error(
                    Pep517DaemonErrorKind::from_str(parts.next().unwrap())?,
                    parts.next().unwrap().to_string(),
                ),
                "STDOUT" => Self::Stdout(parts.next().unwrap().into()),
                "STDERR" => Self::Stderr(parts.next().unwrap().into()),
                "READY" => Self::Ready,
                "FATAL" => Self::Fatal(
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                ),
                "SHUTDOWN" => Self::Shutdown,
                _ => {
                    return Err(Error::DaemonError {
                        message: format!("Unknown response: {}", line),
                    })
                }
            };
            Ok(response)
        } else {
            Err(Error::DaemonError {
                message: "No kind in response.".into(),
            })
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
enum Pep517DaemonErrorKind {
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

impl Pep517DaemonErrorKind {
    fn from_str(name: &str) -> Result<Self, Error> {
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
            _ => Err(Error::DaemonError {
                message: "Unknown error kind".into(),
            }),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Pep517Daemon {
    script_path: PathBuf,
    venv: Virtualenv,
    source_tree: PathBuf,
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    stdin: Option<ChildStdin>,
    handle: Option<Child>,
    last_response: Option<Pep517DaemonResponse>,
    closed: bool,
}

impl Pep517Daemon {
    pub(crate) async fn new(venv: &Virtualenv, source_tree: &Path) -> Result<Self, Error> {
        // Write `hookd` to the virtual environment
        let script_path = venv.bin_dir().join("hookd");
        let mut file = File::create(&script_path).await?;
        file.write_all(HOOKD_SOURCE.as_bytes()).await?;

        Ok(Self {
            script_path,
            venv: venv.clone(),
            source_tree: source_tree.to_path_buf(),
            stdout: None,
            stdin: None,
            handle: None,
            last_response: None,
            closed: false,
        })
    }

    async fn ensure_started(&mut self) -> Result<(), Error> {
        let started = {
            if let Some(handle) = self.handle.as_mut() {
                // Check if the process has exited
                handle.try_wait()?.is_none()
            } else {
                false
            }
        };
        if !started {
            let handle = self.start().await?;
            self.handle = Some(handle);
            self.closed = false;

            let stdout = self
                .handle
                .as_mut()
                .unwrap()
                .stdout
                .take()
                .expect("stdout is available");

            self.stdout = Some(tokio::io::AsyncBufReadExt::lines(BufReader::new(stdout)));

            self.stdin = Some(
                self.handle
                    .as_mut()
                    .unwrap()
                    .stdin
                    .take()
                    .expect("stdin is available"),
            );
        }

        // Wait until ready
        if self.receive_until_actionable().await? == Pep517DaemonResponse::Ready {
            Ok(())
        } else {
            Err(Error::DaemonError {
                message: "did not recieve ready".into(),
            })
        }
    }

    async fn start(&mut self) -> Result<Child, Error> {
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
            "Started new hook daemon in virtualenv {}",
            self.venv.root().to_string_lossy()
        );

        Ok(handle)
    }

    async fn receive_one(&mut self) -> Result<Pep517DaemonResponse, Error> {
        let stdout = self.stdout.as_mut().unwrap();
        if let Some(line) = stdout.next_line().await? {
            let response = Pep517DaemonResponse::from_line(line.as_str())?;
            self.last_response = Some(response.clone());
            Ok(response)
        } else {
            if let Some(output) = self.close().await? {
                Err(Error::DaemonError {
                    message: format!(
                        "Daemon closed unexpectedly with code {:?}",
                        // TODO: debug output
                        output
                    ),
                })
            } else {
                Err(Error::DaemonError {
                    message: format!("Daemon closed unexpectedly"),
                })
            }
        }
    }

    async fn receive_until_actionable(&mut self) -> Result<Pep517DaemonResponse, Error> {
        loop {
            let next = self.receive_one().await?;
            match next {
                Pep517DaemonResponse::Debug(message) => debug!("{message}"),
                Pep517DaemonResponse::Expect(_) => continue,
                Pep517DaemonResponse::Fatal(kind, message) => {
                    if let Pep517DaemonResponse::Traceback(traceback) = self.receive_one().await? {
                        error!("{}", traceback.replace("\\n", "\n").replace("\n\n", "\n"))
                    }
                    return Err(Error::DaemonError {
                        message: format!("Fatal error {kind}: {message}"),
                    });
                }
                _ => return Ok(next),
            }
        }
    }

    async fn run_hook(
        &mut self,
        backend: &Pep517Backend,
        hook_name: &str,
        mut args: Vec<&str>,
    ) -> Result<String, Error> {
        self.ensure_started().await?;

        let stdin = self.stdin.as_mut().unwrap();

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

        stdin.write_all(commands.join("\n").as_bytes()).await?;
        stdin.flush().await?;

        // Read the responses
        loop {
            let next = self.receive_until_actionable().await?;
            match next {
                Pep517DaemonResponse::Stderr(_) => continue,
                Pep517DaemonResponse::Stdout(_) => continue,
                Pep517DaemonResponse::Ok(result) => return Ok(result),
                Pep517DaemonResponse::Error(_kind, message) => {
                    if let Pep517DaemonResponse::Traceback(message) =
                        self.receive_until_actionable().await?
                    {
                        error!("{}", message.replace("\\n", "\n").replace("\n\n", "\n"))
                    }
                    return Err(Error::DaemonError { message });
                }
                _ => break,
            }
        }

        Err(Error::DaemonError {
            message: format!("unexpected response {:?}", self.last_response).to_string(),
        })
    }

    pub(crate) async fn prepare_metadata_for_build_wheel(
        &mut self,
        backend: &Pep517Backend,
        metadata_directory: PathBuf,
    ) -> Result<Option<PathBuf>, Error> {
        let result = self
            .run_hook(
                backend,
                "prepare_metadata_for_build_wheel",
                vec![metadata_directory.to_str().unwrap(), ""],
            )
            .await?;
        Ok(Some(PathBuf::from_str(result.as_str()).unwrap()))
    }

    pub(crate) async fn get_requires_for_build(
        &mut self,
        backend: &Pep517Backend,
        kind: BuildKind,
    ) -> Result<Vec<Requirement>, Error> {
        let result = self
            .run_hook(
                backend,
                format!("get_requires_for_build_{}", kind).as_str(),
                vec![""],
            )
            .await?;

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
                Requirement::from_str(item).map_err(|err| Error::DaemonError {
                    message: format!("Failed to parse {}: {}", item, err.to_string()),
                })
            })
            .collect();

        requirements
    }
    pub(crate) async fn build(
        &mut self,
        backend: &Pep517Backend,
        kind: BuildKind,
        wheel_directory: &Path,
        metadata_directory: Option<&Path>,
    ) -> Result<String, Error> {
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

    pub(crate) async fn close(&mut self) -> Result<Option<Output>, Error> {
        // Mark `closed` before attempting to close
        // If there's an error on close, we should raise that instead of complaining it was never called
        self.closed = true;
        if let Some(mut handle) = self.handle.take() {
            if handle.try_wait()?.is_none() {
                // Send a shutdown command if it's not closed yet
                let stdin = self.stdin.as_mut().unwrap();
                stdin.write_all("shutdown\n".as_bytes()).await?;
            }
            Ok(Some(handle.wait_with_output().await?))
        } else {
            Ok(None)
        }
    }
}

impl Drop for Pep517Daemon {
    fn drop(&mut self) {
        if !self.closed {
            panic!("`Pep517Daemon::close()` not called before drop.");
        }
    }
}
