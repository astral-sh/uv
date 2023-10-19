#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use directories::ProjectDirs;
use fs_err as fs;
use puffin_build::{Error, SourceDistributionBuilder};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use std::{env, io};
use tracing::debug;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

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

async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let wheel_dir = if let Some(wheel_dir) = args.wheels {
        fs::create_dir_all(&wheel_dir).context("Invalid wheel directory")?;
        wheel_dir
    } else {
        env::current_dir()?
    };

    let dirs = ProjectDirs::from("", "", "puffin");
    let cache = dirs.as_ref().map(ProjectDirs::cache_dir);

    // TODO: That's no way to deal with paths in PATH
    let base_python = which::which(args.python.unwrap_or("python3".into())).map_err(|err| {
        Error::IO(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Can't find `python3` ({err})"),
        ))
    })?;
    let interpreter_info = gourgeist::get_interpreter_info(&base_python)?;

    let builder =
        SourceDistributionBuilder::setup(&args.sdist, &base_python, &interpreter_info, cache)
            .await?;
    let wheel = builder.build(&wheel_dir)?;
    println!("Wheel built to {}", wheel.display());
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
