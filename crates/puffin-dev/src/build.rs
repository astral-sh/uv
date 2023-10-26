use anyhow::Context;
use clap::Parser;
use directories::ProjectDirs;
use fs_err as fs;
use platform_host::Platform;
use puffin_build::SourceDistributionBuilder;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
pub struct BuildArgs {
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
pub async fn build(args: BuildArgs) -> anyhow::Result<()> {
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
    let venv = Virtualenv::from_env(platform, cache.as_deref())?;

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
