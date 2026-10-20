use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{env, io};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::Deserialize;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::process::Command;
use tracing::debug;
use walkdir::WalkDir;

use uv_bin_install::install_serc;
use uv_cache::Cache;
use uv_client::BaseClient;
use uv_configuration::Concurrency;
use uv_python_interpreter::Interpreter;
use uv_static::EnvVars;
use uv_threads::initialize_rayon_once;

use super::{CompileError, compile_timeout};

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
    let validate = |file: &PathBuf| {
        if let Err(error) = compiler.validate_destination(file) {
            return Some(error);
        }
        #[cfg(windows)]
        if file.to_str().is_none() {
            return Some(CompileError::SercPathEncoding(file.clone()));
        }
        None
    };
    let error = if files.len() <= 1 {
        files.iter().find_map(validate)
    } else {
        initialize_rayon_once();
        files.par_iter().find_map_first(validate)
    };
    if let Some(error) = error {
        return Err(error);
    }
    let executable = Box::pin(install_serc(cache, client))
        .await
        .map_err(|err| CompileError::SercInstall(err.into()))?;
    let mut command = Command::new(&executable);
    command
        .arg("compile")
        .arg("--files-from")
        .arg("-")
        .arg("--null")
        .arg("--skip-current")
        .arg("--output-format")
        .arg("json")
        .arg("--python-version")
        .arg(&compiler.version)
        .arg("--invalidation-mode")
        .arg(compiler.invalidation_mode)
        .arg("--jobs")
        .arg(concurrency.installs.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if compiler.optimization > 0 {
        command.arg(format!(
            "-{}",
            "O".repeat(usize::from(compiler.optimization))
        ));
    }
    let compile = async {
        let mut child = command.spawn().map_err(CompileError::SercSubcommand)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CompileError::SercStdin(io::Error::other("missing stdin pipe")))?;
        let write = async {
            let mut stdin = BufWriter::new(stdin);
            for file in &files {
                // These are raw path bytes on Unix and, after validation above, UTF-8 on Windows.
                stdin.write_all(file.as_os_str().as_encoded_bytes()).await?;
                stdin.write_all(b"\0").await?;
            }
            stdin.flush().await?;
            drop(stdin);
            Ok::<_, io::Error>(())
        };
        let (written, output) = tokio::join!(write, child.wait_with_output());
        let output = output.map_err(CompileError::SercSubcommand)?;
        // Prefer the compiler's diagnostic if it exits before consuming the file list.
        if !output.status.success() {
            return Err(CompileError::SercFailure {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_owned(),
            });
        }
        written.map_err(CompileError::SercStdin)?;
        Ok(output)
    };
    // The Python compiler applies this timeout per file. Serc uses the combined budget because
    // its summary does not report per-file progress.
    let output = if let Some(timeout) = timeout {
        let count = u32::try_from(files.len()).unwrap_or(u32::MAX);
        let timeout = timeout.saturating_mul(count);
        tokio::time::timeout(timeout, compile)
            .await
            .map_err(|_| CompileError::SercTimeout(timeout))??
    } else {
        compile.await?
    };
    let summary: SercSummary =
        serde_json::from_slice(&output.stdout).map_err(CompileError::SercOutput)?;
    // Serc deduplicates paths, so its total can be smaller than the submitted file list.
    if summary
        .compiled
        .checked_add(summary.skipped)
        .is_none_or(|total| total > files.len())
    {
        return Err(CompileError::SercOutputCount {
            compiled: summary.compiled,
            skipped: summary.skipped,
            submitted: files.len(),
        });
    }
    Ok(summary.compiled)
}

#[derive(Deserialize)]
struct SercSummary {
    compiled: usize,
    skipped: usize,
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
        // Serc 0.1.7 supports these bytecode formats. Prereleases can change magic within a minor.
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
