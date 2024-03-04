use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_channel::{Receiver, SendError};
use tempfile::tempdir;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::task::JoinError;
use tracing::{debug, instrument};
use uv_fs::Simplified;
use walkdir::WalkDir;

use uv_warnings::warn_user;

const COMPILEALL_SCRIPT: &str = include_str!("pip_compileall.py");
const MAIN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Failed to list files in site-packages")]
    Walkdir(#[from] walkdir::Error),
    #[error("Failed to send task to worker")]
    WorkerDisappeared(SendError<PathBuf>),
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Failed to start Python interpreter to run compile script")]
    PythonSubcommand(#[source] io::Error),
    #[error("Failed to create temporary script file")]
    TempFile(#[source] io::Error),
    #[error("Bytecode compilation failed, expected {0:?}, got: {1:?}")]
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
    #[error("Bytecode timed out ({}s)", _0.as_secs_f32())]
    Timeout(Duration),
}

/// Bytecode compile all file in `dir` using a pool of work-stealing Python interpreters running a
/// Python script that calls `compileall.compile_file`.
///
/// All compilation errors are muted (like pip). There is a 10s timeout to handle the case that
/// the workers have gotten stuck. This happens way too easily with channels and subprocesses, e.g.
/// because some pipe is full, we're waiting when it's buffered, or we didn't handle channel closing
/// properly.
///
/// We only compile all files, but we don't update the RECORD, relying on PEP 491:
/// > Uninstallers should be smart enough to remove .pyc even if it is not mentioned in RECORD.
/// I've checked that pip 24.0 does remove the `__pycache__` directory.
#[instrument(skip(python_executable))]
pub async fn compile_tree(dir: &Path, python_executable: &Path) -> Result<usize, CompileError> {
    debug_assert!(
        dir.is_absolute(),
        "compileall doesn't work with relative paths"
    );
    let worker_count = std::thread::available_parallelism().unwrap_or_else(|err| {
        warn_user!("Couldn't determine number of cores, compiling with a single thread: {err}");
        NonZeroUsize::MIN
    });

    let (sender, receiver) = async_channel::bounded::<PathBuf>(worker_count.get() * 10);

    // Running python with an actual file will produce better error messages.
    let tempdir = tempdir().map_err(CompileError::TempFile)?;
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");

    // Start the workers.
    let mut worker_handles = Vec::new();
    for _ in 0..worker_count.get() {
        worker_handles.push(tokio::task::spawn(worker(
            dir.to_path_buf(),
            python_executable.to_path_buf(),
            pip_compileall_py.clone(),
            receiver.clone(),
        )));
    }

    // Start the producer, sending all `.py` files to workers.
    let mut source_files = 0;
    let mut send_error = None;
    let walker = WalkDir::new(dir)
        .into_iter()
        // Otherwise we stumble over temporary files from `compileall`
        .filter_entry(|dir| dir.file_name() != "__pycache__");
    for entry in walker {
        let entry = entry?;
        // https://github.com/pypa/pip/blob/3820b0e52c7fed2b2c43ba731b718f316e6816d1/src/pip/_internal/operations/install/wheel.py#L593-L604
        if entry.metadata()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            source_files += 1;
            match tokio::time::timeout(MAIN_TIMEOUT, sender.send(entry.path().to_owned())).await {
                // The workers are stuck.
                // If we hit this condition, none of the workers made progress in the last 10s.
                // For reference, on my desktop compiling a venv with "jupyter plotly django" while
                // a cargo build was also running the slowest file took 100ms.
                Err(_) => return Err(CompileError::Timeout(MAIN_TIMEOUT)),
                // The workers exited.
                // If e.g. something with the Python interpreter is wrong, the workers have exited
                // with an error. We try to report this informative error and only if that fails,
                // report the send error.
                Ok(Err(err)) => send_error = Some(err),
                Ok(Ok(())) => {}
            }
        }
    }

    // All workers will receive an error after the last item. Note that there are still
    // up to worker_count * 10 items in the queue.
    drop(sender);

    // Make sure all workers exit regularly, avoid hiding errors.
    let results =
        match tokio::time::timeout(MAIN_TIMEOUT, futures::future::join_all(worker_handles)).await {
            Err(_) => {
                // If this happens, we waited more than 10s for n * 10 files on n workers. It
                // could happen when the user's io is bad (e.g. an overloaded misconfigured
                // network storage), there are unreasonably sized source files, but more likely
                // there is a bug in uv (including strange python configurations we need to work
                // around), which we'd like to know.
                return Err(CompileError::Timeout(MAIN_TIMEOUT));
            }
            Ok(results) => results,
        };
    for result in results {
        match result {
            // There spawning earlier errored due to a panic in a task.
            Err(join_err) => return Err(CompileError::Join(join_err)),
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
    // We input the paths through stdin and get the successful paths returned through stdout.
    let mut bytecode_compiler = Command::new(&interpreter)
        .arg(&pip_compileall_py)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(dir)
        // Otherwise stdout is buffered and we'll wait forever for a response
        .env("PYTHONUNBUFFERED", "1")
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
    let mut child_stderr = BufReader::new(
        bytecode_compiler
            .stderr
            .take()
            .expect("Child must have stderr"),
    );

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
        .await?
        .map_err(|err| CompileError::ChildStdio {
            device: "stderr",
            err,
        })?;
    if !child_stderr_collected.is_empty() {
        let stderr = String::from_utf8_lossy(&child_stderr_collected);
        return match result {
            Ok(()) => {
                debug!(
                    "Bytecode compilation `python` at {} stderr:\n{}\n---",
                    interpreter.simplified_display(),
                    stderr
                );
                Ok(())
            }
            Err(err) => Err(CompileError::ErrorWithStderr {
                stderr: stderr.to_string(),
                err: Box::new(err),
            }),
        };
    }

    result
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

        child_stdin
            .write_all(&bytes)
            .await
            .map_err(|err| CompileError::ChildStdio {
                device: "stdin",
                err,
            })?;

        out_line.clear();
        child_stdout
            .read_line(&mut out_line)
            .await
            .map_err(|err| CompileError::ChildStdio {
                device: "stdout",
                err,
            })?;

        // This is a sanity check, if we don't get the path back something has gone wrong, e.g.
        // we're not actually running a python interpreter.
        let actual = out_line.trim_end_matches(['\n', '\r']);
        if actual != source_file {
            return Err(CompileError::WrongPath(source_file, actual.to_string()));
        }
    }
    Ok(())
}
