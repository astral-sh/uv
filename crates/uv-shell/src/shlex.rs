use crate::{Shell, Simplified};
use std::path::Path;

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
pub fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().portable_display().to_string();

    // Like Python's `shlex.quote`, quote the path if it contains any character that is not
    // safe to use unquoted in a POSIX shell. The safe set mirrors CPython exactly:
    // `[a-zA-Z0-9@%+=:,./_-]` — anything outside this set requires quoting.
    //
    // > Use single quotes, and put single quotes into double quotes
    // > The string $'b is then quoted as '$'"'"'b'
    if executable
        .chars()
        .any(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '_' | '-'))
    {
        format!("'{}'", escape_posix_for_single_quotes(&executable))
    } else {
        executable
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_shlex_posix_no_quoting_needed() {
        // Purely safe characters: no quoting expected.
        assert_eq!(shlex_posix(Path::new("/usr/bin/python3")), "/usr/bin/python3");
        assert_eq!(shlex_posix(Path::new("./venv/bin/activate")), "./venv/bin/activate");
        assert_eq!(shlex_posix(Path::new("/home/user/.venv/bin/activate")), "/home/user/.venv/bin/activate");
    }

    #[test]
    fn test_shlex_posix_space_triggers_quoting() {
        assert_eq!(
            shlex_posix(Path::new("/home/user/my project/.venv/bin/activate")),
            "'/home/user/my project/.venv/bin/activate'"
        );
    }

    #[test]
    fn test_shlex_posix_dollar_triggers_quoting() {
        // A `$` in the path must be quoted to prevent shell variable expansion.
        assert_eq!(
            shlex_posix(Path::new("/home/user/my$project/.venv/bin/activate")),
            "'/home/user/my$project/.venv/bin/activate'"
        );
    }

    #[test]
    fn test_shlex_posix_parentheses_trigger_quoting() {
        // Parentheses are common on macOS (e.g., "Applications (Rosetta)").
        assert_eq!(
            shlex_posix(Path::new("/Users/user/Applications (Rosetta)/.venv/bin/activate")),
            "'/Users/user/Applications (Rosetta)/.venv/bin/activate'"
        );
    }

    #[test]
    fn test_shlex_posix_embedded_single_quote() {
        // A path with an embedded single quote must use the '"'"' idiom.
        assert_eq!(
            shlex_posix(Path::new("/home/user/alice's-env/bin/activate")),
            "'/home/user/alice'\"'\"'s-env/bin/activate'"
        );
    }

    #[test]
    fn test_escape_posix_for_single_quotes() {
        assert_eq!(escape_posix_for_single_quotes("it's"), "it'\"'\"'s");
        assert_eq!(escape_posix_for_single_quotes("no quotes here"), "no quotes here");
    }
}
