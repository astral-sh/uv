pub mod windows;

use std::path::{Path, PathBuf};
use uv_fs::Simplified;

/// Shells for which virtualenv activation scripts are available.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(clippy::doc_markdown)]
pub enum Shell {
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
    /// Korn SHell (ksh)
    Ksh,
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
    pub fn from_env() -> Option<Shell> {
        if std::env::var_os("NU_VERSION").is_some() {
            Some(Shell::Nushell)
        } else if std::env::var_os("FISH_VERSION").is_some() {
            Some(Shell::Fish)
        } else if std::env::var_os("BASH_VERSION").is_some() {
            Some(Shell::Bash)
        } else if std::env::var_os("ZSH_VERSION").is_some() {
            Some(Shell::Zsh)
        } else if std::env::var_os("KSH_VERSION").is_some() {
            Some(Shell::Ksh)
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
    pub fn from_shell_path(path: impl AsRef<Path>) -> Option<Shell> {
        parse_shell_from_path(path.as_ref())
    }

    /// Return the configuration files that should be modified to append to a shell's `PATH`.
    ///
    /// Some of the logic here is based on rustup's rc file detection.
    ///
    /// See: <https://github.com/rust-lang/rustup/blob/fede22fea7b160868cece632bd213e6d72f8912f/src/cli/self_update/shell.rs#L197>
    pub fn configuration_files(self) -> Vec<PathBuf> {
        let Some(home_dir) = home::home_dir() else {
            return vec![];
        };
        match self {
            Shell::Bash => {
                // On Bash, we need to update both `.bashrc` and `.bash_profile`. The former is
                // sourced for non-login shells, and the latter is sourced for login shells.
                //
                // In lieu of `.bash_profile`, shells will also respect `.bash_login` and
                // `.profile`, if they exist. So we respect those too.
                vec![
                    [".bash_profile", ".bash_login", ".profile"]
                        .iter()
                        .map(|rc| home_dir.join(rc))
                        .find(|rc| rc.is_file())
                        .unwrap_or_else(|| home_dir.join(".bash_profile")),
                    home_dir.join(".bashrc"),
                ]
            }
            Shell::Ksh => {
                // On Ksh it's standard POSIX `.profile` for login shells, and `.kshrc` for non-login.
                vec![home_dir.join(".profile"), home_dir.join(".kshrc")]
            }
            Shell::Zsh => {
                // On Zsh, we only need to update `.zshenv`. This file is sourced for both login and
                // non-login shells. However, we match rustup's logic for determining _which_
                // `.zshenv` to use.
                //
                // See: https://github.com/rust-lang/rustup/blob/fede22fea7b160868cece632bd213e6d72f8912f/src/cli/self_update/shell.rs#L197
                let zsh_dot_dir = std::env::var("ZDOTDIR")
                    .ok()
                    .filter(|dir| !dir.is_empty())
                    .map(PathBuf::from);

                // Attempt to update an existing `.zshenv` file.
                if let Some(zsh_dot_dir) = zsh_dot_dir.as_ref() {
                    // If `ZDOTDIR` is set, and `ZDOTDIR/.zshenv` exists, then we update that file.
                    let zshenv = zsh_dot_dir.join(".zshenv");
                    if zshenv.is_file() {
                        return vec![zshenv];
                    }
                } else {
                    // If `ZDOTDIR` is _not_ set, and `~/.zshenv` exists, then we update that file.
                    let zshenv = home_dir.join(".zshenv");
                    if zshenv.is_file() {
                        return vec![zshenv];
                    }
                }

                if let Some(zsh_dot_dir) = zsh_dot_dir.as_ref() {
                    // If `ZDOTDIR` is set, then we create `ZDOTDIR/.zshenv`.
                    vec![zsh_dot_dir.join(".zshenv")]
                } else {
                    // If `ZDOTDIR` is _not_ set, then we create `~/.zshenv`.
                    vec![home_dir.join(".zshenv")]
                }
            }
            Shell::Fish => {
                // On Fish, we only need to update `config.fish`. This file is sourced for both
                // login and non-login shells. However, we must respect Fish's logic, which reads
                // from `$XDG_CONFIG_HOME/fish/config.fish` if set, and `~/.config/fish/config.fish`
                // otherwise.
                if let Some(xdg_home_dir) = std::env::var("XDG_CONFIG_HOME")
                    .ok()
                    .filter(|dir| !dir.is_empty())
                    .map(PathBuf::from)
                {
                    vec![xdg_home_dir.join("fish/config.fish")]
                } else {
                    vec![home_dir.join(".config/fish/config.fish")]
                }
            }
            Shell::Csh => {
                // On Csh, we need to update both `.cshrc` and `.login`, like Bash.
                vec![home_dir.join(".cshrc"), home_dir.join(".login")]
            }
            // TODO(charlie): Add support for Nushell.
            Shell::Nushell => vec![],
            // See: [`crate::windows::prepend_path`].
            Shell::Powershell => vec![],
            // See: [`crate::windows::prepend_path`].
            Shell::Cmd => vec![],
        }
    }

    /// Returns `true` if the given path is on the `PATH` in this shell.
    pub fn contains_path(path: &Path) -> bool {
        let home_dir = home::home_dir();
        std::env::var_os("PATH")
            .as_ref()
            .iter()
            .flat_map(std::env::split_paths)
            .map(|path| {
                // If the first component is `~`, expand to the home directory.
                if let Some(home_dir) = home_dir.as_ref() {
                    if path
                        .components()
                        .next()
                        .map(std::path::Component::as_os_str)
                        == Some("~".as_ref())
                    {
                        return home_dir.join(path.components().skip(1).collect::<PathBuf>());
                    }
                }
                path
            })
            .any(|p| same_file::is_same_file(path, p).unwrap_or(false))
    }

    /// Returns the command necessary to prepend a directory to the `PATH` in this shell.
    pub fn prepend_path(self, path: &Path) -> Option<String> {
        match self {
            Shell::Nushell => None,
            Shell::Bash | Shell::Zsh | Shell::Ksh => Some(format!(
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
            Shell::Ksh => write!(f, "Ksh"),
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
        "ksh" => Some(Shell::Ksh),
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
