use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;

use platform_host::Platform;
use puffin_build::{BuildKind, SourceBuild, SourceBuildContext};
use puffin_cache::{Cache, CacheArgs};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;
use pypi_types::IndexUrls;

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
    /// You can edit the python sources of an editable install and the changes will be used without
    /// the need to reinstall it.
    #[clap(short, long)]
    editable: bool,
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
    let build_kind = if args.editable {
        BuildKind::Editable
    } else {
        BuildKind::Wheel
    };

    let cache = Cache::try_from(args.cache_args)?;

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;

    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::new(cache.clone()).build(),
        cache,
        venv.interpreter().clone(),
        fs::canonicalize(venv.python_executable())?,
        false,
        IndexUrls::default(),
    );

    let builder = SourceBuild::setup(
        &args.sdist,
        args.subdirectory.as_deref(),
        build_dispatch.interpreter(),
        &build_dispatch,
        SourceBuildContext::default(),
        // Good enough for the dev command
        &args.sdist.display().to_string(),
        build_kind,
    )
    .await?;
    Ok(wheel_dir.join(builder.build(&wheel_dir).await?))
}
