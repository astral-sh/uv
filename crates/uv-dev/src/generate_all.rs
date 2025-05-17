//! Run all code and documentation generation steps.

use anyhow::Result;

use crate::{
    generate_cli_reference, generate_env_vars_reference, generate_json_schema,
    generate_options_reference, generate_sysconfig_mappings,
};

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
pub(crate) enum Mode {
    /// Update the content in the `configuration.md`.
    #[default]
    Write,

    /// Don't write to the file, check if the file is up-to-date and error if not.
    Check,

    /// Write the generated help to stdout.
    DryRun,
}

pub(crate) async fn main(args: &Args) -> Result<()> {
    generate_json_schema::main(&generate_json_schema::Args { mode: args.mode })?;
    generate_options_reference::main(&generate_options_reference::Args { mode: args.mode })?;
    generate_cli_reference::main(&generate_cli_reference::Args { mode: args.mode })?;
    generate_env_vars_reference::main(&generate_env_vars_reference::Args { mode: args.mode })?;
    generate_sysconfig_mappings::main(&generate_sysconfig_mappings::Args { mode: args.mode })
        .await?;
    Ok(())
}
