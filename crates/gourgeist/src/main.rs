use std::error::Error;
use std::process::ExitCode;
use std::time::Instant;

use anstream::eprintln;
use camino::Utf8PathBuf;
use clap::Parser;
use directories::ProjectDirs;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use gourgeist::{create_bare_venv, parse_python_cli};
use platform_host::Platform;
use uv_cache::Cache;
use uv_interpreter::Interpreter;

#[derive(Parser, Debug)]
struct Cli {
    path: Option<Utf8PathBuf>,
    #[clap(short, long)]
    python: Option<Utf8PathBuf>,
}

fn run() -> Result<(), gourgeist::Error> {
    let cli = Cli::parse();
    let location = cli.path.unwrap_or(Utf8PathBuf::from(".venv"));
    let python = parse_python_cli(cli.python)?;
    let platform = Platform::current()?;
    let cache = if let Some(project_dirs) = ProjectDirs::from("", "", "gourgeist") {
        Cache::from_path(project_dirs.cache_dir())?
    } else {
        Cache::from_path(".gourgeist_cache")?
    };
    let info = Interpreter::query(python.as_std_path(), &platform, &cache).unwrap();
    create_bare_venv(&location, &info)?;
    Ok(())
}

fn main() -> ExitCode {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let start = Instant::now();
    let result = run();
    info!("Took {}ms", start.elapsed().as_millis());
    if let Err(err) = result {
        eprintln!("💥 virtualenv creator failed");

        let mut last_error: Option<&(dyn Error + 'static)> = Some(&err);
        while let Some(err) = last_error {
            eprintln!("  Caused by: {err}");
            last_error = err.source();
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
