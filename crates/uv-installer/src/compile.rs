use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use std::{env, io, panic};

use anyhow::anyhow;
use async_channel::{Receiver, SendError, Sender};
use tempfile::{TempDir, tempdir_in};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tracing::{debug, instrument};
use walkdir::WalkDir;

use uv_configuration::Concurrency;
use uv_distribution_types::CachedDist;
use uv_fs::{CWD, Simplified};
use uv_install_wheel::{Layout, installed_dist_info_path};
use uv_static::EnvVars;
use uv_warnings::warn_user;

const COMPILEALL_SCRIPT: &str = include_str!("pip_compileall.py");
/// This is longer than any compilation should ever take.
const DEFAULT_COMPILE_TIMEOUT: Duration = Duration::from_mins(1);

type WorkerOutcome = std::thread::Result<Result<(), CompileError>>;
type WorkerHandle = oneshot::Receiver<WorkerOutcome>;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Failed to list files in `site-packages`")]
    Walkdir(#[from] walkdir::Error),
    #[error("Failed to send task to worker")]
    WorkerDisappeared(SendError<PathBuf>),
    #[error("Failed to identify Python source files")]
    SourceFiles(#[source] anyhow::Error),
    #[error("The task executor is broken, did some other task panic?")]
    Join,
    #[error("Failed to start Python interpreter to run compile script")]
    PythonSubcommand(#[source] io::Error),
    #[error("Failed to create temporary script file")]
    TempFile(#[source] io::Error),
    #[error(r#"Bytecode compilation failed, expected "{0}", received: "{1}""#)]
    WrongPath(String, String),
    #[error("Failed to write to Python {device}")]
    ChildStdio {
        device: &'static str,
        #[source]
        err: io::Error,
    },
    #[error("Python process stderr:\n{stderr}")]
    ErrorWithStderr {
        stderr: String,
        #[source]
        err: Box<Self>,
    },
    #[error("Bytecode timed out ({}s) compiling file: `{}`", elapsed.as_secs_f32(), source_file)]
    CompileTimeout {
        elapsed: Duration,
        source_file: String,
    },
    #[error("Python startup timed out ({}s)", _0.as_secs_f32())]
    StartupTimeout(Duration),
    #[error("Got invalid value from environment for {var}: {message}.")]
    EnvironmentError { var: &'static str, message: String },
}

fn compile_timeout() -> Result<Option<Duration>, CompileError> {
    let timeout = match env::var(EnvVars::UV_COMPILE_BYTECODE_TIMEOUT) {
        Ok(value) => match value.as_str() {
            "0" => None,
            _ => match value.parse::<u64>().map(Duration::from_secs) {
                Ok(duration) => Some(duration),
                Err(_) => {
                    return Err(CompileError::EnvironmentError {
                        var: EnvVars::UV_COMPILE_BYTECODE_TIMEOUT,
                        message: format!("Expected an integer number of seconds, got \"{value}\""),
                    });
                }
            },
        },
        Err(_) => Some(DEFAULT_COMPILE_TIMEOUT),
    };
    if let Some(duration) = timeout {
        debug!(
            "Using bytecode compilation timeout of {}s",
            duration.as_secs()
        );
    } else {
        debug!("Disabling bytecode compilation timeout");
    }
    Ok(timeout)
}

fn spawn_workers(
    dir: &Path,
    python_executable: &Path,
    pip_compileall_py: &Path,
    receiver: &Arc<Receiver<PathBuf>>,
    worker_count: usize,
    timeout: Option<Duration>,
) -> Vec<WorkerHandle> {
    debug!("Starting {} bytecode compilation workers", worker_count);
    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let (tx, rx) = oneshot::channel();
        let receiver = Arc::clone(receiver);

        let worker = worker(
            dir.to_path_buf(),
            python_executable.to_path_buf(),
            pip_compileall_py.to_path_buf(),
            receiver.as_ref().clone(),
            timeout,
        );

        // Spawn each worker on a dedicated thread.
        std::thread::Builder::new()
            .name("uv-compile".to_owned())
            .spawn(move || {
                // Keep the shared receiver alive only while a worker is running. This lets the
                // wheel compiler grow its pool without hiding the disappearance of every worker.
                let _receiver = receiver;
                // Report panics back to the main thread.
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to build runtime")
                        .block_on(worker)
                }));

                // This may fail if the main thread returned early due to an error.
                let _ = tx.send(result);
            })
            .expect("Failed to start compilation worker");

        worker_handles.push(rx);
    }
    worker_handles
}

