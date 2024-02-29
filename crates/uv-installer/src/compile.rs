use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_channel::{Receiver, SendError};
use tempfile::tempdir;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::task::JoinError;
use walkdir::WalkDir;

const COMPILEALL_SCRIPT: &str = include_str!("pip_compileall.py");
const MAIN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Failed to list files in site-packages")]
    Walkdir(#[from] walkdir::Error),
    #[error("Couldn't determine number of cores")]
    AvailableParallelism(#[source] io::Error),
    #[error("Failed to send task to worker")]
    WorkerDisappeared(SendError<PathBuf>),
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Failed to start Python interpreter to run compile script")]
    PythonSubcommand(#[source] io::Error),
    #[error("Failed to create temporary script file")]
    TempFile(#[source] io::Error),
    #[error("Bytecode compilation failed, expected `{0}`, got: `{1}`")]
    WrongPath(String, String),
    #[error("Failed to write to Python stdin")]
    ChildStdin(#[source] io::Error),
    #[error("Failed to read Python stdout")]
    ChildStdout(#[source] io::Error),
    #[error("Bytecode timed out ({}s)", _0.as_secs_f32())]
    Timeout(Duration),
}

/// Bytecode compile all file in `dir` using a pool of work-stealing python interpreters running a
/// Python script that calls `compileall.compile_file`.
///
/// All compilation errors are muted (like pip). There is a 10s timeout to catch anything gotten
/// stuck; This happens way to easily with channels and subprocesses, e.g. because some pipe is
/// full, we're waiting when it's buffered or we didn't handle channel closing properly.
///
/// We only compile all files, but we don't update the RECORD, relying on PEP 491:
/// > Uninstallers should be smart enough to remove .pyc even if it is not mentioned in RECORD.
pub async fn compile_tree(dir: &Path, python_executable: &Path) -> Result<usize, CompileError> {
    let workers =
        std::thread::available_parallelism().map_err(CompileError::AvailableParallelism)?;
    // 10 is an arbitrary number.
    let (sender, receiver) = async_channel::bounded::<PathBuf>(workers.get() * 10);

    // Start the workers.
    let mut worker_handles = Vec::new();
    for _ in 0..workers.get() {
        worker_handles.push(tokio::task::spawn(worker(
            dir.to_path_buf(),
            python_executable.to_path_buf(),
            receiver.clone(),
        )));
    }

    // Start the producer, sending all `.py` files to workers.
    let mut source_files = 0;
    for entry in WalkDir::new(dir) {
        let entry = entry?;
        // https://github.com/pypa/pip/blob/3820b0e52c7fed2b2c43ba731b718f316e6816d1/src/pip/_internal/operations/install/wheel.py#L593-L604
        if entry.metadata()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            source_files += 1;
            match tokio::time::timeout(MAIN_TIMEOUT, sender.send(entry.path().to_owned())).await {
                // The workers are stuck.
                Err(_) => return Err(CompileError::Timeout(MAIN_TIMEOUT)),
                // The workers exited.
                // TODO(konstin): Should we check for worker errors here?
                Ok(Err(err)) => return Err(CompileError::WorkerDisappeared(err)),
                Ok(Ok(())) => {}
            }
        }
    }

    // All workers will receive an error after the last item. Note that there are still
    // up to workers * 10 items in the queue.
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

    Ok(source_files)
}

async fn worker(
    dir: PathBuf,
    interpreter: PathBuf,
    receiver: Receiver<PathBuf>,
) -> Result<(), CompileError> {
    // Running python with an actual file will produce better error messages.
    let tempdir = tempdir().map_err(CompileError::TempFile)?;
    let pip_compileall_py = tempdir.path().join("pip_compileall.py");
    fs_err::tokio::write(&pip_compileall_py, COMPILEALL_SCRIPT)
        .await
        .map_err(CompileError::TempFile)?;
    // We input the paths through stdin and get the successful paths returned through stdout.
    let mut bytecode_compiler = Command::new(interpreter)
        .arg(&pip_compileall_py)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .current_dir(dir)
        // Otherwise stdout is buffered and we'll wait forever for a response
        .env("PYTHONUNBUFFERED", "1")
        .spawn()
        .map_err(CompileError::PythonSubcommand)?;

    // https://stackoverflow.com/questions/49218599/write-to-child-process-stdin-in-rust/49597789#comment120223107_49597789
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

    let result = worker_main_loop(receiver, child_stdin, &mut child_stdout).await;
    // Avoid `/.venv/bin/python: can't open file` because the tempdir was dropped before the
    // python even started.
    let _ = bytecode_compiler.kill().await;
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
        // TODO(konstin): Does LF alone work on windows too?
        let bytes = format!("{}\n", source_file.display()).into_bytes();
        // Ensure we don't get stuck here because some pipe ran full. The Python process isn't doing
        // anything at this point except waiting for stdin, so this should be immediate.
        let timeout = Duration::from_secs(1);
        match tokio::time::timeout(timeout, child_stdin.write_all(&bytes)).await {
            // Timeout
            Err(_) => {
                receiver.close();
                return Err(CompileError::Timeout(timeout));
            }
            Ok(Err(err)) => return Err(CompileError::ChildStdin(err)),
            Ok(Ok(())) => {}
        }

        out_line.clear();
        child_stdout
            .read_line(&mut out_line)
            .await
            .map_err(CompileError::ChildStdout)?;

        // This is a sanity check, if we don't get the path back something has gone wrong, e.g.
        // we're not actually running a python interpreter.
        let actual = out_line.strip_suffix(['\n', '\r']).unwrap_or(&out_line);
        let expected = source_file.display().to_string();
        if expected != actual {
            return Err(CompileError::WrongPath(expected, actual.to_string()));
        }
    }
    Ok(())
}
