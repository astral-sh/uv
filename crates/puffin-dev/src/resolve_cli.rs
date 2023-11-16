use std::fs;
use std::path::PathBuf;

use anstream::println;
use clap::Parser;
use itertools::Itertools;

use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::{CacheArgs, CacheDir};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;

#[derive(Parser)]
pub(crate) struct ResolveCliArgs {
    requirements: Vec<Requirement>,
    #[clap(long)]
    limit: Option<usize>,
    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
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
    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::new(cache_dir.path().clone()).build(),
        cache_dir.path().clone(),
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
        args.no_build,
    );

    let mut resolution = build_dispatch.resolve(&args.requirements).await?;
    resolution.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    // Concise format for dev
    #[allow(clippy::print_stderr, clippy::ignored_unit_patterns)]
    {
        println!("{}", resolution.iter().map(ToString::to_string).join(" "));
    }

    Ok(())
}
