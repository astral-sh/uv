use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anstream::println;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use fs_err::File;
use itertools::Itertools;
use petgraph::dot::{Config as DotConfig, Dot};

use distribution_types::IndexUrls;
use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::{Cache, CacheArgs};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_resolver::{Manifest, ResolutionOptions, Resolver};

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
}

pub(crate) async fn resolve_cli(args: ResolveCliArgs) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?;

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    let client = RegistryClientBuilder::new(cache.clone()).build();
    let index_urls = IndexUrls::default();

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        venv.interpreter(),
        &index_urls,
        venv.python_executable(),
        args.no_build,
    );

    // Copied from `BuildDispatch`
    let tags = venv.interpreter().tags()?;
    let resolver = Resolver::new(
        Manifest::simple(args.requirements.clone()),
        ResolutionOptions::default(),
        venv.interpreter().markers(),
        tags,
        &client,
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

    let mut resolution = resolution_graph.requirements();
    resolution.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    #[allow(clippy::print_stderr, clippy::ignored_unit_patterns)]
    match args.format {
        ResolveCliFormat::Compact => {
            println!("{}", resolution.iter().map(ToString::to_string).join(" "));
        }
        ResolveCliFormat::Expanded => {
            for package in resolution {
                println!("{}", package);
            }
        }
    }

    Ok(())
}
