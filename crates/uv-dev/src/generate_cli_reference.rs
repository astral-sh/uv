//! Generate a Markdown-compatible reference for the uv command-line interface.
use std::cmp::max;
use std::path::PathBuf;

use anstream::println;
use anyhow::{bail, Result};
use clap::{Command, CommandFactory};
use itertools::Itertools;
use pretty_assertions::StrComparison;

use crate::generate_all::Mode;
use crate::ROOT_DIR;

use uv_cli::Cli;

const REPLACEMENTS: &[(&str, &str)] = &[
    // Replace suggestions to use `uv help python` with a link to the
    // `uv python` section
    (
        "<code>uv help python</code>",
        "<a href=\"#uv-python\">uv python</a>",
    ),
    // Drop the manually included `env` section for `--no-python-downloads`
    // TODO(zanieb): In general, we should show all of the environment variables in the reference
    // but this one is non-standard so it's the only one included right now. When we tackle the rest
    // we can fix the formatting.
    (" [env: &quot;UV<em>PYTHON</em>DOWNLOADS=never&quot;]", ""),
];

const SHOW_HIDDEN_COMMANDS: &[&str] = &["generate-shell-completion"];

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Write the generated output to stdout (rather than to `settings.md`).
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let reference_string = generate();
    let filename = "cli.md";
    let reference_path = PathBuf::from(ROOT_DIR)
        .join("docs")
        .join("reference")
        .join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{reference_string}");
        }
        Mode::Check => match fs_err::read_to_string(reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &reference_string);
                    bail!("{filename} changed, please run `cargo dev generate-cli-reference`:\n{comparison}");
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("{filename} not found, please run `cargo dev generate-cli-reference`");
            }
            Err(err) => {
                bail!("{filename} changed, please run `cargo dev generate-cli-reference`:\n{err}");
            }
        },
        Mode::Write => match fs_err::read_to_string(&reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    println!("Updating: {filename}");
                    fs_err::write(reference_path, reference_string.as_bytes())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                println!("Updating: {filename}");
                fs_err::write(reference_path, reference_string.as_bytes())?;
            }
            Err(err) => {
                bail!("{filename} changed, please run `cargo dev generate-cli-reference`:\n{err}");
            }
        },
    }

    Ok(())
}

fn generate() -> String {
    let mut output = String::new();

    let mut uv = Cli::command();

    // It is very important to build the command before beginning inspection or subcommands
    // will be missing all of the propagated options.
    uv.build();

    let mut parents = Vec::new();

    output.push_str("# CLI Reference\n\n");
    generate_command(&mut output, &uv, &mut parents);

    for (value, replacement) in REPLACEMENTS {
        output = output.replace(value, replacement);
    }

    output
}