/// Wait for all workers to exit so worker failures are not hidden by channel send errors.
async fn wait_for_workers(
    worker_handles: Vec<WorkerHandle>,
    send_error: Option<SendError<PathBuf>>,
) -> Result<(), CompileError> {
    for result in futures::future::join_all(worker_handles).await {
        match result {
            // A worker thread panicked or exited without reporting its result.
            Err(_) | Ok(Err(_)) => return Err(CompileError::Join),
            Ok(Ok(Err(compile_error))) => return Err(compile_error),
            Ok(Ok(Ok(()))) => {}
        }
    }

    if let Some(send_error) = send_error {
        // This is suspicious: Why did the channel stop working, but all workers exited
        // successfully?
        return Err(CompileError::WorkerDisappeared(send_error));
    }

    Ok(())
}

fn python_source_files(
    dir: &Path,
    skip_wheel_data: bool,
) -> impl Iterator<Item = Result<PathBuf, walkdir::Error>> + '_ {
    // https://github.com/pypa/pip/blob/3820b0e52c7fed2b2c43ba731b718f316e6816d1/src/pip/_internal/operations/install/wheel.py#L593-L604
    WalkDir::new(dir)
        .into_iter()
        // Otherwise we stumble over temporary files from `compileall`.
        .filter_entry(move |entry| {
            entry.file_name() != "__pycache__"
                && !(skip_wheel_data
                    && entry.depth() == 1
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("data")))
        })
        .filter_map(|entry| {
            let (entry, metadata) =
                match entry.and_then(|entry| entry.metadata().map(|metadata| (entry, metadata))) {
                    Ok((entry, metadata)) => (entry, metadata),
                    Err(err)
                        if err
                            .io_error()
                            .is_some_and(|err| err.kind() == io::ErrorKind::NotFound) =>
                    {
                        return None;
                    }
                    Err(err) => return Some(Err(err)),
                };
            (metadata.is_file() && entry.path().extension().is_some_and(|ext| ext == "py"))
                .then(|| Ok(entry.into_path()))
        })
}

struct WheelCompilerWorkers {
    sender: Sender<PathBuf>,
    receiver: Weak<Receiver<PathBuf>>,
    worker_handles: Vec<WorkerHandle>,
    tempdir: TempDir,
    timeout: Option<Duration>,
}

/// A shared pool of Python interpreters for compiling installed wheels.
pub struct WheelCompiler {
    python_executable: PathBuf,
    cache: PathBuf,
    worker_count: usize,
    workers: Mutex<Option<WheelCompilerWorkers>>,
    send_error: Mutex<Option<SendError<PathBuf>>>,
    queued: Mutex<HashSet<PathBuf>>,
    source_files: AtomicUsize,
}

impl WheelCompiler {
    /// Create a shared pool that starts Python workers when the first source is queued.
    pub fn new(
        python_executable: &Path,
        concurrency: &Concurrency,
        cache: &Path,
    ) -> Result<Self, CompileError> {
        Ok(Self {
            python_executable: python_executable.to_path_buf(),
            cache: cache.to_path_buf(),
            worker_count: concurrency.installs.max(1),
            workers: Mutex::new(None),
            send_error: Mutex::new(None),
            queued: Mutex::new(HashSet::new()),
            source_files: AtomicUsize::new(0),
        })
    }

    fn sender(&self, worker_count: usize) -> Result<Sender<PathBuf>, CompileError> {
        let mut workers = self.workers.lock().map_err(|_| CompileError::Join)?;
        if let Some(workers) = workers.as_mut() {
            let additional = worker_count.saturating_sub(workers.worker_handles.len());
            if additional > 0
                && let Some(receiver) = workers.receiver.upgrade()
            {
                workers.worker_handles.extend(spawn_workers(
                    &self.cache,
                    &self.python_executable,
                    &workers.tempdir.path().join("pip_compileall.py"),
                    &receiver,
                    additional,
                    workers.timeout,
                ));
            }
            return Ok(workers.sender.clone());
        }

        // A larger buffer is significantly faster than just 1 or the worker count.
        let (sender, receiver) =
            async_channel::bounded::<PathBuf>(self.worker_count.saturating_mul(10));
        let receiver = Arc::new(receiver);
        // Running Python with an actual file will produce better error messages.
        let tempdir = tempdir_in(&self.cache).map_err(CompileError::TempFile)?;
        let pip_compileall_py = tempdir.path().join("pip_compileall.py");
        let timeout = compile_timeout()?;
        let worker_handles = spawn_workers(
            &self.cache,
            &self.python_executable,
            &pip_compileall_py,
            &receiver,
            worker_count,
            timeout,
        );
        let shared_receiver = Arc::downgrade(&receiver);
        *workers = Some(WheelCompilerWorkers {
            sender: sender.clone(),
            receiver: shared_receiver,
            worker_handles,
            tempdir,
            timeout,
        });
        Ok(sender)
    }

