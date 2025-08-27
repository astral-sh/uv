use std::env;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use uv_logging::UvFormat;

/// Entrypoint for the `uv-build` Python package.
fn main() -> Result<()> {
    // Support configuring the log level with `RUST_LOG` (shows only the error level by default) and
    // color.
    //
    // This configuration is a simplified version of the uv logging configuration. When using
    // uv_build through uv proper, the uv logging configuration applies.
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .from_env()
        .context("Invalid RUST_LOG directives")?;
    let stderr_layer = tracing_subscriber::fmt::layer()
        .event_format(UvFormat::default())
        .with_writer(std::sync::Mutex::new(anstream::stderr()))
        .with_filter(filter);
    tracing_subscriber::registry().with(stderr_layer).init();

    // Handrolled to avoid the large clap dependency
    let mut args = env::args_os();
    // Skip the name of the binary
    args.next();
    let command = args
        .next()
        .context("Missing command")?
        .to_str()
        .context("Invalid non-UTF8 command")?
        .to_string();
    match command.as_str() {
        "build-sdist" => {
            let sdist_directory = PathBuf::from(args.next().context("Missing sdist directory")?);
            let filename = uv_build_backend::build_source_dist(
                &env::current_dir()?,
                &sdist_directory,
                uv_version::version(),
            )?;
            // Tell the build frontend about the name of the artifact we built
            writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
        }
        "build-wheel" => {
            let wheel_directory = PathBuf::from(args.next().context("Missing wheel directory")?);
            let metadata_directory = args.next().map(PathBuf::from);
            let filename = uv_build_backend::build_wheel(
                &env::current_dir()?,
                &wheel_directory,
                metadata_directory.as_deref(),
                uv_version::version(),
            )?;
            // Tell the build frontend about the name of the artifact we built
            writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
        }
        "build-editable" => {
            let wheel_directory = PathBuf::from(args.next().context("Missing wheel directory")?);
            let metadata_directory = args.next().map(PathBuf::from);
            let filename = uv_build_backend::build_editable(
                &env::current_dir()?,
                &wheel_directory,
                metadata_directory.as_deref(),
                uv_version::version(),
            )?;
            // Tell the build frontend about the name of the artifact we built
            writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
        }
        "prepare-metadata-for-build-wheel" => {
            let wheel_directory = PathBuf::from(args.next().context("Missing wheel directory")?);
            let filename = uv_build_backend::metadata(
                &env::current_dir()?,
                &wheel_directory,
                uv_version::version(),
            )?;
            // Tell the build frontend about the name of the artifact we built
            writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
        }
        "prepare-metadata-for-build-editable" => {
            let wheel_directory = PathBuf::from(args.next().context("Missing wheel directory")?);
            let filename = uv_build_backend::metadata(
                &env::current_dir()?,
                &wheel_directory,
                uv_version::version(),
            )?;
            // Tell the build frontend about the name of the artifact we built
            writeln!(&mut std::io::stdout(), "{filename}").context("stdout is closed")?;
        }
        "--help" => {
            // This works both as redirect to use the proper uv package and as smoke test.
            writeln!(
                &mut std::io::stderr(),
                "uv_build contains only the PEP 517 build backend for uv and can't be used on the CLI. \
                Use `uv build` or another build frontend instead."
            ).context("stdout is closed")?;
        }
        unknown => {
            bail!(
                "Unknown subcommand: {} (cli: {:?})",
                unknown,
                env::args_os()
            );
        }
    }
    Ok(())
}
