use std::env;
#[cfg(unix)]
use std::fs::Permissions;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use anyhow::{Context, bail};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use serc::{CompileOptions, OptimizationLevel, PythonVersion, SourceDecodeError};
use siphasher::sip::SipHasher13;
use tracing::debug;
use walkdir::WalkDir;

use uv_fs::{Simplified, persist_with_retry_sync, tempfile_in};
use uv_python::Interpreter;
use uv_static::EnvVars;
use uv_threads::initialize_rayon_once;

use super::CompileError;

/// A serc compiler configured for the target interpreter's bytecode format.
pub(super) struct SercCompiler {
    options: CompileOptions,
    cache_suffix: String,
    invalidation_mode: InvalidationMode,
}

/// How Python determines whether cached bytecode matches its source.
enum InvalidationMode {
    Timestamp,
    Hash { checked: bool },
}

impl SercCompiler {
    /// Configure serc from cached interpreter metadata and the current environment.
    pub(super) fn new(interpreter: &Interpreter) -> Result<Self, CompileError> {
        if interpreter.implementation_name() != "cpython" {
            return Err(CompileError::NativeImplementation(
                interpreter.implementation_name().to_owned(),
            ));
        }
        let python_version = interpreter
            .python_minor_version()
            .to_string()
            .parse::<PythonVersion>()?;
        // Prereleases can change bytecode formats within the same minor version.
        if interpreter.bytecode_magic_number() != python_version.target().magic_number {
            return Err(CompileError::NativeMagicNumber);
        }
        let cache_tag = interpreter
            .cache_tag()
            .ok_or(CompileError::NativeCacheTag)?;
        for (variable, setting) in [
            ("PYTHONPYCACHEPREFIX", "a custom bytecode cache prefix"),
            ("PYTHONNODEBUGRANGES", "omitting debug ranges"),
        ] {
            if env::var_os(variable).is_some_and(|value| !value.is_empty()) {
                return Err(CompileError::NativeSetting { setting, variable });
            }
        }

        // Python treats any nonempty value other than a nonnegative C int as level one.
        // Levels above two use -OO semantics but retain their requested cache filename suffix.
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
        let optimization_level = match optimization {
            0 => OptimizationLevel::Zero,
            1 => OptimizationLevel::One,
            _ => OptimizationLevel::Two,
        };
        let cache_suffix = if optimization == 0 {
            format!("{cache_tag}.pyc")
        } else {
            format!("{cache_tag}.opt-{optimization}.pyc")
        };

        let invalidation_mode = match env::var_os(EnvVars::PYC_INVALIDATION_MODE) {
            Some(value) => match value.to_str() {
                Some("TIMESTAMP") => InvalidationMode::Timestamp,
                Some("CHECKED_HASH") => InvalidationMode::Hash { checked: true },
                Some("UNCHECKED_HASH") => InvalidationMode::Hash { checked: false },
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
                InvalidationMode::Hash { checked: true }
            }
            None => InvalidationMode::Timestamp,
        };
        debug!(
            "Using serc for Python {python_version} bytecode compilation at optimization level {optimization}"
        );
        Ok(Self {
            options: CompileOptions {
                python_version,
                optimization_level,
                ..CompileOptions::default()
            },
            cache_suffix,
            invalidation_mode,
        })
    }

