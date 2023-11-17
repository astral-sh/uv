use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anstream::println;
use anyhow::Context;
use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use petgraph::dot::{Config as DotConfig, Dot};

use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::{CacheArgs, CacheDir};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_resolver::{Manifest, PreReleaseMode, ResolutionMode, Resolver};

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
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn resolve_cli(args: ResolveCliArgs) -> anyhow::Result<()> {
    let cache_dir = CacheDir::try_from(args.cache_args)?;

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, Some(cache_dir.path()))?;
    let client = RegistryClientBuilder::new(cache_dir.path().clone()).build();
    let build_dispatch = BuildDispatch::new(
        client.clone(),
        cache_dir.path().clone(),
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
        args.no_build,
    );

    // Copied from `BuildDispatch`
    let tags = Tags::from_env(
        venv.interpreter_info().platform(),
        venv.interpreter_info().simple_version(),
    )?;
    let resolver = Resolver::new(
        // TODO(konstin): Split settings (for all resolutions) and inputs (only for this
        // resolution) and attach the former to Self.
        Manifest::new(
            args.requirements.clone(),
            Vec::default(),
            Vec::default(),
            ResolutionMode::default(),
            PreReleaseMode::default(),
            None, // TODO(zanieb): We may want to provide a project name here
            None,
        ),
        venv.interpreter_info().markers(),
        &tags,
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

    // Concise format for dev
    #[allow(clippy::print_stderr, clippy::ignored_unit_patterns)]
    {
        println!("{}", resolution.iter().map(ToString::to_string).join(" "));
    }

    Ok(())
}
