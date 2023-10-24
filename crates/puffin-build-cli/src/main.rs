#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use directories::ProjectDirs;
use fs_err as fs;
use tracing::debug;
use tracing_subscriber::{EnvFilter, fmt};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use platform_host::Platform;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Venv;

#[derive(Parser)]
struct Args {
    /// Base python in a way that can be found with `which`
    /// TODO(konstin): Also use proper python parsing here
    #[clap(short, long)]
    python: Option<PathBuf>,
    /// Directory to story the built wheel in
    #[clap(short, long)]
    wheels: Option<PathBuf>,
    sdist: PathBuf,
}

async fn run() -> Result<()> {
    let args = Args::parse();
    let wheel_dir = if let Some(wheel_dir) = args.wheels {
        fs::create_dir_all(&wheel_dir).context("Invalid wheel directory")?;
        wheel_dir
    } else {
        env::current_dir()?
    };

    let dirs = ProjectDirs::from("", "", "puffin");
    let cache = dirs
        .as_ref()
        .map(|dir| ProjectDirs::cache_dir(dir).to_path_buf());

    let platform = Platform::current()?;
    let venv = Venv::from_env(platform, cache.as_deref())?;

    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::default().build(),
        cache,
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
    );
    let builder =
        SourceDistributionBuilder::setup(&args.sdist, venv.interpreter_info(), &build_dispatch)
            .await?;
    let wheel = builder.build(&wheel_dir)?;
    println!("Wheel built to {}", wheel_dir.join(wheel).display());
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
        eprintln!("{}", "puffin-build failed".red().bold());
        for err in err.chain() {
            eprintln!("  {}: {}", "Caused by".red().bold(), err);
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
