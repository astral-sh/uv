use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::{env, io};

use tokio::process::Command;
use tracing::debug;
use walkdir::WalkDir;

use uv_bin_install::install_serc;
use uv_cache::Cache;
use uv_client::BaseClient;
use uv_configuration::Concurrency;
use uv_python_interpreter::Interpreter;
use uv_static::EnvVars;

use super::{CompileError, compile_timeout};

// Keep path arguments well below Windows' command-line limit, even after quoting.
#[cfg(windows)]
const MAX_PATH_ARGUMENT_BYTES: usize = 8192;
// Unix allows larger argument lists; avoid repeatedly starting the compiler for small batches.
#[cfg(not(windows))]
const MAX_PATH_ARGUMENT_BYTES: usize = 65536;

/// Compile a directory using the downloaded serc executable.
pub async fn compile_serc_tree(
    dir: &Path,
    interpreter: &Interpreter,
    concurrency: &Concurrency,
    cache: &Cache,
    client: &BaseClient,
) -> Result<usize, CompileError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "__pycache__")
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err)
                if err
                    .io_error()
                    .is_some_and(|err| err.kind() == io::ErrorKind::NotFound) =>
            {
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            files.push(entry.into_path());
        }
    }
    compile_serc_files(
        files.into_iter().map(Ok),
        interpreter,
        concurrency,
        cache,
        client,
    )
    .await
}

/// Compile the given source files using the downloaded serc executable.
pub async fn compile_serc_files(
    files: impl IntoIterator<Item = anyhow::Result<PathBuf>>,
    interpreter: &Interpreter,
    concurrency: &Concurrency,
    cache: &Cache,
    client: &BaseClient,
) -> Result<usize, CompileError> {
    let files = files
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(CompileError::SourceFiles)?;
    if files.is_empty() {
        return Ok(0);
    }
    let compiler = SercCompiler::new(interpreter)?;
    let timeout = compile_timeout()?;
    for file in &files {
        compiler.validate_destination(file)?;
    }
    let executable = Box::pin(install_serc(cache, client))
        .await
        .map_err(|err| CompileError::SercInstall(err.into()))?;
    let mut start = 0;
    while start < files.len() {
        let mut end = start;
        let mut size = 0;
        while let Some(file) = files.get(end) {
            // Quoting can at most double the encoded length, so budget for that too.
            let argument_size = file.as_os_str().len().saturating_mul(2).saturating_add(3);
            if argument_size > MAX_PATH_ARGUMENT_BYTES {
                return Err(CompileError::SercArgumentTooLong(file.clone()));
            }
            if size + argument_size > MAX_PATH_ARGUMENT_BYTES {
                break;
            }
            size += argument_size;
            end += 1;
        }
        loop {
            let mut command = Command::new(&executable);
            command
                .arg("compile")
                .arg("--python-version")
                .arg(&compiler.version)
                .arg("--invalidation-mode")
                .arg(compiler.invalidation_mode)
                .arg("--jobs")
                .arg(concurrency.installs.to_string())
                .arg("--quiet");
            if compiler.optimization > 0 {
                command.arg(format!(
                    "-{}",
                    "O".repeat(usize::from(compiler.optimization))
                ));
            }
            command
                .arg("--")
                .args(&files[start..end])
                .kill_on_drop(true);
            // The Python compiler applies this timeout per file. A batch can use the sum of those
            // budgets because serc does not report per-file progress.
            let result = if let Some(timeout) = timeout {
                let count = u32::try_from(end - start).unwrap_or(u32::MAX);
                let timeout = timeout.saturating_mul(count);
                tokio::time::timeout(timeout, command.output())
                    .await
                    .map_err(|_| CompileError::SercTimeout(timeout))?
            } else {
                command.output().await
            };
            let output = match result {
                // The environment and platform limits also count toward the argument limit.
                // Retry a smaller batch when the OS refuses to start the subprocess.
                Err(err) if err.kind() == io::ErrorKind::ArgumentListTooLong && end - start > 1 => {
                    end = start + (end - start) / 2;
                    continue;
                }
                result => result.map_err(CompileError::SercSubcommand)?,
            };
            if !output.status.success() {
                return Err(CompileError::SercFailure {
                    status: output.status,
                    stderr: String::from_utf8_lossy(&output.stderr)
                        .trim_end()
                        .to_owned(),
                });
            }
            start = end;
            break;
        }
    }
    Ok(files.len())
}

struct SercCompiler {
    version: String,
    cache_suffix: String,
    optimization: u8,
    invalidation_mode: &'static str,
}

