use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;

use distribution_types::IndexLocations;
use platform_host::Platform;
use uv_build::{SourceBuild, SourceBuildContext};
use uv_cache::{Cache, CacheArgs};
use uv_client::{FlatIndex, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_installer::NoBinary;
use uv_interpreter::Virtualenv;
use uv_resolver::InMemoryIndex;
use uv_traits::{BuildContext, BuildKind, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};

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
    let client = RegistryClientBuilder::new(cache.clone()).build();
    let index_urls = IndexLocations::default();
    let flat_index = FlatIndex::default();
    let index = InMemoryIndex::default();
    let setup_py = SetupPyStrategy::default();
    let in_flight = InFlight::default();
    let config_settings = ConfigSettings::default();

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_urls,
        &flat_index,
        &index,
        &in_flight,
        venv.python_executable(),
        setup_py,
        &config_settings,
        &NoBuild::None,
        &NoBinary::None,
    );

    let builder = SourceBuild::setup(
        &args.sdist,
        args.subdirectory.as_deref(),
        build_dispatch.interpreter(),
        &build_dispatch,
        SourceBuildContext::default(),
        args.sdist.display().to_string(),
        setup_py,
        config_settings.clone(),
        build_kind,
    )
    .await?;
    Ok(wheel_dir.join(builder.build(&wheel_dir).await?))
}
