use std::collections::HashSet;
use std::ffi::OsString;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{env, io, panic};

use async_channel::{Receiver, SendError, Sender};
use serde::Serialize;
use tempfile::{TempDir, tempdir_in};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, instrument};
use walkdir::WalkDir;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_key::cache_digest;
use uv_configuration::Concurrency;
use uv_fs::{LockedFile, Simplified};
use uv_python::Interpreter;
use uv_static::EnvVars;

const COMPILEALL_SCRIPT: &str = include_str!("pip_compileall.py");
/// This is longer than any compilation should ever take.
const DEFAULT_COMPILE_TIMEOUT: Duration = Duration::from_mins(1);

type WorkerOutcome = std::thread::Result<Result<(), CompileError>>;
type WorkerHandle = oneshot::Receiver<WorkerOutcome>;

#[derive(Debug, Clone, Serialize)]
struct CompileTask {
    source: String,
    output: Option<String>,
    display: Option<String>,
}

impl CompileTask {
    fn in_place(source_file: &Path) -> Self {
        Self {
            source: source_file.display().to_string(),
            output: None,
            display: None,
        }
    }

    fn cached(source_file: &Path, output_file: &Path, display_file: &Path) -> Self {
        Self {
            source: source_file.display().to_string(),
            output: Some(output_file.display().to_string()),
            display: Some(display_file.display().to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("Failed to list files in `site-packages`")]
    Walkdir(#[from] walkdir::Error),
    #[error("Failed to send task to worker")]
    WorkerDisappeared,
    #[error("Failed to serialize bytecode compilation task")]
    SerializeTask(#[from] serde_json::Error),
    #[error("Failed to identify Python source files")]
    SourceFiles(#[source] anyhow::Error),
    #[error("The task executor is broken, did some other task panic?")]
    Join,
    #[error("Failed to start Python interpreter to run compile script")]
    PythonSubcommand(#[source] io::Error),
    #[error("Failed to create temporary script file")]
    TempFile(#[source] io::Error),
    #[error("Failed to access the bytecode cache")]
    BytecodeCache(#[source] io::Error),
    #[error("Failed to lock the bytecode cache")]
    BytecodeCacheLock(#[source] uv_cache::Error),
    #[error("Wheel path is not an archive in the cache: `{0}`")]
    WheelCachePath(PathBuf),
    #[error("Python interpreter does not report a bytecode cache tag")]
    MissingCacheTag,
    #[error("Python source is not contained in its wheel: `{0}`")]
    WheelSource(PathBuf),
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
    receiver: &Receiver<CompileTask>,
    worker_count: usize,
    timeout: Option<Duration>,
) -> Vec<WorkerHandle> {
    debug!("Starting {} bytecode compilation workers", worker_count);
    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let (tx, rx) = oneshot::channel();

        let worker = worker(
            dir.to_path_buf(),
            python_executable.to_path_buf(),
            pip_compileall_py.to_path_buf(),
            receiver.clone(),
            timeout,
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

        worker_handles.push(rx);
    }
    worker_handles
}

/// Wait for all workers to exit so worker failures are not hidden by channel send errors.
async fn wait_for_workers(
    worker_handles: Vec<WorkerHandle>,
    send_error: Option<SendError<CompileTask>>,
) -> Result<(), CompileError> {
    for result in futures::future::join_all(worker_handles).await {
        match result {
            // A worker thread panicked or exited without reporting its result.
            Err(_) | Ok(Err(_)) => return Err(CompileError::Join),
            Ok(Ok(Err(compile_error))) => return Err(compile_error),
            Ok(Ok(Ok(()))) => {}
        }
    }

    if send_error.is_some() {
        // This is suspicious: Why did the channel stop working, but all workers exited
        // successfully?
        return Err(CompileError::WorkerDisappeared);
    }

    Ok(())
}

fn python_source_files<'a>(
    dir: &'a Path,
    skip_wheel_data: bool,
    excluded: Option<&'a HashSet<PathBuf>>,
) -> impl Iterator<Item = Result<PathBuf, walkdir::Error>> + 'a {
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
        .filter_map(move |entry| {
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
            (metadata.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "py")
                && excluded.is_none_or(|excluded| !excluded.contains(entry.path())))
            .then(|| Ok(entry.into_path()))
        })
}

/// Return the Python source files in an unpacked wheel.
pub fn wheel_python_source_files(
    dir: &Path,
) -> impl Iterator<Item = Result<PathBuf, walkdir::Error>> + '_ {
    python_source_files(dir, true, None)
}

fn count_wheel_python_source_files(dir: &Path) -> Result<usize, CompileError> {
    wheel_python_source_files(dir).try_fold(0, |count, source| {
        source.map(|_| count + 1).map_err(CompileError::from)
    })
}

#[derive(Debug, Clone)]
struct BytecodeCacheKey {
    fingerprint: String,
    cache_tag: Box<str>,
}

impl BytecodeCacheKey {
    fn new(cache_tag: &str, magic_number: &str) -> Self {
        Self {
            fingerprint: cache_digest(&(cache_tag, magic_number, 0u8, "checked-hash")),
            cache_tag: cache_tag.into(),
        }
    }

    fn from_interpreter(interpreter: &Interpreter) -> Result<Self, CompileError> {
        let cache_tag = interpreter
            .bytecode_cache_tag()
            .ok_or(CompileError::MissingCacheTag)?;
        Ok(Self::new(cache_tag, interpreter.bytecode_magic_number()))
    }
}

/// A persistent cache of Python bytecode compiled from wheel archives.
#[derive(Debug, Clone)]
pub struct BytecodeCache {
    cache: Cache,
    key: BytecodeCacheKey,
}

impl BytecodeCache {
    fn new(cache: &Cache, interpreter: &Interpreter) -> Result<Self, CompileError> {
        Ok(Self {
            cache: cache.clone(),
            key: BytecodeCacheKey::from_interpreter(interpreter)?,
        })
    }

    fn entry(&self, wheel: &Path) -> Result<CacheEntry, CompileError> {
        let archive_id = self
            .cache
            .archive_id(wheel)
            .ok_or_else(|| CompileError::WheelCachePath(wheel.to_path_buf()))?;
        Ok(self
            .cache
            .entry(CacheBucket::Bytecode, archive_id, &self.key.fingerprint))
    }

    pub(crate) fn get(&self, wheel: &Path) -> Result<Option<PathBuf>, CompileError> {
        let entry = self.entry(wheel)?;
        match self.cache.resolve_link(entry.path()) {
            Ok(path) => Ok(Some(path)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(CompileError::BytecodeCache(err)),
        }
    }
}

struct Workers {
    sender: Sender<CompileTask>,
    worker_handles: Vec<WorkerHandle>,
    tempdir: TempDir,
}

struct PendingBytecode {
    tempdir: TempDir,
    entry: CacheEntry,
    _lock: LockedFile,
}

/// A shared pool of Python interpreters for populating the persistent bytecode cache.
pub struct BytecodeCompiler {
    cache: BytecodeCache,
    python_executable: PathBuf,
    worker_count: usize,
    workers: Mutex<Option<Workers>>,
    send_error: Mutex<Option<SendError<CompileTask>>>,
    pending: Mutex<Vec<PendingBytecode>>,
    source_files: AtomicUsize,
}

impl BytecodeCompiler {
    pub fn new(
        python_executable: &Path,
        interpreter: &Interpreter,
        concurrency: &Concurrency,
        cache: &Cache,
    ) -> Result<Self, CompileError> {
        Ok(Self {
            cache: BytecodeCache::new(cache, interpreter)?,
            python_executable: python_executable.to_path_buf(),
            worker_count: concurrency.installs,
            workers: Mutex::new(None),
            send_error: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            source_files: AtomicUsize::new(0),
        })
    }

    async fn sender(&self) -> Result<Sender<CompileTask>, CompileError> {
        let mut workers = self.workers.lock().await;
        if let Some(workers) = workers.as_ref() {
            return Ok(workers.sender.clone());
        }

        // A larger buffer is significantly faster than just 1 or the worker count.
        let (sender, receiver) = async_channel::bounded::<CompileTask>(self.worker_count * 10);
        let tempdir = tempdir_in(self.cache.cache.root()).map_err(CompileError::TempFile)?;
        let pip_compileall_py = tempdir.path().join("pip_compileall.py");
        let timeout = compile_timeout()?;
        let worker_handles = spawn_workers(
            self.cache.cache.root(),
            &self.python_executable,
            &pip_compileall_py,
            &receiver,
            self.worker_count,
            timeout,
        );
        drop(receiver);
        *workers = Some(Workers {
            sender: sender.clone(),
            worker_handles,
            tempdir,
        });
        Ok(sender)
    }

    /// Queue the Python source files in an unpacked wheel for bytecode compilation.
    pub async fn compile_wheel(&self, dir: &Path) -> Result<usize, CompileError> {
        debug_assert!(
            dir.is_absolute(),
            "compileall doesn't work with relative paths: `{}`",
            dir.display()
        );

        let entry = self.cache.entry(dir)?;
        if self.cache.get(dir)?.is_some() {
            let source_files = count_wheel_python_source_files(dir)?;
            self.source_files.fetch_add(source_files, Ordering::Relaxed);
            return Ok(source_files);
        }

        let lock_name = format!("{}.lock", self.cache.key.fingerprint);
        let lock = CacheEntry::new(entry.dir(), lock_name)
            .lock()
            .await
            .map_err(CompileError::BytecodeCacheLock)?;
        if self.cache.get(dir)?.is_some() {
            let source_files = count_wheel_python_source_files(dir)?;
            self.source_files.fetch_add(source_files, Ordering::Relaxed);
            return Ok(source_files);
        }

        let tempdir = tempdir_in(self.cache.cache.root()).map_err(CompileError::TempFile)?;
        let mut source_files = 0;
        for source_file in wheel_python_source_files(dir) {
            let source_file = source_file?;
            let relative = source_file
                .strip_prefix(dir)
                .map_err(|_| CompileError::WheelSource(source_file.clone()))?;
            let Some(parent) = relative.parent() else {
                return Err(CompileError::WheelSource(source_file));
            };
            let Some(stem) = relative.file_stem() else {
                return Err(CompileError::WheelSource(source_file));
            };
            let mut filename = OsString::from(stem);
            filename.push(format!(".{}.pyc", self.cache.key.cache_tag));
            let output_file = tempdir
                .path()
                .join(parent)
                .join("__pycache__")
                .join(filename);
            fs_err::create_dir_all(
                output_file
                    .parent()
                    .expect("bytecode output always has a parent"),
            )
            .map_err(CompileError::BytecodeCache)?;

            source_files += 1;
            let sender = self.sender().await?;
            if let Err(err) = sender
                .send(CompileTask::cached(&source_file, &output_file, relative))
                .await
            {
                let mut send_error = self.send_error.lock().await;
                if send_error.is_none() {
                    *send_error = Some(err);
                }
                break;
            }
        }
        self.source_files.fetch_add(source_files, Ordering::Relaxed);
        self.pending.lock().await.push(PendingBytecode {
            tempdir,
            entry,
            _lock: lock,
        });
        Ok(source_files)
    }

    /// Finish all queued compilation tasks and persist their bytecode cache entries.
    pub async fn finish(self) -> Result<(BytecodeCache, usize), CompileError> {
        let Self {
            cache,
            workers,
            send_error,
            pending,
            source_files,
            ..
        } = self;
        if let Some(Workers {
            sender,
            worker_handles,
            tempdir,
        }) = workers.into_inner()
        {
            drop(sender);
            wait_for_workers(worker_handles, send_error.into_inner()).await?;
            // Keep the compile script alive until all workers have exited.
            drop(tempdir);
        }

        for pending in pending.into_inner() {
            cache
                .cache
                .persist(pending.tempdir.path(), pending.entry.path())
                .await
                .map_err(CompileError::BytecodeCache)?;
        }

        Ok((cache, source_files.into_inner()))
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
    compile_tree_inner(dir, python_executable, concurrency, cache, None).await
}

/// Bytecode compile all Python source files in `dir`, except for the given paths.
pub async fn compile_tree_excluding(
    dir: &Path,
    excluded: &HashSet<PathBuf>,
    python_executable: &Path,
    concurrency: &Concurrency,
    cache: &Path,
) -> Result<usize, CompileError> {
    compile_tree_inner(dir, python_executable, concurrency, cache, Some(excluded)).await
}

async fn compile_tree_inner(
    dir: &Path,
    python_executable: &Path,
    concurrency: &Concurrency,
    cache: &Path,
    excluded: Option<&HashSet<PathBuf>>,
) -> Result<usize, CompileError> {
    debug_assert!(
        dir.is_absolute(),
        "compileall doesn't work with relative paths: `{}`",
        dir.display()
    );
    let worker_count = concurrency.installs;

    // A larger buffer is significantly faster than just 1 or the worker count.
    let (sender, receiver) = async_channel::bounded::<CompileTask>(worker_count * 10);

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
    for source_file in python_source_files(dir, false, excluded) {
        source_files += 1;
        if let Err(err) = sender.send(CompileTask::in_place(&source_file?)).await {
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
    let (sender, receiver) = async_channel::bounded::<CompileTask>(worker_count * 10);

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
        if let Err(err) = sender.send(CompileTask::in_place(&file)).await {
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
    receiver: Receiver<CompileTask>,
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
    receiver: Receiver<CompileTask>,
    mut child_stdin: ChildStdin,
    child_stdout: &mut BufReader<ChildStdout>,
    timeout: Option<Duration>,
) -> Result<(), CompileError> {
    let mut out_line = String::new();
    while let Ok(task) = receiver.recv().await {
        let source_file = task.source.clone();
        let task = serde_json::to_string(&task)?;
        // Luckily, LF alone works on windows too
        let bytes = format!("{task}\n").into_bytes();

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
        if actual != task {
            return Err(CompileError::WrongPath(task, actual.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use uv_cache::{Cache, CacheBucket};
    use uv_configuration::Concurrency;
    use uv_python::{EnvironmentPreference, PythonEnvironment, PythonPreference, PythonRequest};

    use super::{BytecodeCacheKey, BytecodeCompiler};

    #[test]
    fn bytecode_cache_key_isolates_interpreters() {
        let cpython_312 = BytecodeCacheKey::new("cpython-312", "cb0d0d0a");
        let cpython_313 = BytecodeCacheKey::new("cpython-313", "f30d0d0a");
        let other_magic = BytecodeCacheKey::new("cpython-312", "aa0d0d0a");

        assert_ne!(cpython_312.fingerprint, cpython_313.fingerprint);
        assert_ne!(cpython_312.fingerprint, other_magic.fingerprint);
    }

    #[tokio::test]
    async fn bytecode_compiler_reuses_pool_and_skips_data_directory() {
        let cache = Cache::temp().expect("cache should be available");
        let wheel = tempfile::tempdir_in(cache.root()).expect("wheel directory should exist");
        let package = wheel.path().join("package");
        let scripts = wheel.path().join("package-1.0.0.data/scripts");
        fs_err::create_dir_all(&package).expect("package directory should exist");
        fs_err::create_dir_all(&scripts).expect("scripts directory should exist");
        fs_err::write(package.join("__init__.py"), "VALUE = 1\n")
            .expect("package source should be written");
        fs_err::write(scripts.join("script.py"), "VALUE = 1\n")
            .expect("script source should be written");

        let environment = PythonEnvironment::find(
            &PythonRequest::Any,
            EnvironmentPreference::Any,
            PythonPreference::System,
            &cache,
        )
        .expect("Python environment should be available");
        let concurrency = Concurrency::new(1, 1, 1, 1);

        let other_wheel = tempfile::tempdir_in(cache.root()).expect("wheel directory should exist");
        let other_package = other_wheel.path().join("other_package");
        fs_err::create_dir_all(&other_package).expect("package directory should exist");
        fs_err::write(other_package.join("__init__.py"), "VALUE = 2\n")
            .expect("package source should be written");

        let wheel_entry = cache.bucket(CacheBucket::Wheels).join("test").join("wheel");
        cache
            .persist(wheel.path(), &wheel_entry)
            .await
            .expect("wheel should be persisted");
        let wheel = cache
            .resolve_link(&wheel_entry)
            .expect("wheel should resolve");
        let other_wheel_entry = cache
            .bucket(CacheBucket::Wheels)
            .join("test")
            .join("other-wheel");
        cache
            .persist(other_wheel.path(), &other_wheel_entry)
            .await
            .expect("other wheel should be persisted");
        let other_wheel = cache
            .resolve_link(&other_wheel_entry)
            .expect("other wheel should resolve");

        let compiler = BytecodeCompiler::new(
            environment.python_executable(),
            environment.interpreter(),
            &concurrency,
            &cache,
        )
        .expect("compiler should start");
        let compiled = compiler
            .compile_wheel(&wheel)
            .await
            .expect("wheel should be queued");
        let other_compiled = compiler
            .compile_wheel(&other_wheel)
            .await
            .expect("wheel should be queued");
        let (bytecode_cache, total) = compiler.finish().await.expect("wheels should compile");
        let wheel_bytecode = bytecode_cache
            .get(&wheel)
            .expect("bytecode lookup should succeed")
            .expect("wheel bytecode should be cached");
        let other_wheel_bytecode = bytecode_cache
            .get(&other_wheel)
            .expect("bytecode lookup should succeed")
            .expect("other wheel bytecode should be cached");

        assert_eq!(compiled, 1);
        assert_eq!(other_compiled, 1);
        assert_eq!(total, 2);
        assert!(!wheel.join("package/__pycache__").exists());
        assert!(!other_wheel.join("other_package/__pycache__").exists());
        assert!(
            wheel_bytecode
                .join("package/__pycache__")
                .read_dir()
                .expect("package bytecode directory should exist")
                .next()
                .is_some()
        );
        assert!(
            other_wheel_bytecode
                .join("other_package/__pycache__")
                .read_dir()
                .expect("other package bytecode directory should exist")
                .next()
                .is_some()
        );
        assert!(!wheel_bytecode.join("package-1.0.0.data").exists());

        let bytecode = fs_err::read(
            wheel_bytecode
                .join("package/__pycache__")
                .read_dir()
                .expect("package bytecode directory should exist")
                .next()
                .expect("package bytecode should exist")
                .expect("package bytecode should be readable")
                .path(),
        )
        .expect("package bytecode should be readable");
        let flags = bytecode
            .get(4..8)
            .expect("bytecode has a header")
            .try_into()
            .expect("bytecode flags are four bytes");
        assert_eq!(
            u32::from_le_bytes(flags),
            3,
            "cached bytecode should use checked-hash invalidation"
        );

        let compiler = BytecodeCompiler::new(
            environment.python_executable(),
            environment.interpreter(),
            &concurrency,
            &cache,
        )
        .expect("compiler should start");
        compiler
            .compile_wheel(&wheel)
            .await
            .expect("cached wheel should be reusable");
        compiler
            .compile_wheel(&other_wheel)
            .await
            .expect("other cached wheel should be reusable");
        assert!(
            compiler.workers.lock().await.is_none(),
            "cache hits should not start compilation workers"
        );
        let (_, total) = compiler.finish().await.expect("cache hits should finish");
        assert_eq!(total, 2);
    }
}
