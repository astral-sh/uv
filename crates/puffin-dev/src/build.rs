use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use fs_err as fs;

use platform_host::Platform;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;

#[derive(Parser)]
pub(crate) struct BuildArgs {
    /// Base python in a way that can be found with `which`
    /// TODO(konstin): Also use proper python parsing here
    #[clap(short, long)]
    python: Option<PathBuf>,
    /// Directory to story the built wheel in
    #[clap(short, long)]
    wheels: Option<PathBuf>,
    sdist: PathBuf,
}

/// Build a source distribution to a wheel
pub(crate) async fn build(args: BuildArgs) -> Result<PathBuf> {
    let wheel_dir = if let Some(wheel_dir) = args.wheels {
        fs::create_dir_all(&wheel_dir).context("Invalid wheel directory")?;
        wheel_dir
    } else {
        env::current_dir()?
    };

    let project_dirs = ProjectDirs::from("", "", "puffin");
    let cache = project_dirs
        .as_ref()
        .map(|project_dirs| project_dirs.cache_dir().to_path_buf())
        .or_else(|| Some(tempfile::tempdir().ok()?.into_path()))
        .unwrap_or_else(|| PathBuf::from(".puffin_cache"));

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, Some(&cache))?;

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
    Ok(wheel_dir.join(wheel))
}