    /// Queue an installed Python source file for bytecode compilation.
    pub fn queue_file(&self, source_file: PathBuf) -> Result<(), CompileError> {
        debug_assert!(
            source_file.is_absolute(),
            "compileall doesn't work with relative paths: `{}`",
            source_file.display()
        );

        let mut queued = self.queued.lock().map_err(|_| CompileError::Join)?;
        if !queued.insert(source_file.clone()) {
            return Ok(());
        }
        let worker_count = queued.len().min(self.worker_count);
        drop(queued);

        if let Err(err) = self.sender(worker_count)?.send_blocking(source_file) {
            let mut send_error = self.send_error.lock().map_err(|_| CompileError::Join)?;
            if send_error.is_none() {
                *send_error = Some(err);
            }
        } else {
            self.source_files.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Queue the root Python source files belonging to an installed wheel.
    pub(crate) fn queue_wheel(
        &self,
        layout: &Layout,
        wheel: &CachedDist,
    ) -> Result<(), CompileError> {
        let dist_info = installed_dist_info_path(layout, wheel.path())
            .map_err(|err| CompileError::SourceFiles(err.into()))?;
        let site_packages = dist_info.parent().ok_or_else(|| {
            CompileError::SourceFiles(anyhow!(
                "Installed distribution has no site-packages parent: `{wheel}`"
            ))
        })?;

        // Files in `.data` are relocated during installation. They are queued from the installed
        // RECORD after all wheels have been installed.
        for source_file in python_source_files(wheel.path(), true) {
            let source_file = source_file?;
            let relative = source_file.strip_prefix(wheel.path()).map_err(|_| {
                CompileError::SourceFiles(anyhow!(
                    "Python source is outside its cached wheel: `{}`",
                    source_file.display()
                ))
            })?;
            self.queue_file(CWD.join(site_packages.join(relative)))?;
        }
        Ok(())
    }

    /// Queue the Python source files in an installed directory.
    pub fn queue_tree(&self, dir: &Path) -> Result<(), CompileError> {
        for source_file in python_source_files(dir, false) {
            self.queue_file(source_file?)?;
        }
        Ok(())
    }

    /// Finish all queued compilation tasks and shut down the worker pool.
    pub async fn finish(self: Arc<Self>) -> Result<usize, CompileError> {
        let compiler = Arc::try_unwrap(self).map_err(|_| CompileError::Join)?;
        let Self {
            python_executable: _,
            cache: _,
            worker_count: _,
            workers,
            send_error,
            queued: _,
            source_files,
        } = compiler;
        if let Some(WheelCompilerWorkers {
            sender,
            receiver: _,
            worker_handles,
            tempdir,
            timeout: _,
        }) = workers.into_inner().map_err(|_| CompileError::Join)?
        {
            drop(sender);
            let send_error = send_error.into_inner().map_err(|_| CompileError::Join)?;
            let result = wait_for_workers(worker_handles, send_error).await;
            // Keep the compile script alive until all workers have exited.
            drop(tempdir);
            result?;
        }
        Ok(source_files.into_inner())
    }
}

/// Bytecode compile all file in `dir` using a pool of Python interpreters running a Python script
/// that calls `compileall.compile_file`.
///
/// All compilation errors are muted (like pip). There is a 60s timeout for each file to handle
/// a broken `python`. The timeout can be configured with `UV_COMPILE_BYTECODE_TIMEOUT`; a value of
/// `0` disables the timeout.
///
/// We only compile all files, but we don't update the RECORD, relying on PEP 491:
/// > Uninstallers should be smart enough to remove .pyc even if it is not mentioned in RECORD.
///
/// We've confirmed that both uv and pip (as of 24.0.0) remove the `__pycache__` directory.
#[instrument(skip(python_executable))]
pub async fn compile_tree(
    dir: &Path,
    python_executable: &Path,
    concurrency: &Concurrency,
    cache: &Path,
) -> Result<usize, CompileError> {
    compile_tree_inner(dir, python_executable, concurrency, cache).await
}

async fn compile_tree_inner(
    dir: &Path,
    python_executable: &Path,
    concurrency: &Concurrency,
    cache: &Path,
) -> Result<usize, CompileError> {
    debug_assert!(
        dir.is_absolute(),
        "compileall doesn't work with relative paths: `{}`",
        dir.display()
    );
    let worker_count = concurrency.installs;

    // A larger buffer is significantly faster than just 1 or the worker count.
    let (sender, receiver) = async_channel::bounded::<PathBuf>(worker_count * 10);
    let receiver = Arc::new(receiver);

    // Running Python with an actual file will produce better error messages.
    let tempdir = tempdir_in(cache).map_err(CompileError::TempFile)?;
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");
    let timeout = compile_timeout()?;
    let worker_handles = spawn_workers(
        dir,
        python_executable,
        &pip_compileall_py,
        &receiver,
        worker_count,
        timeout,
    );
    // Make sure the channel gets closed when all workers exit.
    drop(receiver);

    // Start the producer, sending all `.py` files to workers.
    let mut source_files = 0;
    let mut send_error = None;
    for source_file in python_source_files(dir, false) {
        source_files += 1;
        if let Err(err) = sender.send(source_file?).await {
            // The workers exited.
            // If e.g. something with the Python interpreter is wrong, the workers have exited
            // with an error. We try to report this informative error and only if that fails,
            // report the send error.
            send_error = Some(err);
            break;
        }
    }

    // All workers will receive an error after the last item. Note that there are still
    // up to worker_count * 10 items in the queue.
    drop(sender);

    wait_for_workers(worker_handles, send_error).await?;

    Ok(source_files)
}

/// Bytecode compile the given Python source files using a pool of Python interpreters.
///
/// All paths must be absolute. Compilation errors are muted (like pip), while failures to launch
/// or communicate with the Python workers are returned.
#[instrument(skip(files, python_executable))]
pub async fn compile_files(
    files: impl IntoIterator<Item = anyhow::Result<PathBuf>>,
    python_executable: &Path,
    concurrency: &Concurrency,
    cache: &Path,
) -> Result<usize, CompileError> {
    let mut files = files.into_iter();
    let mut initial_files = Vec::with_capacity(concurrency.installs);
    for file in files.by_ref().take(concurrency.installs) {
        initial_files.push(file.map_err(CompileError::SourceFiles)?);
    }
    if initial_files.is_empty() {
        return Ok(0);
    }

    let worker_count = initial_files.len();
    let (sender, receiver) = async_channel::bounded::<PathBuf>(worker_count * 10);
    let receiver = Arc::new(receiver);

    // Running Python with an actual file will produce better error messages.
    let tempdir = tempdir_in(cache).map_err(CompileError::TempFile)?;
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");
    let timeout = compile_timeout()?;
    let worker_handles = spawn_workers(
        cache,
        python_executable,
        &pip_compileall_py,
        &receiver,
        worker_count,
        timeout,
    );
    drop(receiver);

    let mut send_error = None;
    let mut source_error = None;
    let mut source_files = 0;
    for file in initial_files.into_iter().map(Ok).chain(files) {
        let file = match file {
            Ok(file) => file,
            Err(err) => {
                source_error = Some(err);
                break;
            }
        };
        debug_assert!(
            file.is_absolute(),
            "compileall doesn't work with relative paths: `{}`",
            file.display()
        );
        source_files += 1;
        if let Err(err) = sender.send(file).await {
            send_error = Some(err);
            break;
        }
    }
    drop(sender);

    wait_for_workers(worker_handles, send_error).await?;
    if let Some(source_error) = source_error {
        return Err(CompileError::SourceFiles(source_error));
    }

    Ok(source_files)
}

async fn worker(
    dir: PathBuf,
    interpreter: PathBuf,
    pip_compileall_py: PathBuf,
    receiver: Receiver<PathBuf>,
    timeout: Option<Duration>,
) -> Result<(), CompileError> {
    fs_err::tokio::write(&pip_compileall_py, COMPILEALL_SCRIPT)
        .await
        .map_err(CompileError::TempFile)?;

    // Sometimes, the first time we read from stdout, we get an empty string back (no newline). If
    // we try to write to stdin, it will often be a broken pipe. In this case, we have to restart
    // the child process
    // https://github.com/astral-sh/uv/issues/2245
    let wait_until_ready = async {
        loop {
            // If the interpreter started successful, return it, else retry.
            if let Some(child) =
                launch_bytecode_compiler(&dir, &interpreter, &pip_compileall_py).await?
            {
                break Ok::<_, CompileError>(child);
            }
        }
    };

    // Handle a broken `python` by using a timeout, one that's higher than any compilation
    // should ever take.
    let (mut bytecode_compiler, child_stdin, mut child_stdout, mut child_stderr) =
        if let Some(duration) = timeout {
            tokio::time::timeout(duration, wait_until_ready)
                .await
                .map_err(|_| CompileError::StartupTimeout(timeout.unwrap()))??
        } else {
            wait_until_ready.await?
        };

    let stderr_reader = tokio::task::spawn(async move {
        let mut child_stderr_collected: Vec<u8> = Vec::new();
        child_stderr
            .read_to_end(&mut child_stderr_collected)
            .await?;
        Ok(child_stderr_collected)
    });

    let result = worker_main_loop(receiver, child_stdin, &mut child_stdout, timeout).await;
    // Reap the process to avoid zombies.
    let _ = bytecode_compiler.kill().await;

    // If there was something printed to stderr (which shouldn't happen, we muted all errors), tell
    // the user, otherwise only forward the result.
    let child_stderr_collected = stderr_reader
        .await
        .map_err(|_| CompileError::Join)?
        .map_err(|err| CompileError::ChildStdio {
            device: "stderr",
            err,
        })?;
    let result = if child_stderr_collected.is_empty() {
        result
    } else {
        let stderr = String::from_utf8_lossy(&child_stderr_collected);
        match result {
            Ok(()) => {
                debug!(
                    "Bytecode compilation `python` at {} stderr:\n{}\n---",
                    interpreter.user_display(),
                    stderr
                );
                Ok(())
            }
            Err(err) => Err(CompileError::ErrorWithStderr {
                stderr: stderr.trim().to_string(),
                err: Box::new(err),
            }),
        }
    };

    debug!("Bytecode compilation worker exiting: {:?}", result);

    result
}

/// Returns the child and stdin/stdout/stderr on a successful launch or `None` for a broken interpreter state.
async fn launch_bytecode_compiler(
    dir: &Path,
    interpreter: &Path,
    pip_compileall_py: &Path,
) -> Result<
    Option<(
        Child,
        ChildStdin,
        BufReader<ChildStdout>,
        BufReader<ChildStderr>,
    )>,
    CompileError,
> {
    // We input the paths through stdin and get the successful paths returned through stdout.
    let mut bytecode_compiler = Command::new(interpreter)
        .arg(pip_compileall_py)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(dir)
        // Otherwise stdout is buffered and we'll wait forever for a response
        .env(EnvVars::PYTHONUNBUFFERED, "1")
        .spawn()
        .map_err(CompileError::PythonSubcommand)?;

    // https://stackoverflow.com/questions/49218599/write-to-child-process-stdin-in-rust/49597789#comment120223107_49597789
    // Unbuffered, we need to write immediately or the python process will get stuck waiting
    let child_stdin = bytecode_compiler
        .stdin
        .take()
        .expect("Child must have stdin");
    let mut child_stdout = BufReader::new(
        bytecode_compiler
            .stdout
            .take()
            .expect("Child must have stdout"),
    );
    let child_stderr = BufReader::new(
        bytecode_compiler
            .stderr
            .take()
            .expect("Child must have stderr"),
    );

    // Check if the launch was successful.
    let mut out_line = String::new();
    child_stdout
        .read_line(&mut out_line)
        .await
        .map_err(|err| CompileError::ChildStdio {
            device: "stdout",
            err,
        })?;

    if out_line.trim_end() == "Ready" {
        // Success
        Ok(Some((
            bytecode_compiler,
            child_stdin,
            child_stdout,
            child_stderr,
        )))
    } else if out_line.is_empty() {
        // Failed to launch, try again
        Ok(None)
    } else {
        // Not observed yet
        Err(CompileError::WrongPath("Ready".to_string(), out_line))
    }
}

/// We use stdin/stdout as a sort of bounded channel. We write one path to stdin, then wait until
/// we get the same path back from stdout. This way we ensure one worker is only working on one
/// piece of work at the same time.
async fn worker_main_loop(
    receiver: Receiver<PathBuf>,
    mut child_stdin: ChildStdin,
    child_stdout: &mut BufReader<ChildStdout>,
    timeout: Option<Duration>,
) -> Result<(), CompileError> {
    let mut out_line = String::new();
    while let Ok(source_file) = receiver.recv().await {
        let source_file = source_file.display().to_string();
        if source_file.contains(['\r', '\n']) {
            warn_user!("Path contains newline, skipping: {source_file:?}");
            continue;
        }
        // Luckily, LF alone works on windows too
        let bytes = format!("{source_file}\n").into_bytes();

        let python_handle = async {
            child_stdin
                .write_all(&bytes)
                .await
                .map_err(|err| CompileError::ChildStdio {
                    device: "stdin",
                    err,
                })?;

            out_line.clear();
            child_stdout.read_line(&mut out_line).await.map_err(|err| {
                CompileError::ChildStdio {
                    device: "stdout",
                    err,
                }
            })?;
            Ok::<(), CompileError>(())
        };

        // Handle a broken `python` by using a timeout, one that's higher than any compilation
        // should ever take.
        if let Some(duration) = timeout {
            tokio::time::timeout(duration, python_handle)
                .await
                .map_err(|_| CompileError::CompileTimeout {
                    elapsed: duration,
                    source_file: source_file.clone(),
                })??;
        } else {
            python_handle.await?;
        }

        // This is a sanity check, if we don't get the path back something has gone wrong, e.g.
        // we're not actually running a python interpreter.
        let actual = out_line.trim_end_matches(['\n', '\r']);
        if actual != source_file {
            return Err(CompileError::WrongPath(source_file, actual.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uv_cache::Cache;
    use uv_configuration::Concurrency;
    use uv_python::{EnvironmentPreference, PythonEnvironment, PythonPreference, PythonRequest};

    use super::WheelCompiler;

    #[tokio::test]
    async fn wheel_compiler_reuses_pool_and_deduplicates_files() {
        let cache = Cache::temp().expect("cache should be available");
        let wheel = tempfile::tempdir_in(cache.root()).expect("wheel directory should exist");
        let package = wheel.path().join("package");
        fs_err::create_dir_all(&package).expect("package directory should exist");
        fs_err::write(package.join("__init__.py"), "VALUE = 1\n")
            .expect("package source should be written");

        let environment = PythonEnvironment::find(
            &PythonRequest::Any,
            EnvironmentPreference::Any,
            PythonPreference::System,
            &cache,
        )
        .expect("Python environment should be available");
        let concurrency = Concurrency::new(1, 1, 4, 1);

        let other_wheel = tempfile::tempdir_in(cache.root()).expect("wheel directory should exist");
        let other_package = other_wheel.path().join("other_package");
        fs_err::create_dir_all(&other_package).expect("package directory should exist");
        fs_err::write(other_package.join("__init__.py"), "VALUE = 2\n")
            .expect("package source should be written");

        let compiler = Arc::new(
            WheelCompiler::new(environment.python_executable(), &concurrency, cache.root())
                .expect("compiler should start"),
        );
        assert!(
            compiler
                .workers
                .lock()
                .expect("worker state should be available")
                .is_none()
        );
        compiler
            .queue_file(package.join("__init__.py"))
            .expect("source should be queued");
        assert_eq!(
            compiler
                .workers
                .lock()
                .expect("worker state should be available")
                .as_ref()
                .expect("worker should start for the first source")
                .worker_handles
                .len(),
            1
        );
        compiler
            .queue_file(package.join("__init__.py"))
            .expect("duplicate source should be ignored");
        compiler
            .queue_file(other_package.join("__init__.py"))
            .expect("source should be queued");
        assert_eq!(
            compiler
                .workers
                .lock()
                .expect("worker state should be available")
                .as_ref()
                .expect("worker should remain available")
                .worker_handles
                .len(),
            2
        );
        let compiled = compiler.finish().await.expect("sources should compile");

        assert_eq!(compiled, 2);
        assert!(package.join("__pycache__").exists());
        assert!(other_package.join("__pycache__").exists());
    }
}
