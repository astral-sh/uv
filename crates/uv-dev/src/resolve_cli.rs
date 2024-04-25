use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anstream::println;
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use fs_err::File;
use itertools::Itertools;
use petgraph::dot::{Config as DotConfig, Dot};

use distribution_types::{FlatIndexLocation, IndexLocations, IndexUrl, Resolution, UvRequirement};
use pep508_rs::Requirement;
use uv_cache::{Cache, CacheArgs};
use uv_client::{FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{ConfigSettings, NoBinary, NoBuild, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_resolver::{ExcludeNewer, FlatIndex, InMemoryIndex, Manifest, Options, Resolver};
use uv_types::{BuildIsolation, HashStrategy, InFlight};

#[derive(ValueEnum, Default, Clone)]
pub(crate) enum ResolveCliFormat {
    #[default]
    Compact,
    Expanded,
}

#[derive(Parser)]
pub(crate) struct ResolveCliArgs {
    requirements: Vec<Requirement>,
    /// Write debug output in DOT format for graphviz to this file
    #[clap(long)]
    graphviz: Option<PathBuf>,
    /// Don't build source distributions. This means resolving will not run arbitrary code. The
    /// cached wheels of already built source distributions will be reused.
    #[clap(long)]
    no_build: bool,
    #[clap(long, default_value = "compact")]
    format: ResolveCliFormat,
    #[command(flatten)]
    cache_args: CacheArgs,
    #[arg(long)]
    exclude_newer: Option<ExcludeNewer>,
    #[clap(long, short, env = "UV_INDEX_URL")]
    index_url: Option<IndexUrl>,
    #[clap(long, env = "UV_EXTRA_INDEX_URL")]
    extra_index_url: Vec<IndexUrl>,
    #[clap(long)]
    find_links: Vec<FlatIndexLocation>,
}

pub(crate) async fn resolve_cli(args: ResolveCliArgs) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?;

    let venv = PythonEnvironment::from_virtualenv(&cache)?;
    let index_locations =
        IndexLocations::new(args.index_url, args.extra_index_url, args.find_links, false);
    let index = InMemoryIndex::default();
    let in_flight = InFlight::default();
    let no_build = if args.no_build {
        NoBuild::All
    } else {
        NoBuild::None
    };
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .build();
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(
            entries,
            venv.interpreter().tags()?,
            &HashStrategy::None,
            &no_build,
            &NoBinary::None,
        )
    };
    let config_settings = ConfigSettings::default();

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        SetupPyStrategy::default(),
        &config_settings,
        BuildIsolation::Isolated,
        install_wheel_rs::linker::LinkMode::default(),
        &no_build,
        &NoBinary::None,
    );

    let site_packages = SitePackages::from_executable(&venv)?;

    // Copied from `BuildDispatch`
    let tags = venv.interpreter().tags()?;
    let resolver = Resolver::new(
        Manifest::simple(
            args.requirements
                .iter()
                .cloned()
                .map(UvRequirement::from_requirement)
                .collect::<Result<_, _>>()?,
        ),
        Options::default(),
        venv.interpreter().markers(),
        venv.interpreter(),
        tags,
        &client,
        &flat_index,
        &index,
        &HashStrategy::None,
        &build_dispatch,
        &site_packages,
    )?;
    let resolution_graph = resolver.resolve().await.with_context(|| {
        format!(
            "No solution found when resolving: {}",
            args.requirements.iter().map(ToString::to_string).join(", "),
        )
    })?;

    if let Some(graphviz) = args.graphviz {
        let mut writer = BufWriter::new(File::create(graphviz)?);
        let graphviz = Dot::with_attr_getters(
            resolution_graph.petgraph(),
            &[DotConfig::NodeNoLabel, DotConfig::EdgeNoLabel],
            &|_graph, edge_ref| format!("label={:?}", edge_ref.weight().to_string()),
            &|_graph, (_node_index, dist)| {
                format!("label={:?}", dist.to_string().replace("==", "\n"))
            },
        );
        write!(&mut writer, "{graphviz:?}")?;
    }

    let requirements = Resolution::from(resolution_graph).requirements();

    match args.format {
        ResolveCliFormat::Compact => {
            println!("{}", requirements.iter().map(ToString::to_string).join(" "));
        }
        ResolveCliFormat::Expanded => {
            for package in requirements {
                println!("{}", package);
            }
        }
    }

    Ok(())
}
