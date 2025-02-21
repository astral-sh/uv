use std::env;

use anyhow::Result;
use clap::Parser;
use tracing::instrument;

use crate::clear_compile::ClearCompileArgs;
use crate::compile::CompileArgs;
use crate::generate_all::Args as GenerateAllArgs;
use crate::generate_cli_reference::Args as GenerateCliReferenceArgs;
use crate::generate_env_vars_reference::Args as GenerateEnvVarsReferenceArgs;
use crate::generate_json_schema::Args as GenerateJsonSchemaArgs;
use crate::generate_options_reference::Args as GenerateOptionsReferenceArgs;
#[cfg(feature = "render")]
use crate::render_benchmarks::RenderBenchmarksArgs;
use crate::wheel_metadata::WheelMetadataArgs;

mod clear_compile;
mod compile;
mod generate_all;
mod generate_cli_reference;
mod generate_env_vars_reference;
mod generate_json_schema;
mod generate_options_reference;
mod render_benchmarks;
mod wheel_metadata;

const ROOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");

#[derive(Parser)]
enum Cli {
    /// Display the metadata for a `.whl` at a given URL.
    WheelMetadata(WheelMetadataArgs),
    /// Compile all `.py` to `.pyc` files in the tree.
    Compile(CompileArgs),
    /// Remove all `.pyc` in the tree.
    ClearCompile(ClearCompileArgs),
    /// Run all code and documentation generation steps.
    GenerateAll(GenerateAllArgs),
    /// Generate JSON schema for the TOML configuration file.
    GenerateJSONSchema(GenerateJsonSchemaArgs),
    /// Generate the options reference for the documentation.
    GenerateOptionsReference(GenerateOptionsReferenceArgs),
    /// Generate the CLI reference for the documentation.
    GenerateCliReference(GenerateCliReferenceArgs),
    /// Generate the environment variables reference for the documentation.
    GenerateEnvVarsReference(GenerateEnvVarsReferenceArgs),
    #[cfg(feature = "render")]
    /// Render the benchmarks.
    RenderBenchmarks(RenderBenchmarksArgs),
}

#[instrument] // Anchor span to check for overhead
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::WheelMetadata(args) => wheel_metadata::wheel_metadata(args).await?,
        Cli::Compile(args) => compile::compile(args).await?,
        Cli::ClearCompile(args) => clear_compile::clear_compile(&args)?,
        Cli::GenerateAll(args) => generate_all::main(&args)?,
        Cli::GenerateJSONSchema(args) => generate_json_schema::main(&args)?,
        Cli::GenerateOptionsReference(args) => generate_options_reference::main(&args)?,
        Cli::GenerateCliReference(args) => generate_cli_reference::main(&args)?,
        Cli::GenerateEnvVarsReference(args) => generate_env_vars_reference::main(&args)?,
        #[cfg(feature = "render")]
        Cli::RenderBenchmarks(args) => render_benchmarks::render_benchmarks(&args)?,
    }
    Ok(())
}
