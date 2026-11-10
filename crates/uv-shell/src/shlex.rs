use crate::{Shell, Simplified};
use std::borrow::Cow;
use std::path::Path;

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
pub fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().portable_display().to_string();

    shlex_posix_str(&executable).into_owned()
}

/// Quote a string, if necessary, for safe use as a single argument in a POSIX-compatible shell
/// command.
pub fn shlex_posix_str(value: &str) -> Cow<'_, str> {
    // Match Python's `shlex.quote` and leave only shell-safe ASCII characters unquoted.
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@%+=:,./-_".contains(&byte))
    {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("'{}'", escape_posix_for_single_quotes(value)))
    }
}

/// Escape a string for being used in single quotes in a POSIX-compatible shell command.
///
/// We want our scripts to support any POSIX shell. There are two kinds of quotes in POSIX:
/// Single and double quotes. In bash, single quotes must not contain another single
/// quote, you can't even escape it (<https://linux.die.net/man/1/bash> under "QUOTING").
/// Double quotes have escaping rules that differ from shell to shell, which we can't handle.
/// Bash has `$'\''`, but that's not universal enough.
///
/// As a solution, use implicit string concatenations, by putting the single quote into double
/// quotes.
pub fn escape_posix_for_single_quotes(string: &str) -> Cow<'_, str> {
    if string.contains('\'') {
        Cow::Owned(string.replace('\'', r#"'"'"'"#))
    } else {
        Cow::Borrowed(string)
    }
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

#[cfg(test)]
mod tests {
    use super::{shlex_posix, shlex_posix_str};

    #[test]
    fn posix_safe_str() {
        assert_eq!(shlex_posix_str("3.13"), "3.13");
        assert_eq!(shlex_posix_str("==3.13"), "==3.13");
        assert_eq!(shlex_posix_str("cpython@3.13"), "cpython@3.13");
    }

    #[test]
    fn posix_str_with_metacharacters() {
        assert_eq!(shlex_posix_str(">=3.13, <3.14"), "'>=3.13, <3.14'");
        assert_eq!(shlex_posix_str("3.*"), "'3.*'");
        assert_eq!(shlex_posix_str(""), "''");
    }

    #[test]
    fn posix_safe_path() {
        assert_eq!(shlex_posix("/usr/bin/python3.12"), "/usr/bin/python3.12");
    }

    #[test]
    fn posix_empty_path() {
        assert_eq!(shlex_posix(""), "''");
    }

    #[test]
    fn posix_path_with_metacharacters() {
        assert_eq!(
            shlex_posix("Testing's/$venv;activate"),
            r#"'Testing'"'"'s/$venv;activate'"#
        );
    }
}
