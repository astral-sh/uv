#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::ExitCode;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing::debug;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use puffin_dev::{build, BuildArgs};

#[derive(Parser)]
enum Cli {
    /// Build a source distribution into a wheel
    Build(BuildArgs),
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::Build(args) => {
            let target = build(args).await?;
            println!("Wheel built to {}", target.display());
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::registry()
        .with(fmt::layer().with_span_events(FmtSpan::CLOSE))
        .with(EnvFilter::from_default_env())
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
