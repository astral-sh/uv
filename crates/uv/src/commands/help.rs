use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fmt::Display, fmt::Write};

use anstream::{stream::IsTerminal, ColorChoice};
use anyhow::{anyhow, Result};
use clap::CommandFactory;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use which::which;

use super::ExitStatus;
use crate::printer::Printer;
use uv_cli::Cli;
use uv_static::EnvVars;

// hidden subcommands to show in the help command
const SHOW_HIDDEN_COMMANDS: &[&str] = &["generate-shell-completion"];

pub(crate) fn help(query: &[String], printer: Printer, no_pager: bool) -> Result<ExitStatus> {
    let mut uv: clap::Command = SHOW_HIDDEN_COMMANDS
        .iter()
        .fold(Cli::command(), |uv, &name| {
            uv.mut_subcommand(name, |cmd| cmd.hide(false))
        });

    // It is very important to build the command before beginning inspection or subcommands
    // will be missing all of the propagated options.
    uv.build();

    let command = find_command(query, &uv).map_err(|(unmatched, nearest)| {
        let missing = if unmatched.len() == query.len() {
            format!("`{}` for `uv`", query.join(" "))
        } else {
            format!("`{}` for `uv {}`", unmatched.join(" "), nearest.get_name())
        };
        anyhow!(
            "There is no command {}. Did you mean one of:\n    {}",
            missing,
            nearest
                .get_subcommands()
                .filter(|cmd| !cmd.is_hide_set())
                .map(clap::Command::get_name)
                .filter(|name| *name != "help")
                .join("\n    "),
        )
    })?;

    let name = command.get_name();
    let is_root = name == uv.get_name();
    let mut command = command.clone();

    let help = if is_root {
        command
            .after_help(format!(
                "Use `{}` for more information on a specific command.",
                "uv help <command>".bold()
            ))
            .render_help()
    } else {
        if command.has_subcommands() {
            command.after_long_help(format!(
                "Use `{}` for more information on a specific command.",
                format!("uv help {name} <command>").bold()
            ))
        } else {
            command
        }
        .render_long_help()
    };

    let help_ansi = match anstream::Stdout::choice(&std::io::stdout()) {
        ColorChoice::Always | ColorChoice::AlwaysAnsi => Either::Left(help.ansi()),
        ColorChoice::Never => Either::Right(help.clone()),
        // We just asked anstream for a choice, that can't be auto
        ColorChoice::Auto => unreachable!(),
    };

    let is_terminal = std::io::stdout().is_terminal();
    let should_page = !no_pager && !is_root && is_terminal;

    if should_page {
        if let Some(pager) = Pager::try_from_env() {
            let content = if pager.supports_colors() {
                help_ansi
            } else {
                Either::Right(help.clone())
            };
            pager.spawn(
                format!("{}: {}", "uv help".bold(), query.join(" ")),
                &content,
            )?;
        } else {
            writeln!(printer.stdout(), "{help_ansi}")?;
        }
    } else {
        writeln!(printer.stdout(), "{help_ansi}")?;
    }

    Ok(ExitStatus::Success)
}

/// Find the command corresponding to a set of arguments, e.g., `["uv", "pip", "install"]`.
///
/// If the command cannot be found, the nearest command is returned.
fn find_command<'a>(
    query: &'a [String],
    cmd: &'a clap::Command,
) -> Result<&'a clap::Command, (&'a [String], &'a clap::Command)> {
    let Some(next) = query.first() else {
        return Ok(cmd);
    };

    let subcommand = cmd.find_subcommand(next).ok_or((query, cmd))?;
    find_command(&query[1..], subcommand)
}

#[derive(Debug)]
enum PagerKind {
    Less,
    More,
    Other(String),
}

#[derive(Debug)]
struct Pager {
    kind: PagerKind,
    args: Vec<String>,
    path: Option<PathBuf>,
}

impl PagerKind {
    fn default_args(&self, prompt: String) -> Vec<String> {
        match self {
            Self::Less => vec!["-R".to_string(), "-P".to_string(), prompt],
            Self::More => vec![],
            Self::Other(_) => vec![],
        }
    }
}

impl Display for PagerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Less => write!(f, "less"),
            Self::More => write!(f, "more"),
            Self::Other(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for Pager {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split_ascii_whitespace();

        // Empty string
        let Some(first) = split.next() else {
            return Err(());
        };

        match first {
            "less" => Ok(Self {
                kind: PagerKind::Less,
                args: split.map(str::to_string).collect(),
                path: None,
            }),
            "more" => Ok(Self {
                kind: PagerKind::More,
                args: split.map(str::to_string).collect(),
                path: None,
            }),
            _ => Ok(Self {
                kind: PagerKind::Other(first.to_string()),
                args: split.map(str::to_string).collect(),
                path: None,
            }),
        }
    }
}

impl Pager {
    /// Display `contents` using the pager.
    fn spawn(self, prompt: String, contents: impl Display) -> Result<()> {
        use std::io::Write;

        let command = self
            .path
            .as_ref()
            .map(|path| path.as_os_str().to_os_string())
            .unwrap_or(OsString::from(self.kind.to_string()));

        let args = if self.args.is_empty() {
            self.kind.default_args(prompt)
        } else {
            self.args
        };

        let mut child = std::process::Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to take child process stdin"))?;

        let contents = contents.to_string();
        let writer = std::thread::spawn(move || stdin.write_all(contents.as_bytes()));

        drop(child.wait());
        drop(writer.join());

        Ok(())
    }

    /// Get a pager to use and its path, if available.
    ///
    /// Supports the `PAGER` environment variable, otherwise checks for `less` and `more` in the
    /// search path.
    fn try_from_env() -> Option<Pager> {
        if let Some(pager) = std::env::var_os(EnvVars::PAGER) {
            if !pager.is_empty() {
                return Pager::from_str(&pager.to_string_lossy()).ok();
            }
        }

        if let Ok(less) = which("less") {
            Some(Pager {
                kind: PagerKind::Less,
                args: vec![],
                path: Some(less),
            })
        } else if let Ok(more) = which("more") {
            Some(Pager {
                kind: PagerKind::More,
                args: vec![],
                path: Some(more),
            })
        } else {
            None
        }
    }

    fn supports_colors(&self) -> bool {
        match self.kind {
            // The `-R` flag is required for color support. We will provide it by default.
            PagerKind::Less => self.args.is_empty() || self.args.iter().any(|arg| arg == "-R"),
            PagerKind::More => false,
            PagerKind::Other(_) => false,
        }
    }
}
