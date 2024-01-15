use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anstream::println;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use fs_err::File;
use itertools::Itertools;
use petgraph::dot::{Config as DotConfig, Dot};

use distribution_types::{FlatIndexLocation, IndexLocations, IndexUrl, Resolution};
use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::{Cache, CacheArgs};
use puffin_client::{FlatIndex, FlatIndexClient, RegistryClientBuilder};
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_resolver::{Manifest, ResolutionOptions, Resolver};
use puffin_traits::SetupPyStrategy;

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
    exclude_newer: Option<DateTime<Utc>>,
    #[clap(long, short, default_value = IndexUrl::Pypi.as_str(), env = "PUFFIN_INDEX_URL")]
    index_url: IndexUrl,
    #[clap(long)]
    extra_index_url: Vec<IndexUrl>,
    #[clap(long)]
    find_links: Vec<FlatIndexLocation>,
}

pub(crate) async fn resolve_cli(args: ResolveCliArgs) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?;

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    let index_locations =
        IndexLocations::from_args(args.index_url, args.extra_index_url, args.find_links, false);
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .build();
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_indexes()).await?;
        FlatIndex::from_entries(entries, venv.interpreter().tags()?)
    };

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_locations,
        &flat_index,
        venv.python_executable(),
        SetupPyStrategy::default(),
        args.no_build,
    );

    // Copied from `BuildDispatch`
    let tags = venv.interpreter().tags()?;
    let resolver = Resolver::new(
        Manifest::simple(args.requirements.clone()),
        ResolutionOptions::default(),
        venv.interpreter().markers(),
        venv.interpreter(),
        tags,
        &client,
        &flat_index,
        &build_dispatch,
    );
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
                format!(
                    "label={:?}",
                    dist.to_string().replace("==", "\n").to_string()
                )
            },
        );
        write!(&mut writer, "{graphviz:?}")?;
    }

    let requirements = Resolution::from(resolution_graph).requirements();

    #[allow(clippy::print_stderr)]
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
