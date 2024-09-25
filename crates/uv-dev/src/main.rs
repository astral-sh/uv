use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Instant;

use anstream::eprintln;
use anyhow::Result;
use clap::Parser;
use owo_colors::OwoColorize;
use tracing::{debug, instrument};
use tracing_durations_export::plot::PlotConfig;
use tracing_durations_export::DurationsLayerBuilder;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::clear_compile::ClearCompileArgs;
use crate::compile::CompileArgs;
use crate::generate_all::Args as GenerateAllArgs;
use crate::generate_cli_reference::Args as GenerateCliReferenceArgs;
use crate::generate_json_schema::Args as GenerateJsonSchemaArgs;
use crate::generate_options_reference::Args as GenerateOptionsReferenceArgs;
#[cfg(feature = "render")]
use crate::render_benchmarks::RenderBenchmarksArgs;
use crate::wheel_metadata::WheelMetadataArgs;

mod clear_compile;
mod compile;
mod generate_all;
mod generate_cli_reference;
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
    #[cfg(feature = "render")]
    /// Render the benchmarks.
    RenderBenchmarks(RenderBenchmarksArgs),
}

#[instrument] // Anchor span to check for overhead
async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::WheelMetadata(args) => wheel_metadata::wheel_metadata(args).await?,
        Cli::Compile(args) => compile::compile(args).await?,
        Cli::ClearCompile(args) => clear_compile::clear_compile(&args)?,
        Cli::GenerateAll(args) => generate_all::main(&args)?,
        Cli::GenerateJSONSchema(args) => generate_json_schema::main(&args)?,
        Cli::GenerateOptionsReference(args) => generate_options_reference::main(&args)?,
        Cli::GenerateCliReference(args) => generate_cli_reference::main(&args)?,
        #[cfg(feature = "render")]
        Cli::RenderBenchmarks(args) => render_benchmarks::render_benchmarks(&args)?,
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let (duration_layer, _guard) = if let Ok(location) = env::var("TRACING_DURATIONS_FILE") {
        let location = PathBuf::from(location);
        if let Some(parent) = location.parent() {
            fs_err::tokio::create_dir_all(&parent)
                .await
                .expect("Failed to create parent of TRACING_DURATIONS_FILE");
        }
        let plot_config = PlotConfig {
            multi_lane: true,
            min_length: None,
            remove: Some(
                ["get_cached_with_callback".to_string()]
                    .into_iter()
                    .collect(),
            ),
            ..PlotConfig::default()
        };
        let (layer, guard) = DurationsLayerBuilder::default()
            .durations_file(&location)
            .plot_file(location.with_extension("svg"))
            .plot_config(plot_config)
            .build()
            .expect("Couldn't create TRACING_DURATIONS_FILE files");
        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    // Show `INFO` messages from the uv crate, but allow `RUST_LOG` to override.
    let default_directive = Directive::from_str("uv=info").unwrap();

    let filter = EnvFilter::builder()
        .with_default_directive(default_directive)
        .from_env()
        .expect("Valid RUST_LOG directives");

    tracing_subscriber::registry()
        .with(duration_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter),
        )
        .init();

    let start = Instant::now();
    let result = run().await;
    debug!("Took {}ms", start.elapsed().as_millis());
    if let Err(err) = result {
        eprintln!("{}", "uv-dev failed".red().bold());
        for err in err.chain() {
            eprintln!("  {}: {}", "Caused by".red().bold(), err);
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
