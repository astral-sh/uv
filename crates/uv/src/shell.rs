use std::path::{Path, PathBuf};
use uv_fs::Simplified;

/// Shells for which virtualenv activation scripts are available.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::doc_markdown)]
pub(crate) enum Shell {
    /// Bourne Again SHell (bash)
    Bash,
    /// Friendly Interactive SHell (fish)
    Fish,
    /// PowerShell
    Powershell,
    /// Cmd (Command Prompt)
    Cmd,
    /// Z SHell (zsh)
    Zsh,
    /// Nushell
    Nushell,
    /// C SHell (csh)
    Csh,
}

impl Shell {
    /// Determine the user's current shell from the environment.
    ///
    /// This will read the `SHELL` environment variable and try to determine which shell is in use
    /// from that.
    ///
    /// If `SHELL` is not set, then on windows, it will default to powershell, and on
    /// other `OSes` it will return `None`.
    ///
    /// If `SHELL` is set, but contains a value that doesn't correspond to one of the supported
    /// shell types, then return `None`.
    pub(crate) fn from_env() -> Option<Shell> {
        if std::env::var_os("NU_VERSION").is_some() {
            Some(Shell::Nushell)
        } else if std::env::var_os("FISH_VERSION").is_some() {
            Some(Shell::Fish)
        } else if std::env::var_os("BASH_VERSION").is_some() {
            Some(Shell::Bash)
        } else if std::env::var_os("ZSH_VERSION").is_some() {
            Some(Shell::Zsh)
        } else if let Some(env_shell) = std::env::var_os("SHELL") {
            Shell::from_shell_path(env_shell)
        } else if cfg!(windows) {
            // Command Prompt relies on PROMPT for its appearance whereas PowerShell does not.
            // See: https://stackoverflow.com/a/66415037.
            if std::env::var_os("PROMPT").is_some() {
                Some(Shell::Cmd)
            } else {
                // Fallback to PowerShell if the PROMPT environment variable is not set.
                Some(Shell::Powershell)
            }
        } else {
            None
        }
    }

    /// Parse a shell from a path to the executable for the shell.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::shells::Shell;
    ///
    /// assert_eq!(Shell::from_shell_path("/bin/bash"), Some(Shell::Bash));
    /// assert_eq!(Shell::from_shell_path("/usr/bin/zsh"), Some(Shell::Zsh));
    /// assert_eq!(Shell::from_shell_path("/opt/my_custom_shell"), None);
    /// ```
    pub(crate) fn from_shell_path(path: impl AsRef<Path>) -> Option<Shell> {
        parse_shell_from_path(path.as_ref())
    }

    /// Return the configuration files that should be modified to append to a shell's `PATH`.
    pub(crate) fn configuration_files(self) -> Vec<PathBuf> {
        let Some(home_dir) = home::home_dir() else {
            return vec![];
        };
        match self {
            Shell::Bash => {
                // On Bash, we need to update both `.bashrc` and `.bash_profile`. The former is
                // sourced for non-login shells, and the latter is sourced for login shells. If
                // `.profile` is present, we prefer it over `.bash_profile`, to match the behavior
                // of the system shell.
                vec![
                    home_dir.join(".bashrc"),
                    if home_dir.join(".profile").is_file() {
                        home_dir.join(".profile")
                    } else {
                        home_dir.join(".bash_profile")
                    },
                ]
            }
            Shell::Zsh => {
                // On Zsh, we only need to update `.zshrc`. This file is sourced for both login and
                // non-login shells.
                vec![home_dir.join(".zshrc")]
            }
            Shell::Fish => {
                // On Fish, we only need to update `config.fish`. This file is sourced for both
                // login and non-login shells.
                vec![home_dir.join(".config/fish/config.fish")]
            }
            Shell::Csh => {
                // On Csh, we need to update both `.cshrc` and `.login`, like Bash.
                vec![home_dir.join(".cshrc"), home_dir.join(".login")]
            }
            // TODO(charlie): Add support for Nushell, PowerShell, and Cmd.
            Shell::Nushell => vec![],
            Shell::Powershell => vec![],
            Shell::Cmd => vec![],
        }
    }

    /// Returns `true` if the given path is on the `PATH` in this shell.
    pub(crate) fn contains_path(path: &Path) -> bool {
        std::env::var_os("PATH")
            .as_ref()
            .iter()
            .flat_map(std::env::split_paths)
            .any(|p| same_file::is_same_file(path, p).unwrap_or(false))
    }

    /// Returns the command necessary to prepend a directory to the `PATH` in this shell.
    pub(crate) fn prepend_path(self, path: &Path) -> Option<String> {
        match self {
            Shell::Nushell => None,
            Shell::Bash | Shell::Zsh => Some(format!(
                "export PATH=\"{}:$PATH\"",
                backslash_escape(&path.simplified_display().to_string()),
            )),
            Shell::Fish => Some(format!(
                "fish_add_path \"{}\"",
                backslash_escape(&path.simplified_display().to_string()),
            )),
            Shell::Csh => Some(format!(
                "setenv PATH \"{}:$PATH\"",
                backslash_escape(&path.simplified_display().to_string()),
            )),
            Shell::Powershell => Some(format!(
                "$env:PATH = \"{};$env:PATH\"",
                backtick_escape(&path.simplified_display().to_string()),
            )),
            Shell::Cmd => Some(format!(
                "set PATH=\"{};%PATH%\"",
                backslash_escape(&path.simplified_display().to_string()),
            )),
        }
    }
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shell::Bash => write!(f, "Bash"),
            Shell::Fish => write!(f, "Fish"),
            Shell::Powershell => write!(f, "PowerShell"),
            Shell::Cmd => write!(f, "Command Prompt"),
            Shell::Zsh => write!(f, "Zsh"),
            Shell::Nushell => write!(f, "Nushell"),
            Shell::Csh => write!(f, "Csh"),
        }
    }
}

/// Parse the shell from the name of the shell executable.
fn parse_shell_from_path(path: &Path) -> Option<Shell> {
    let name = path.file_stem()?.to_str()?;
    match name {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "csh" => Some(Shell::Csh),
        "powershell" | "powershell_ise" => Some(Shell::Powershell),
        _ => None,
    }
}

/// Escape a string for use in a shell command by inserting backslashes.
fn backslash_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '"' => escaped.push('\\'),
            _ => {}
        }
        escaped.push(c);
    }
    escaped
}

/// Escape a string for use in a `PowerShell` command by inserting backticks.
fn backtick_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '"' | '$' => escaped.push('`'),
            _ => {}
        }
        escaped.push(c);
    }
    escaped
}
