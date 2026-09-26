#[cfg(unix)]
use std::fs::Permissions;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, bail};
use serc::{CompileOptions, PythonVersion};
use serde::Deserialize;
use tokio::process::Command;
use tracing::debug;

use uv_fs::{persist_with_retry_sync, tempfile_in};

use super::CompileError;

/// A serc compiler configured for the target interpreter's bytecode format.
#[derive(Clone)]
pub(super) struct RustCompiler {
    options: CompileOptions,
    cache_tag: String,
}

#[derive(Deserialize)]
struct Target {
    python_version: String,
    cache_tag: String,
    magic_number: [u8; 4],
}

impl RustCompiler {
    /// Probe the interpreter once, rejecting unsupported formats or settings.
    pub(super) async fn query(
        dir: &Path,
        python_executable: &Path,
        timeout: Option<Duration>,
    ) -> anyhow::Result<Self> {
        let mut command = Command::new(python_executable);
        command
            .arg("-c")
            .arg(include_str!("target.py"))
            .current_dir(dir)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let output = if let Some(duration) = timeout {
            tokio::time::timeout(duration, command.output())
                .await
                .map_err(|_| CompileError::StartupTimeout(duration))??
        } else {
            command.output().await?
        };
        if !output.status.success() {
            bail!(
                "Bytecode target query failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let target = serde_json::from_slice::<Target>(&output.stdout)?;
        let python_version = target
            .python_version
            .parse::<PythonVersion>()
            .with_context(|| format!("serc does not support Python {}", target.python_version))?;
        // In particular, prereleases can change bytecode formats within the same minor version.
        if target.magic_number != python_version.target().magic_number {
            bail!(
                "serc does not support the bytecode magic number for this Python {python_version} interpreter"
            );
        }
        debug!("Using serc for Python {python_version} bytecode compilation");
        Ok(Self {
            options: CompileOptions {
                python_version,
                ..CompileOptions::default()
            },
            cache_tag: target.cache_tag,
        })
    }

    /// Compile a source file, reusing current bytecode and atomically replacing stale bytecode.
    pub(super) fn compile(&self, source_file: &Path) -> anyhow::Result<()> {
        let metadata = fs_err::metadata(source_file)?;
        // CPython stores the low 32 bits of the source timestamp and size in the pyc header.
        let source_mtime = u32::try_from(
            metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs() & u64::from(u32::MAX),
        )?;
        let source_size = u32::try_from(metadata.len() & u64::from(u32::MAX))?;
        let parent = source_file.parent().context("Source file has no parent")?;
        let mut filename = source_file
            .file_stem()
            .context("Source file has no stem")?
            .to_os_string();
        filename.push(format!(".{}.pyc", self.cache_tag));
        let cache_dir = parent.join("__pycache__");
        let bytecode_file = cache_dir.join(filename);

        // Like py_compile, never replace symlinks or special files with bytecode.
        if let Ok(metadata) = fs_err::symlink_metadata(&bytecode_file)
            && !metadata.file_type().is_file()
        {
            bail!("Bytecode destination is not a regular file");
        }

        let mut expected = [0; 16];
        expected[..4].copy_from_slice(&self.options.python_version.target().magic_number);
        expected[8..12].copy_from_slice(&source_mtime.to_le_bytes());
        expected[12..].copy_from_slice(&source_size.to_le_bytes());
        let mut header = [0; 16];
        if let Ok(mut file) = fs_err::File::open(&bytecode_file)
            && file.read_exact(&mut header).is_ok()
            && header == expected
        {
            return Ok(());
        }

        let source = fs_err::read(source_file)?;
        let module =
            serc::compile_bytes_with_path_and_options(&source, source_file, &self.options)?;
        let bytecode = module.to_timestamp_pyc(source_mtime, source_size);
        fs_err::create_dir_all(&cache_dir)?;
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
        Ok(())
    }
}
