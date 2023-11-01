#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::ExitCode;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing::debug;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use resolve_many::ResolveManyArgs;

use crate::build::{build, BuildArgs};
use crate::resolve_cli::ResolveCliArgs;

mod build;
mod resolve_cli;
mod resolve_many;

#[derive(Parser)]
enum Cli {
    /// Build a source distribution into a wheel
    Build(BuildArgs),
    /// Resolve many requirements independently in parallel and report failures and sucesses.
    ///
    /// Run `scripts/resolve/get_pypi_top_8k.sh` once, then
    /// ```bash
    /// cargo run --bin puffin-dev -- resolve-many scripts/resolve/pypi_top_8k_flat.txt
    /// ```
    ResolveMany(ResolveManyArgs),
    /// Resolve requirements passed on the CLI
    ResolveCli(ResolveCliArgs),
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::Build(args) => {
            let target = build(args).await?;
            println!("Wheel built to {}", target.display());
        }
        Cli::ResolveMany(args) => {
            resolve_many::resolve_many(args).await?;
        }
        Cli::ResolveCli(args) => {
            resolve_cli::resolve_cli(args).await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let indicatif_layer = IndicatifLayer::new();
    let indicitif_compatible_writer_layer = tracing_subscriber::fmt::layer()
        .with_writer(indicatif_layer.get_stderr_writer())
        .with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            // Show only the important spans
            .parse("puffin_dev=info,puffin_dispatch=info")
            .unwrap()
    });
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(indicitif_compatible_writer_layer)
        .with(indicatif_layer)
        .init();

    let start = Instant::now();
    let result = run().await;
    debug!("Took {}ms", start.elapsed().as_millis());
    if let Err(err) = result {
        eprintln!("{}", "puffin-dev failed".red().bold());
        for err in err.chain() {
            eprintln!("  {}: {}", "Caused by".red().bold(), err);
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