    /// Compile a directory in parallel and count newly compiled files.
    pub(super) fn compile_tree(&self, dir: &Path) -> Result<usize, CompileError> {
        let mut files = Vec::new();
        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|entry| entry.file_name() != "__pycache__")
            .filter(|entry| {
                entry.as_ref().map_or(true, |entry| {
                    entry.file_type().is_file()
                        && entry.path().extension().is_some_and(|ext| ext == "py")
                })
            })
        {
            match entry {
                Ok(entry) => files.push(entry.into_path()),
                Err(err)
                    if err
                        .io_error()
                        .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound) => {}
                Err(err) => return Err(CompileError::Walkdir(err)),
            }
        }
        self.compile_files(&files)
    }

    /// Compile installed files in parallel and count newly compiled files.
    pub(super) fn compile_files(&self, files: &[PathBuf]) -> Result<usize, CompileError> {
        initialize_rayon_once();
        let cache_directories: FxHashMap<_, _> = files
            .iter()
            .filter_map(|path| path.parent())
            .map(|parent| (parent, OnceLock::new()))
            .collect();
        files
            .par_iter()
            .map(|source_file| {
                let cache_directory = source_file
                    .parent()
                    .and_then(|parent| cache_directories.get(parent));
                self.compile(source_file, cache_directory)
                    .map(usize::from)
                    .map_err(|err| CompileError::NativeCompile {
                        source_file: source_file.clone(),
                        err,
                    })
            })
            .try_reduce(|| 0, |left, right| Ok(left + right))
    }

    /// Compile a source file, returning false for current bytecode or invalid Python source.
    fn compile(
        &self,
        source_file: &Path,
        cache_directory: Option<&OnceLock<()>>,
    ) -> anyhow::Result<bool> {
        let metadata = fs_err::metadata(source_file)?;
        let parent = source_file.parent().context("Source file has no parent")?;
        let filename = source_file
            .file_name()
            .context("Source file has no filename")?;
        let cache_dir = parent.join("__pycache__");
        let bytecode_file = cache_dir.join(filename).with_extension(&self.cache_suffix);

        // Like py_compile, never replace symlinks or special files with bytecode.
        if let Ok(metadata) = fs_err::symlink_metadata(&bytecode_file)
            && !metadata.file_type().is_file()
        {
            bail!("Bytecode destination is not a regular file");
        }

        let magic_number = self.options.python_version.target().magic_number;
        let mut expected = [0; 16];
        expected[..4].copy_from_slice(&magic_number);
        let source = match self.invalidation_mode {
            InvalidationMode::Timestamp => {
                // CPython stores the low 32 bits of the source timestamp and size.
                let source_mtime = u32::try_from(
                    metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs()
                        & u64::from(u32::MAX),
                )?;
                let source_size = u32::try_from(metadata.len() & u64::from(u32::MAX))?;
                expected[8..12].copy_from_slice(&source_mtime.to_le_bytes());
                expected[12..].copy_from_slice(&source_size.to_le_bytes());
                None
            }
            InvalidationMode::Hash { checked } => {
                let source = fs_err::read(source_file)?;
                expected[4..8].copy_from_slice(&(1 | (u32::from(checked) << 1)).to_le_bytes());
                let key = u64::from(u32::from_le_bytes(magic_number));
                let hash = SipHasher13::new_with_keys(key, 0).hash(&source);
                expected[8..].copy_from_slice(&hash.to_le_bytes());
                Some(source)
            }
        };
        let mut header = [0; 16];
        if let Ok(mut file) = fs_err::File::open(&bytecode_file)
            && file.read_exact(&mut header).is_ok()
            && header == expected
        {
            return Ok(false);
        }

        let source = source.map_or_else(|| fs_err::read(source_file), Ok)?;
        let module =
            match serc::compile_bytes_with_path_and_options(&source, source_file, &self.options) {
                Ok(module) => module,
                // Wheels may contain invalid Python or vendored Python 2 source. Like pip, skip
                // these files; unsupported valid syntax and compiler failures remain errors.
                Err(
                    err @ (serc::CompileError::Parse(_)
                    | serc::CompileError::InvalidPattern { .. }
                    | serc::CompileError::InvalidSyntax { .. }
                    | serc::CompileError::InvalidDocstring { .. }
                    | serc::CompileError::Decode(
                        SourceDecodeError::MultipleUtf8Boms
                        | SourceDecodeError::Utf8BomConflict(_)
                        | SourceDecodeError::InvalidEncoding(_)
                        | SourceDecodeError::InvalidUtf8(_)
                        | SourceDecodeError::NullByte,
                    )),
                ) => {
                    debug!(
                        "Skipping invalid Python source {}: {err}",
                        source_file.user_display()
                    );
                    return Ok(false);
                }
                Err(err) => return Err(err.into()),
            };
        let bytecode = match self.invalidation_mode {
            InvalidationMode::Timestamp => module.to_timestamp_pyc(
                u32::from_le_bytes(expected[8..12].try_into()?),
                u32::from_le_bytes(expected[12..16].try_into()?),
            ),
            InvalidationMode::Hash { checked } => module.to_hash_pyc(&source, checked),
        };
        // Initialize lazily so invalid source and current bytecode do not require a writable
        // directory. Concurrent first writes may race to create it; later writes reuse it.
        if cache_directory.is_none_or(|directory| directory.get().is_none()) {
            fs_err::create_dir_all(&cache_dir)?;
            if let Some(directory) = cache_directory {
                let _ = directory.set(());
            }
        }
        let mut file = tempfile_in(&cache_dir)?;
        // Match py_compile: inherit the source's read permissions and make the bytecode writable
        // by its owner. Creation applies the process umask.
        #[cfg(unix)]
        file.as_file().set_permissions(Permissions::from_mode(
            file.as_file().metadata()?.permissions().mode()
                & (metadata.permissions().mode() | 0o200),
        ))?;
        file.write_all(&bytecode)?;
        persist_with_retry_sync(file, &bytecode_file)?;
        Ok(true)
    }
}
