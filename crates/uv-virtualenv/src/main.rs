use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anstream::eprintln;
use clap::Parser;
use directories::ProjectDirs;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use platform_host::Platform;
use uv_cache::Cache;
use uv_interpreter::{find_default_python, find_requested_python};
use uv_virtualenv::{create_bare_venv, Prompt};

#[derive(Parser, Debug)]
struct Cli {
    path: Option<PathBuf>,
    #[clap(short, long)]
    python: Option<String>,
    #[clap(long)]
    prompt: Option<String>,
    #[clap(long)]
    system_site_packages: bool,
}

fn run() -> Result<(), uv_virtualenv::Error> {
    let cli = Cli::parse();
    let location = cli.path.unwrap_or(PathBuf::from(".venv"));
    let platform = Platform::current()?;
    let cache = if let Some(project_dirs) = ProjectDirs::from("", "", "uv-virtualenv") {
        Cache::from_path(project_dirs.cache_dir())?
    } else {
        Cache::from_path(".cache")?
    };
    let interpreter = if let Some(python_request) = &cli.python {
        find_requested_python(python_request, &platform, &cache)?.ok_or(
            uv_interpreter::Error::NoSuchPython(python_request.to_string()),
        )?
    } else {
        find_default_python(&platform, &cache)?
    };
    create_bare_venv(
        &location,
        &interpreter,
        Prompt::from_args(cli.prompt),
        cli.system_site_packages,
        Vec::new(),
    )?;
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
