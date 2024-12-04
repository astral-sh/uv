use crate::{Shell, Simplified};
use std::path::Path;

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
pub fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().portable_display().to_string();

    // Like Python's `shlex.quote`:
    // > Use single quotes, and put single quotes into double quotes
    // > The string $'b is then quoted as '$'"'"'b'
    if executable.contains(' ') {
        format!("'{}'", escape_posix_for_single_quotes(&executable))
    } else {
        executable
    }
}

/// Escape a string for being used in single quotes in a POSIX-compatible shell command.
///
/// We want our scripts to support any POSIX shell. There's two kind of quotes in POSIX:
/// Single and double quotes. In bash, single quotes must not contain another single
/// quote, you can't even escape it (<https://linux.die.net/man/1/bash> under "QUOTING").
/// Double quotes have escaping rules different from shell to shell, which we can't do.
/// Bash has `$'\''`, but that's not universal enough.
///
/// As solution, use implicit string concatenations, by putting the single quote into double
/// quotes.
pub fn escape_posix_for_single_quotes(string: &str) -> String {
    string.replace('\'', r#"'"'"'"#)
}

/// Quote a path, if necessary, for safe use in `PowerShell` and `cmd`.
pub fn shlex_windows(executable: impl AsRef<Path>, shell: Shell) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().user_display().to_string();

    // Wrap the executable in quotes (and a `&` invocation on PowerShell), if it contains spaces.
    if executable.contains(' ') {
        if shell == Shell::Powershell {
            // For PowerShell, wrap in a `&` invocation.
            format!("& \"{executable}\"")
        } else {
            // Otherwise, assume `cmd`, which doesn't need the `&`.
            format!("\"{executable}\"")
        }
    } else {
        executable
    }
}
