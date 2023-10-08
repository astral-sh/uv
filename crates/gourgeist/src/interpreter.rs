use std::io;
use std::io::{BufReader, Write};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use fs_err::File;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::{crate_cache_dir, Error};

const QUERY_PYTHON: &str = include_str!("query_python.py");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InterpreterInfo {
    pub base_exec_prefix: String,
    pub base_prefix: String,
    pub major: u8,
    pub minor: u8,
    pub python_version: String,
}

/// Gets the interpreter.rs info, either cached or by running it.
pub fn get_interpreter_info(interpreter: &Utf8Path) -> Result<InterpreterInfo, Error> {
    let cache_dir = crate_cache_dir()?.join("interpreter_info");

    let index = seahash::hash(interpreter.as_str().as_bytes());
    let cache_file = cache_dir.join(index.to_string()).with_extension("json");

    let modified = fs::metadata(interpreter)?
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    if cache_file.exists() {
        let cache_entry: Result<CacheEntry, String> = File::open(&cache_file)
            .map_err(|err| err.to_string())
            .and_then(|cache_reader| {
                serde_json::from_reader(BufReader::new(cache_reader)).map_err(|err| err.to_string())
            });
        match cache_entry {
            Ok(cache_entry) => {
                debug!("Using cache entry {cache_file}");
                if modified == cache_entry.modified && interpreter == cache_entry.interpreter {
                    return Ok(cache_entry.interpreter_info);
                }
                debug!(
                    "Removing mismatching cache entry {cache_file} ({} {} {} {})",
                    modified, cache_entry.modified, interpreter, cache_entry.interpreter
                );
                if let Err(remove_err) = fs::remove_file(&cache_file) {
                    warn!("Failed to mismatching cache file at {cache_file}: {remove_err}");
                }
            }
            Err(cache_err) => {
                debug!("Removing broken cache entry {cache_file} ({cache_err})");
                if let Err(remove_err) = fs::remove_file(&cache_file) {
                    warn!("Failed to remove broken cache file at {cache_file}: {remove_err} (original error: {cache_err})");
                }
            }
        }
    }

    let interpreter_info = query_interpreter(interpreter)?;
    fs::create_dir_all(&cache_dir)?;
    let cache_entry = CacheEntry {
        interpreter: interpreter.to_path_buf(),
        modified,
        interpreter_info: interpreter_info.clone(),
    };
    let mut cache_writer = File::create(&cache_file)?;
    serde_json::to_writer(&mut cache_writer, &cache_entry).map_err(io::Error::from)?;

    Ok(interpreter_info)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheEntry {
    interpreter: Utf8PathBuf,
    modified: u128,
    interpreter_info: InterpreterInfo,
}

/// Runs a python script that returns the relevant info about the interpreter.rs as json
fn query_interpreter(interpreter: &Utf8Path) -> Result<InterpreterInfo, Error> {
    let mut child = Command::new(interpreter)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(QUERY_PYTHON.as_bytes())
            .map_err(|err| Error::PythonSubcommand {
                interpreter: interpreter.to_path_buf(),
                err,
            })?;
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8(output.stdout).unwrap_or_else(|err| {
        // At this point, there was most likely an error caused by a non-utf8 character, so we're in
        // an ugly case but still very much want to give the user a chance
        error!(
            "The stdout of the failed call of the call to {} contains non-utf8 characters",
            interpreter
        );
        String::from_utf8_lossy(err.as_bytes()).to_string()
    });
    let stderr = String::from_utf8(output.stderr).unwrap_or_else(|err| {
        error!(
            "The stderr of the failed call of the call to {} contains non-utf8 characters",
            interpreter
        );
        String::from_utf8_lossy(err.as_bytes()).to_string()
    });
    // stderr isn't technically a criterion for success, but i don't know of any cases where there
    // should be stderr output and if there is, we want to know
    if !output.status.success() || !stderr.trim().is_empty() {
        return Err(Error::PythonSubcommand {
            interpreter: interpreter.to_path_buf(),
            err: io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Querying python at {} failed with status {}:\n--- stdout:\n{}\n--- stderr:\n{}",
                    interpreter,
                    output.status,
                    stdout.trim(),
                    stderr.trim()
                ),
            )
        });
    }
    let data = serde_json::from_str::<InterpreterInfo>(&stdout).map_err(|err|
        Error::PythonSubcommand {
            interpreter: interpreter.to_path_buf(),
            err: io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Querying python at {} did not return the expected data ({}):\n--- stdout:\n{}\n--- stderr:\n{}",
                    interpreter,
                    err,
                    stdout.trim(),
                    stderr.trim()
                )
            )
        }
    )?;
    Ok(data)
}

/// Parse the value of the `-p`/`--python` option, which can be e.g. `3.11`, `python3.11`,
/// `tools/bin/python3.11` or `/usr/bin/python3.11`.
pub fn parse_python_cli(cli_python: Option<Utf8PathBuf>) -> Result<Utf8PathBuf, crate::Error> {
    let python = if let Some(python) = cli_python {
        if let Some((major, minor)) = python
            .as_str()
            .split_once('.')
            .and_then(|(major, minor)| Some((major.parse::<u8>().ok()?, minor.parse::<u8>().ok()?)))
        {
            if major != 3 {
                return Err(crate::Error::InvalidPythonInterpreter(
                    "Only python 3 is supported".into(),
                ));
            }
            info!("Looking for python {major}.{minor}");
            Utf8PathBuf::from(format!("python{major}.{minor}"))
        } else {
            python
        }
    } else {
        Utf8PathBuf::from("python3".to_string())
    };

    // Call `which` to find it in path, if not given a path
    let python = if python.components().count() > 1 {
        // Does this path contain a slash (unix) or backslash (windows)? In that case, assume it's
        // relative or absolute path that we don't need to resolve
        info!("Assuming {python} is a path");
        python
    } else {
        let python_in_path = which::which(python.as_std_path())
            .map_err(|err| {
                crate::Error::InvalidPythonInterpreter(
                    format!("Can't find {python} ({err})").into(),
                )
            })?
            .try_into()
            .map_err(camino::FromPathBufError::into_io_error)?;
        info!("Resolved {python} to {python_in_path}");
        python_in_path
    };
    Ok(python)
}
