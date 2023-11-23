use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;

use platform_host::Platform;
use puffin_cache::{CacheArgs, CacheDir};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;

#[derive(Parser)]
pub(crate) struct BuildArgs {
    /// Base python in a way that can be found with `which`
    /// TODO(konstin): Also use proper python parsing here
    #[clap(short, long)]
    python: Option<PathBuf>,
    /// Directory to story the built wheel in
    #[clap(short, long)]
    wheels: Option<PathBuf>,
    /// The source distribution to build, either a directory or a source archive.
    sdist: PathBuf,
    /// The subdirectory to build within the source distribution.
    subdirectory: Option<PathBuf>,
    #[command(flatten)]
    cache_args: CacheArgs,
}

/// Build a source distribution to a wheel
pub(crate) async fn build(args: BuildArgs) -> Result<PathBuf> {
    let wheel_dir = if let Some(wheel_dir) = args.wheels {
        fs::create_dir_all(&wheel_dir).context("Invalid wheel directory")?;
        wheel_dir
    } else {
        env::current_dir()?
    };

    let cache_dir = CacheDir::try_from(args.cache_args)?;

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, Some(cache_dir.path()))?;

    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::new(cache_dir.path().clone()).build(),
        cache_dir.path().clone(),
        venv.interpreter().clone(),
        fs::canonicalize(venv.python_executable())?,
        false,
    );
    let wheel = build_dispatch
        .build_source(
            &args.sdist,
            args.subdirectory.as_deref(),
            &wheel_dir,
            // Good enough for the dev command
            &args.sdist.display().to_string(),
        )
        .await?;
    Ok(wheel_dir.join(wheel))
}
