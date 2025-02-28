use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use std::{io, panic};

use async_channel::{Receiver, SendError};
use tempfile::tempdir_in;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tracing::{debug, instrument};
use walkdir::WalkDir;

use uv_configuration::Concurrency;
use uv_fs::Simplified;
use uv_static::EnvVars;
use uv_warnings::warn_user;

const COMPILEALL_SCRIPT: &str = include_str!("pip_compileall.py");
/// This is longer than any compilation should ever take.
const COMPILE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Failed to list files in `site-packages`")]
    Walkdir(#[from] walkdir::Error),
    #[error("Failed to send task to worker")]
    WorkerDisappeared(SendError<PathBuf>),
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
}

/// Bytecode compile all file in `dir` using a pool of Python interpreters running a Python script
/// that calls `compileall.compile_file`.
///
/// All compilation errors are muted (like pip). There is a 60s timeout for each file to handle
/// a broken `python`.
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
    debug_assert!(
        dir.is_absolute(),
        "compileall doesn't work with relative paths: `{}`",
        dir.display()
    );
    let worker_count = concurrency.installs;

    // A larger buffer is significantly faster than just 1 or the worker count.
    let (sender, receiver) = async_channel::bounded::<PathBuf>(worker_count * 10);

    // Running Python with an actual file will produce better error messages.
    let tempdir = tempdir_in(cache).map_err(CompileError::TempFile)?;
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");

    debug!("Starting {} bytecode compilation workers", worker_count);
    let mut worker_handles = Vec::new();
    for _ in 0..worker_count {
        let (tx, rx) = oneshot::channel();

        let worker = worker(
            dir.to_path_buf(),
            python_executable.to_path_buf(),
            pip_compileall_py.clone(),
            receiver.clone(),
        );

        // Spawn each worker on a dedicated thread.
        std::thread::Builder::new()
            .name("uv-compile".to_owned())
            .spawn(move || {
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

        worker_handles.push(async { rx.await.unwrap() });
    }
    // Make sure the channel gets closed when all workers exit.
    drop(receiver);

    // Start the producer, sending all `.py` files to workers.
    let mut source_files = 0;
    let mut send_error = None;
    let walker = WalkDir::new(dir)
        .into_iter()
        // Otherwise we stumble over temporary files from `compileall`.
        .filter_entry(|dir| dir.file_name() != "__pycache__");
    for entry in walker {
        // Retrieve the entry and its metadata, with shared handling for IO errors
        let (entry, metadata) =
            match entry.and_then(|entry| entry.metadata().map(|metadata| (entry, metadata))) {
                Ok((entry, metadata)) => (entry, metadata),
                Err(err) => {
                    if err
                        .io_error()
                        .is_some_and(|err| err.kind() == io::ErrorKind::NotFound)
                    {
                        // The directory was removed, just ignore it
                        continue;
                    }
                    return Err(err.into());
                }
            };
        // https://github.com/pypa/pip/blob/3820b0e52c7fed2b2c43ba731b718f316e6816d1/src/pip/_internal/operations/install/wheel.py#L593-L604
        if metadata.is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            source_files += 1;
            if let Err(err) = sender.send(entry.path().to_owned()).await {
                // The workers exited.
                // If e.g. something with the Python interpreter is wrong, the workers have exited
                // with an error. We try to report this informative error and only if that fails,
                // report the send error.
                send_error = Some(err);
                break;
            }
        }
    }

    // All workers will receive an error after the last item. Note that there are still
    // up to worker_count * 10 items in the queue.
    drop(sender);

    // Make sure all workers exit regularly, avoid hiding errors.
    for result in futures::future::join_all(worker_handles).await {
        match result {
            // There spawning earlier errored due to a panic in a task.
            Err(_) => return Err(CompileError::Join),
            // The worker reports an error.
            Ok(Err(compile_error)) => return Err(compile_error),
            Ok(Ok(())) => {}
        }
    }

    if let Some(send_error) = send_error {
        // This is suspicious: Why did the channel stop working, but all workers exited
        // successfully?
        return Err(CompileError::WorkerDisappeared(send_error));
    }

    Ok(source_files)
}

async fn worker(
    dir: PathBuf,
    interpreter: PathBuf,
    pip_compileall_py: PathBuf,
    receiver: Receiver<PathBuf>,
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
        tokio::time::timeout(COMPILE_TIMEOUT, wait_until_ready)
            .await
            .map_err(|_| CompileError::StartupTimeout(COMPILE_TIMEOUT))??;

    let stderr_reader = tokio::task::spawn(async move {
        let mut child_stderr_collected: Vec<u8> = Vec::new();
        child_stderr
            .read_to_end(&mut child_stderr_collected)
            .await?;
        Ok(child_stderr_collected)
    });

    let result = worker_main_loop(receiver, child_stdin, &mut child_stdout).await;
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
        tokio::time::timeout(COMPILE_TIMEOUT, python_handle)
            .await
            .map_err(|_| CompileError::CompileTimeout {
                elapsed: COMPILE_TIMEOUT,
                source_file: source_file.clone(),
            })??;

        // This is a sanity check, if we don't get the path back something has gone wrong, e.g.
        // we're not actually running a python interpreter.
        let actual = out_line.trim_end_matches(['\n', '\r']);
        if actual != source_file {
            return Err(CompileError::WrongPath(source_file, actual.to_string()));
        }
    }
    Ok(())
}
