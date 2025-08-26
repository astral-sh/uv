use anyhow::{Context, Result, bail};
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use uv_preview::{Preview, PreviewFeatures};
use uv_static::{EnvVars, parse_boolish_environment_variable};

/// Entrypoint for the `uv-build` Python package.
fn main() -> Result<()> {
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

    // Ad-hoc preview features parsing due to a lack of clap CLI in uv-build.
    let preview_features =
        if parse_boolish_environment_variable(EnvVars::UV_PREVIEW)?.unwrap_or(false) {
            PreviewFeatures::all()
        } else if let Some(preview_features) = env::var_os(EnvVars::UV_PREVIEW_FEATURES) {
            let preview_features = preview_features.to_str().with_context(|| {
                format!("`{}` is not valid UTF-8", EnvVars::UV_PREVIEW_FEATURES)
            })?;
            PreviewFeatures::from_str(preview_features).with_context(|| {
                format!(
                    "Invalid preview features list in `{}`",
                    EnvVars::UV_PREVIEW_FEATURES
                )
            })?
        } else {
            PreviewFeatures::default()
        };
    let preview = Preview::new(preview_features);
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
                preview,
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
                preview,
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
                preview,
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
                preview,
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
