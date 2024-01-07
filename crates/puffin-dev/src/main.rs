#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Instant;

use anstream::eprintln;
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
use crate::install_many::InstallManyArgs;
use crate::resolve_cli::ResolveCliArgs;
use crate::wheel_metadata::WheelMetadataArgs;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod build;
mod install_many;
mod resolve_cli;
mod resolve_many;
mod wheel_metadata;

#[derive(Parser)]
enum Cli {
    /// Build a source distribution into a wheel
    Build(BuildArgs),
    /// Resolve many requirements independently in parallel and report failures and successes.
    ///
    /// Run `scripts/popular_packages/pypi_8k_downloads.sh` once, then
    /// ```bash
    /// cargo run --bin puffin-dev -- resolve-many scripts/popular_packages/pypi_8k_downloads.txt
    /// ```
    /// or
    /// ```bash
    /// cargo run --bin puffin-dev -- resolve-many scripts/popular_packages/pypi_10k_most_dependents.txt
    /// ```
    ResolveMany(ResolveManyArgs),
    InstallMany(InstallManyArgs),
    /// Resolve requirements passed on the CLI
    Resolve(ResolveCliArgs),
    WheelMetadata(WheelMetadataArgs),
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
        Cli::InstallMany(args) => {
            install_many::install_many(args).await?;
        }
        Cli::Resolve(args) => {
            resolve_cli::resolve_cli(args).await?;
        }
        Cli::WheelMetadata(args) => wheel_metadata::wheel_metadata(args).await?,
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let indicatif_layer = IndicatifLayer::new();
    let indicatif_compatible_writer_layer = tracing_subscriber::fmt::layer()
        .with_writer(indicatif_layer.get_stderr_writer())
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            // Show only the important spans
            .parse("puffin_dev=info,puffin_dispatch=info")
            .unwrap()
    });
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(indicatif_compatible_writer_layer)
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