fn generate_command<'a>(output: &mut String, command: &'a Command, parents: &mut Vec<&'a Command>) {
    if command.is_hide_set() && !SHOW_HIDDEN_COMMANDS.contains(&command.get_name()) {
        return;
    }

    // Generate the command header.
    let name = if parents.is_empty() {
        command.get_name().to_string()
    } else {
        format!(
            "{} {}",
            parents.iter().map(|cmd| cmd.get_name()).join(" "),
            command.get_name()
        )
    };

    // Display the top-level `uv` command at the same level as its children
    let level = max(2, parents.len() + 1);
    output.push_str(&format!("{} {name}\n\n", "#".repeat(level)));

    // Display the command description.
    if let Some(about) = command.get_long_about().or_else(|| command.get_about()) {
        output.push_str(&about.to_string());
        output.push_str("\n\n");
    };

    // Display the usage
    {
        // This appears to be the simplest way to get rendered usage from Clap,
        // it is complicated to render it manually. It's annoying that it
        // requires a mutable reference but it doesn't really matter.
        let mut command = command.clone();
        output.push_str("<h3 class=\"cli-reference\">Usage</h3>\n\n");
        output.push_str(&format!(
            "```\n{}\n```",
            command
                .render_usage()
                .to_string()
                .trim_start_matches("Usage: "),
        ));
        output.push_str("\n\n");
    }

    // Display a list of child commands
    let mut subcommands = command.get_subcommands().peekable();
    let has_subcommands = subcommands.peek().is_some();
    if has_subcommands {
        output.push_str("<h3 class=\"cli-reference\">Commands</h3>\n\n");
        output.push_str("<dl class=\"cli-reference\">");

        for subcommand in subcommands {
            if subcommand.is_hide_set() {
                continue;
            }
            let subcommand_name = format!("{name} {}", subcommand.get_name());
            output.push_str(&format!(
                "<dt><a href=\"#{}\"><code>{subcommand_name}</code></a></dt>",
                subcommand_name.replace(' ', "-")
            ));
            if let Some(about) = subcommand.get_about() {
                output.push_str(&format!(
                    "<dd>{}</dd>\n",
                    markdown::to_html(&about.to_string())
                ));
            }
        }

        output.push_str("</dl>\n\n");
    }

    // Do not display options for commands with children
    if !has_subcommands {
        // Display positional arguments
        let mut arguments = command
            .get_positionals()
            .filter(|arg| !arg.is_hide_set())
            .peekable();

        if arguments.peek().is_some() {
            output.push_str("<h3 class=\"cli-reference\">Arguments</h3>\n\n");
            output.push_str("<dl class=\"cli-reference\">");

            for arg in arguments {
                output.push_str("<dt>");
                output.push_str(&format!(
                    "<code>{}</code>",
                    arg.get_id().to_string().to_uppercase()
                ));
                output.push_str("</dt>");
                if let Some(help) = arg.get_long_help().or_else(|| arg.get_help()) {
                    output.push_str("<dd>");
                    output.push_str(&format!("{}\n", markdown::to_html(&help.to_string())));
                    output.push_str("</dd>");
                }
            }

            output.push_str("</dl>\n\n");
        }

        // Display options and flags
        let mut options = command
            .get_arguments()
            .filter(|arg| !arg.is_positional())
            .filter(|arg| !arg.is_hide_set())
            .sorted_by_key(|arg| arg.get_id())
            .peekable();

        if options.peek().is_some() {
            output.push_str("<h3 class=\"cli-reference\">Options</h3>\n\n");
            output.push_str("<dl class=\"cli-reference\">");
            for opt in options {
                let Some(long) = opt.get_long() else { continue };

                output.push_str("<dt>");
                output.push_str(&format!("<code>--{long}</code>"));
                if let Some(short) = opt.get_short() {
                    output.push_str(&format!(", <code>-{short}</code>"));
                }

                // Re-implements private `Arg::is_takes_value_set` used in `Command::get_opts`
                if opt
                    .get_num_args()
                    .unwrap_or_else(|| 1.into())
                    .takes_values()
                {
                    if let Some(values) = opt.get_value_names() {
                        for value in values {
                            output.push_str(&format!(
                                " <i>{}</i>",
                                value.to_lowercase().replace('_', "-")
                            ));
                        }
                    }
                }
                output.push_str("</dt>");
                if let Some(help) = opt.get_long_help().or_else(|| opt.get_help()) {
                    output.push_str("<dd>");
                    output.push_str(&format!("{}\n", markdown::to_html(&help.to_string())));
                    emit_env_option(opt, output);
                    emit_default_option(opt, output);
                    emit_possible_options(opt, output);
                    output.push_str("</dd>");
                }
            }

            output.push_str("</dl>");
        }

        output.push_str("\n\n");
    }

    parents.push(command);

    // Recurse to all of the subcommands.
    for subcommand in command.get_subcommands() {
        generate_command(output, subcommand, parents);
    }

    parents.pop();
}

fn emit_env_option(opt: &clap::Arg, output: &mut String) {
    if opt.is_hide_env_set() {
        return;
    }
    if let Some(env) = opt.get_env() {
        output.push_str(&markdown::to_html(&format!(
            "May also be set with the `{}` environment variable.",
            env.to_string_lossy()
        )));
    }
}

fn emit_default_option(opt: &clap::Arg, output: &mut String) {
    if opt.is_hide_default_value_set() || !opt.get_num_args().expect("built").takes_values() {
        return;
    }

    let values = opt.get_default_values();
    if !values.is_empty() {
        let value = format!(
            "\n[default: {}]",
            opt.get_default_values()
                .iter()
                .map(|s| s.to_string_lossy())
                .join(",")
        );
        output.push_str(&markdown::to_html(&value));
    }
}

fn emit_possible_options(opt: &clap::Arg, output: &mut String) {
    if opt.is_hide_possible_values_set() {
        return;
    }

    let values = opt.get_possible_values();
    if !values.is_empty() {
        let value = format!(
            "\nPossible values:\n{}",
            values
                .into_iter()
                .map(|value| {
                    let name = value.get_name();
                    value.get_help().map_or_else(
                        || format!(" - `{name}`"),
                        |help| format!(" - `{name}`:  {help}"),
                    )
                })
                .collect_vec()
                .join("\n"),
        );
        output.push_str(&markdown::to_html(&value));
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use crate::generate_all::Mode;

    use super::{main, Args};

    #[test]
    fn test_generate_cli_reference() -> Result<()> {
        let mode = if env::var("UV_UPDATE_SCHEMA").as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&Args { mode })
    }
}
