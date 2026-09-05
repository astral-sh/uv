use std::env::current_dir;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::str::from_utf8;

use owo_colors::OwoColorize;

use uv_errors::{Hint, Hints};

/// The maximum number of bytes read from a resolved executable when looking for a shebang line.
///
/// uv only ever emits a *direct* shebang (`#!<path>`, no `/bin/sh` wrapper) when the interpreter
/// path is under 127 bytes (see `uv-install-wheel::wheel::format_shebang`), so this is generous
/// headroom while still bounding memory use against arbitrary (e.g. large binary) files.
const SHEBANG_PROBE_LEN: usize = 4096;

/// Raised when spawning a resolved command fails because the executable — or the interpreter
/// named in its shebang — cannot be found.
#[derive(Debug, thiserror::Error)]
#[error("Failed to spawn: `{executable}`")]
pub(crate) struct SpawnNotFoundError {
    executable: String,
    #[source]
    source: io::Error,
    /// The interpreter path named in the executable's shebang, if it no longer exists.
    missing_interpreter: Option<PathBuf>,
    /// A caller-specific suggestion for how to repair the environment, if any.
    recreate_hint: Option<&'static str>,
}

impl Hint for SpawnNotFoundError {
    fn hints(&self) -> Hints<'_> {
        let Some(interpreter) = &self.missing_interpreter else {
            return Hints::none();
        };
        let suffix = self
            .recreate_hint
            .unwrap_or("The environment may have been moved, deleted, or gone stale.");
        Hints::from(format!(
            "`{}` uses a Python interpreter at `{}`, which no longer exists. {suffix}",
            self.executable.cyan(),
            interpreter.display().cyan(),
        ))
    }
}

/// Wrap a `spawn()` failure as an error, attaching a hint if `program` resolves — within one of
/// `managed_dirs` — to a script whose shebang names a Python interpreter that no longer exists.
///
/// `display_executable` is the user-facing name for the error message (unchanged from before this
/// diagnostic existed). `program` is the exact argument passed to
/// [`std::process::Command::new`], and `path` is the `PATH` the child process was given — both are
/// needed to replicate the OS's own executable resolution when looking for a shebang to inspect.
/// `managed_dirs` restricts the diagnostic to scripts uv itself installed (e.g. a venv's
/// `bin`/`Scripts` directory): a same-named executable found elsewhere on `PATH` is not uv's to
/// diagnose, and guessing at unrelated scripts' interpreters would just be a second wrong message.
/// `recreate_hint`, if given, is appended verbatim to suggest how to repair the environment.
pub(crate) fn spawn_error(
    display_executable: &str,
    program: &OsStr,
    path: Option<&OsStr>,
    managed_dirs: &[PathBuf],
    recreate_hint: Option<&'static str>,
    source: io::Error,
) -> anyhow::Error {
    let missing_interpreter = (source.kind() == io::ErrorKind::NotFound)
        .then(|| resolve_missing_interpreter(program, path, managed_dirs))
        .flatten();

    SpawnNotFoundError {
        executable: display_executable.to_string(),
        source,
        missing_interpreter,
        recreate_hint,
    }
    .into()
}

/// Resolve `program` against `path` and, if it lives under `managed_dirs` and is a script with a
/// dangling direct shebang, return the shebang's interpreter path.
fn resolve_missing_interpreter(
    program: &OsStr,
    path: Option<&OsStr>,
    managed_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let cwd = current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resolved = which::which_in(program, path, cwd).ok()?;
    if !managed_dirs.iter().any(|dir| resolved.starts_with(dir)) {
        return None;
    }
    dangling_shebang_interpreter(&resolved)
}

/// If `script` begins with a direct (non-`/bin/sh`-wrapped) shebang line whose interpreter no
/// longer exists on disk, return that interpreter's path.
///
/// uv only emits a direct shebang (`#!<path>`) when the interpreter path is short and
/// space-free; longer or relocatable paths get a `/bin/sh` wrapper instead (see
/// `uv-install-wheel::wheel::format_shebang`). The wrapper form spawns successfully and reports
/// its own, already-clear shell error, so it's out of scope here — and since `/bin/sh` itself
/// always exists, the existence check below already excludes it without special-casing.
fn dangling_shebang_interpreter(script: &Path) -> Option<PathBuf> {
    let mut file = fs_err::File::open(script).ok()?;
    let mut buf = vec![0u8; SHEBANG_PROBE_LEN];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);

    let first_line = buf.split(|&b| b == b'\n').next().unwrap_or_default();
    let first_line = from_utf8(first_line).ok()?;
    let shebang = first_line.strip_prefix("#!")?.trim();
    // A shebang may carry a single interpreter argument (e.g. `#!/venv/bin/python3 -s`, as
    // installed by some build backends); only the leading token is a path.
    let interpreter = shebang.split_whitespace().next()?;

    let interpreter_path = PathBuf::from(interpreter);
    if interpreter_path.is_absolute() && !interpreter_path.exists() {
        Some(interpreter_path)
    } else {
        None
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::path::PathBuf;

    use super::dangling_shebang_interpreter;

    #[test]
    fn dangling_shebang_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        fs_err::write(&script, "#!/definitely/does/not/exist/python3\n").unwrap();

        assert_eq!(
            dangling_shebang_interpreter(&script),
            Some(PathBuf::from("/definitely/does/not/exist/python3"))
        );
    }

    #[test]
    fn dangling_shebang_with_argument_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        fs_err::write(&script, "#!/definitely/does/not/exist/python3 -s\n").unwrap();

        assert_eq!(
            dangling_shebang_interpreter(&script),
            Some(PathBuf::from("/definitely/does/not/exist/python3"))
        );
    }

    #[test]
    fn existing_interpreter_is_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        // Any file that exists is a valid stand-in interpreter for this check.
        fs_err::write(&script, format!("#!{}\n", script.display())).unwrap();

        assert_eq!(dangling_shebang_interpreter(&script), None);
    }

    #[test]
    fn sh_wrapper_form_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        fs_err::write(
            &script,
            "#!/bin/sh\n'''exec' '/definitely/missing/python3' \"$0\" \"$@\"\n' '''\n",
        )
        .unwrap();

        assert_eq!(dangling_shebang_interpreter(&script), None);
    }

    #[test]
    fn relative_interpreter_is_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        fs_err::write(&script, "#!python3\n").unwrap();

        assert_eq!(dangling_shebang_interpreter(&script), None);
    }

    #[test]
    fn non_script_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("entry");
        fs_err::write(&script, [0xffu8, 0xfe, 0x00, 0x01]).unwrap();

        assert_eq!(dangling_shebang_interpreter(&script), None);
    }
}
