use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use fs_err as fs;
use rustc_hash::FxHashMap;

use distribution_types::IndexLocations;
use uv_build::{SourceBuild, SourceBuildContext};
use uv_cache::{Cache, CacheArgs};
use uv_client::RegistryClientBuilder;
use uv_configuration::{
    BuildKind, BuildOptions, Concurrency, ConfigSettings, IndexStrategy, SetupPyStrategy,
    SourceStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_git::GitResolver;
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};
use uv_resolver::{FlatIndex, InMemoryIndex};
use uv_types::{BuildIsolation, InFlight};

#[derive(Parser)]
pub(crate) struct BuildArgs {
    /// Base python in a way that can be found with `which`
    /// TODO(konstin): Also use proper python parsing here
    #[clap(short, long)]
    python: Option<PathBuf>,
    /// Directory to story the built wheel in
    #[clap(short, long)]
    wheels: Option<PathBuf>,
    /// The source distribution to build, as a directory.
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

    let cache = Cache::try_from(args.cache_args)?.init()?;

    let client = RegistryClientBuilder::new(cache.clone()).build();
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let exclude_newer = None;
    let flat_index = FlatIndex::default();
    let git = GitResolver::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let index_urls = IndexLocations::default();
    let index_strategy = IndexStrategy::default();
    let setup_py = SetupPyStrategy::default();
    let sources = SourceStrategy::default();
    let python = PythonEnvironment::find(
        &PythonRequest::default(),
        EnvironmentPreference::OnlyVirtual,
        &cache,
    )?;
    let build_options = BuildOptions::default();
    let build_constraints = [];

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &build_constraints,
        python.interpreter(),
        &index_urls,
        &flat_index,
        &index,
        &git,
        &in_flight,
        index_strategy,
        setup_py,
        &config_settings,
        BuildIsolation::Isolated,
        install_wheel_rs::linker::LinkMode::default(),
        &build_options,
        exclude_newer,
        sources,
        concurrency,
    );

    let builder = SourceBuild::setup(
        &args.sdist,
        args.subdirectory.as_deref(),
        python.interpreter(),
        &build_dispatch,
        SourceBuildContext::default(),
        args.sdist.display().to_string(),
        setup_py,
        config_settings.clone(),
        BuildIsolation::Isolated,
        build_kind,
        FxHashMap::default(),
        concurrency.builds,
    )
    .await?;
    Ok(wheel_dir.join(builder.build_wheel(&wheel_dir).await?))
}
