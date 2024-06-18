use std::path::Path;

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
    /// ```
    /// use crate::shells::Shell;
    ///
    /// assert_eq!(Shell::from_shell_path("/bin/bash"), Some(Shell::Bash));
    /// assert_eq!(Shell::from_shell_path("/usr/bin/zsh"), Some(Shell::Zsh));
    /// assert_eq!(Shell::from_shell_path("/opt/my_custom_shell"), None);
    /// ```
    pub(crate) fn from_shell_path(path: impl AsRef<Path>) -> Option<Shell> {
        parse_shell_from_path(path.as_ref())
    }
}

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
