use std::env;

use anyhow::Result;
use clap::Parser;
use tracing::instrument;

use uv_settings::EnvironmentOptions;

use crate::clear_compile::ClearCompileArgs;
use crate::compile::CompileArgs;
use crate::generate_all::Args as GenerateAllArgs;
use crate::generate_cli_reference::Args as GenerateCliReferenceArgs;
use crate::generate_env_vars_reference::Args as GenerateEnvVarsReferenceArgs;
use crate::generate_json_schema::Args as GenerateJsonSchemaArgs;
use crate::generate_options_reference::Args as GenerateOptionsReferenceArgs;
use crate::generate_sysconfig_mappings::Args as GenerateSysconfigMetadataArgs;
use crate::list_packages::ListPackagesArgs;
#[cfg(feature = "render")]
use crate::render_benchmarks::RenderBenchmarksArgs;
use crate::validate_zip::ValidateZipArgs;
use crate::wheel_metadata::WheelMetadataArgs;

mod clear_compile;
mod compile;
mod generate_all;
mod generate_cli_reference;
mod generate_env_vars_reference;
mod generate_json_schema;
mod generate_options_reference;
mod generate_sysconfig_mappings;
mod list_packages;
mod render_benchmarks;
mod validate_zip;
mod wheel_metadata;

const ROOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");

#[derive(Parser)]
enum Cli {
    /// Display the metadata for a `.whl` at a given URL.
    WheelMetadata(WheelMetadataArgs),
    /// Validate that a `.whl` or `.zip` file at a given URL is a valid ZIP file.
    ValidateZip(ValidateZipArgs),
    /// Compile all `.py` to `.pyc` files in the tree.
    Compile(CompileArgs),
    /// Remove all `.pyc` in the tree.
    ClearCompile(ClearCompileArgs),
    /// List all packages from a Simple API index.
    ListPackages(ListPackagesArgs),
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
    /// Generate the sysconfig metadata from derived targets.
    GenerateSysconfigMetadata(GenerateSysconfigMetadataArgs),
    #[cfg(feature = "render")]
    /// Render the benchmarks.
    RenderBenchmarks(RenderBenchmarksArgs),
}

#[instrument] // Anchor span to check for overhead
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let environment = EnvironmentOptions::new()?;
    match cli {
        Cli::WheelMetadata(args) => wheel_metadata::wheel_metadata(args, environment).await?,
        Cli::ValidateZip(args) => validate_zip::validate_zip(args, environment).await?,
        Cli::Compile(args) => compile::compile(args).await?,
        Cli::ClearCompile(args) => clear_compile::clear_compile(&args)?,
        Cli::ListPackages(args) => list_packages::list_packages(args, environment).await?,
        Cli::GenerateAll(args) => generate_all::main(&args).await?,
        Cli::GenerateJSONSchema(args) => generate_json_schema::main(&args)?,
        Cli::GenerateOptionsReference(args) => generate_options_reference::main(&args)?,
        Cli::GenerateCliReference(args) => generate_cli_reference::main(&args)?,
        Cli::GenerateEnvVarsReference(args) => generate_env_vars_reference::main(&args)?,
        Cli::GenerateSysconfigMetadata(args) => generate_sysconfig_mappings::main(&args).await?,
        #[cfg(feature = "render")]
        Cli::RenderBenchmarks(args) => render_benchmarks::render_benchmarks(&args)?,
    }
    Ok(())
}
