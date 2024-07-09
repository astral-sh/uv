use std::fmt::Write;

use anyhow::{anyhow, Result};
use clap::CommandFactory;
use itertools::Itertools;

use super::ExitStatus;
use crate::printer::Printer;
use uv_cli::Cli;

pub(crate) fn help(query: &[String], printer: Printer) -> Result<ExitStatus> {
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

    let mut command = command.clone();
    let help = command.render_long_help();
    writeln!(printer.stderr(), "{}", help.ansi())?;

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
