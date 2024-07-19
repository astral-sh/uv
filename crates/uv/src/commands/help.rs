use std::ffi::OsStr;
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

pub(crate) fn help(query: &[String], printer: Printer, no_pager: bool) -> Result<ExitStatus> {
    let mut uv = Cli::command();

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
        if let Ok(less) = which("less") {
            // When using less, we use the command name as the file name and can support colors
            let prompt = format!("help: uv {}", query.join(" "));
            spawn_pager(less, &["-R", "-P", &prompt], &help_ansi)?;
        } else if let Ok(more) = which("more") {
            // When using more, we skip the ANSI color codes
            spawn_pager(more, &[], &help)?;
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

/// Spawn a paging command to display contents.
fn spawn_pager(command: impl AsRef<OsStr>, args: &[&str], contents: impl Display) -> Result<()> {
    use std::io::Write;

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
