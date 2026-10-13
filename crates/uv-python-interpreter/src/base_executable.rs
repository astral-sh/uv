use std::collections::HashSet;
use std::env::consts::EXE_SUFFIX;
use std::io;
use std::path::{Path, PathBuf};

use same_file::is_same_file;
use tracing::debug;

use uv_cache::Cache;
use uv_fs::{Simplified, is_virtualenv_base};

use crate::{Interpreter, PyVenvConfiguration};

/// Find a base executable without relying on interpreter-specific private Python attributes.
pub(crate) fn resolve(interpreter: &Interpreter, cache: &Cache) -> io::Result<PathBuf> {
    if !interpreter.is_virtualenv() {
        return std::path::absolute(interpreter.sys_executable());
    }

    let cfg = PyVenvConfiguration::parse(interpreter.sys_prefix().join("pyvenv.cfg")).ok();
    let mut candidates = Vec::new();
    if let Some(executable) = cfg.as_ref().and_then(PyVenvConfiguration::base_executable) {
        candidates.push(executable.to_path_buf());
    }
    if let Some(executable) = cfg.as_ref().and_then(|cfg| cfg.executable.as_ref()) {
        candidates.push(executable.clone());
    }

    let base_directory = if cfg!(windows) {
        interpreter.sys_base_prefix().to_path_buf()
    } else {
        interpreter.sys_base_prefix().join("bin")
    };
    let key = interpreter.key();
    for directory in cfg
        .as_ref()
        .and_then(|cfg| cfg.home.as_ref())
        .into_iter()
        .chain(Some(&base_directory))
    {
        if let Some(name) = interpreter.sys_executable().file_name() {
            candidates.push(directory.join(name));
        }
        candidates.push(directory.join(key.executable_name_minor()));
        candidates.push(directory.join(key.executable_name_major()));
        candidates.push(directory.join(key.executable_name()));
        candidates.push(directory.join(format!("python{EXE_SUFFIX}")));
    }

    let target = executable_target(interpreter);
    if let Some(target) = &target {
        candidates.push(target.clone());
    }

    let mut visited = HashSet::new();
    for candidate in candidates {
        let candidate = std::path::absolute(candidate)?;
        if !visited.insert(candidate.clone()) || !candidate.is_file() {
            continue;
        }

        // Symlinks and uv's launchers identify the executable directly. Keep the selected alias
        // when it points to that file, without querying the base interpreter again.
        if let Some(target) = &target {
            if !is_same_file(&candidate, target).unwrap_or(false) {
                continue;
            }
            if !has_venv_configuration(&candidate) {
                return Ok(candidate);
            }
        }

        // A copied executable (including a Windows venv launcher) has a different file identity.
        // Check the candidate's installation and ABI against the venv's public metadata instead.
        // The same standard library can be reachable through different installation prefixes.
        match Interpreter::query(&candidate, cache) {
            Ok(base)
                if !base.is_virtualenv()
                    && base.key() == key
                    && base.pointer_size() == interpreter.pointer_size()
                    && base.markers().implementation_version()
                        == interpreter.markers().implementation_version()
                    && (is_same_file(base.sys_base_prefix(), interpreter.sys_base_prefix())
                        .unwrap_or(false)
                        || is_same_file(base.stdlib(), interpreter.stdlib()).unwrap_or(false)) =>
            {
                return Ok(candidate);
            }
            Ok(_) => debug!(
                "Skipping incompatible base executable: {}",
                candidate.display()
            ),
            Err(err) => debug!(
                "Failed to inspect base executable {}: {err}",
                candidate.display()
            ),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "Could not find a compatible base Python executable for `{}`",
            interpreter.sys_executable().user_display()
        ),
    ))
}

/// Return a known executable target outside the virtual environment, if it has one.
fn executable_target(interpreter: &Interpreter) -> Option<PathBuf> {
    let target = if cfg!(windows) {
        uv_trampoline_builder::Launcher::try_from_path(interpreter.sys_executable())
            .ok()
            .flatten()
            .map(|launcher| launcher.python_path)
    } else {
        let executable = fs_err::canonicalize(interpreter.sys_executable()).ok()?;
        let root = fs_err::canonicalize(interpreter.sys_prefix()).ok()?;
        (!executable.starts_with(root)).then_some(executable)
    };
    target.filter(|target| !has_venv_configuration(target))
}

/// Python looks for `pyvenv.cfg` beside the executable and one directory above it.
fn has_venv_configuration(executable: &Path) -> bool {
    executable.parent().is_some_and(|directory| {
        is_virtualenv_base(directory) || directory.parent().is_some_and(is_virtualenv_base)
    })
}
