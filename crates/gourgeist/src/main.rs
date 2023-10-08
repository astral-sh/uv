use camino::Utf8PathBuf;
use clap::Parser;
use gourgeist::{create_venv, get_interpreter_info, parse_python_cli};
use std::error::Error;
use std::process::ExitCode;
use std::time::Instant;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
struct Cli {
    path: Option<Utf8PathBuf>,
    #[clap(short, long)]
    python: Option<Utf8PathBuf>,
    #[clap(long)]
    bare: bool,
}

fn run() -> Result<(), gourgeist::Error> {
    let cli = Cli::parse();
    let location = cli.path.unwrap_or(Utf8PathBuf::from(".venv"));
    let python = parse_python_cli(cli.python)?;
    let data = get_interpreter_info(&python)?;
    create_venv(&location, &python, &data, cli.bare)?;

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
            eprintln!("  Caused by: {}", err);
            last_error = err.source();
        }
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