impl SercCompiler {
    fn new(interpreter: &Interpreter) -> Result<Self, CompileError> {
        if interpreter.implementation_name() != "cpython" {
            return Err(CompileError::NativeImplementation(
                interpreter.implementation_name().to_owned(),
            ));
        }
        let version = interpreter.python_minor_version().to_string();
        // Serc 0.1.6 supports these bytecode formats. Prereleases can change magic within a minor.
        let magic = match version.as_str() {
            "3.12" => [0xcb, 0x0d, 0x0d, 0x0a],
            "3.13" => [0xf3, 0x0d, 0x0d, 0x0a],
            "3.14" => [0x2b, 0x0e, 0x0d, 0x0a],
            "3.15" => [0x52, 0x0e, 0x0d, 0x0a],
            _ => return Err(CompileError::NativeVersion(version)),
        };
        if interpreter.bytecode_magic_number() != magic {
            return Err(CompileError::NativeMagicNumber);
        }
        // The CLI always writes to its built-in CPython cache tag.
        let expected_tag = format!(
            "cpython-{}{}",
            interpreter.python_major(),
            interpreter.python_minor()
        );
        if interpreter.cache_tag() != Some(expected_tag.as_str()) {
            return Err(CompileError::NativeCacheTag);
        }
        for (variable, setting) in [
            ("PYTHONPYCACHEPREFIX", "a custom bytecode cache prefix"),
            ("PYTHONNODEBUGRANGES", "omitting debug ranges"),
        ] {
            if env::var_os(variable).is_some_and(|value| !value.is_empty()) {
                return Err(CompileError::NativeSetting { setting, variable });
            }
        }
        // Python treats any nonempty value other than a nonnegative C int as level one.
        let optimization = env::var_os("PYTHONOPTIMIZE")
            .filter(|value| !value.is_empty())
            .map_or(0, |value| {
                value
                    .to_string_lossy()
                    .trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r'])
                    .parse::<i32>()
                    .ok()
                    .filter(|value| *value >= 0)
                    .unwrap_or(1)
            });
        let optimization = u8::try_from(optimization).map_err(|_| CompileError::NativeSetting {
            setting: "optimization levels above 255",
            variable: "PYTHONOPTIMIZE",
        })?;
        let cache_suffix = if optimization == 0 {
            format!("{expected_tag}.pyc")
        } else {
            format!("{expected_tag}.opt-{optimization}.pyc")
        };
        let invalidation_mode = match env::var_os(EnvVars::PYC_INVALIDATION_MODE) {
            Some(value) => match value.to_str() {
                Some("TIMESTAMP") => "timestamp",
                Some("CHECKED_HASH") => "checked-hash",
                Some("UNCHECKED_HASH") => "unchecked-hash",
                _ => {
                    return Err(CompileError::EnvironmentError {
                        var: EnvVars::PYC_INVALIDATION_MODE,
                        message: format!(
                            "Expected TIMESTAMP, CHECKED_HASH, or UNCHECKED_HASH, got \"{}\"",
                            value.display()
                        ),
                    });
                }
            },
            None if env::var_os("SOURCE_DATE_EPOCH").is_some_and(|value| !value.is_empty()) => {
                "checked-hash"
            }
            None => "timestamp",
        };
        debug!(
            "Using serc for Python {version} bytecode compilation at optimization level {optimization}"
        );
        Ok(Self {
            version,
            cache_suffix,
            optimization,
            invalidation_mode,
        })
    }

    fn validate_destination(&self, source: &Path) -> Result<(), CompileError> {
        let parent = source
            .parent()
            .ok_or_else(|| CompileError::SercDestination(source.to_path_buf()))?;
        let filename = source
            .file_name()
            .ok_or_else(|| CompileError::SercDestination(source.to_path_buf()))?;
        let separator = filename
            .as_encoded_bytes()
            .iter()
            .rposition(|&byte| byte == b'.');
        let mut stem = if separator == Some(0) {
            // Serc treats a lone leading dot as an extension separator, like CPython.
            let mut prefixed = OsString::from("_");
            prefixed.push(filename);
            Path::new(&prefixed).extension().map(OsString::from)
        } else {
            source.file_stem().map(OsString::from)
        }
        .ok_or_else(|| CompileError::SercDestination(source.to_path_buf()))?;
        if separator.is_some() {
            stem.push(".");
        }
        stem.push(&self.cache_suffix);
        let destination = parent.join("__pycache__").join(stem);
        match fs_err::symlink_metadata(&destination) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                Err(CompileError::SercDestination(destination))
            }
            Ok(_) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(CompileError::SercDestinationIo { destination, err }),
        }
    }
}
