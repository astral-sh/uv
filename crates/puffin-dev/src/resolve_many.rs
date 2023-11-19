use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use fs_err as fs;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indicatif::ProgressStyle;
use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::{CacheArgs, CacheDir};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tracing::{info, info_span, span, Level, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

#[derive(Parser)]
pub(crate) struct ResolveManyArgs {
    list: PathBuf,
    #[clap(long)]
    limit: Option<usize>,
    /// Don't build source distributions. This means resolving will not run arbitrary code. The
    /// cached wheels of already built source distributions will be reused.
    #[clap(long)]
    no_build: bool,
    /// Run this many tasks in parallel
    #[clap(long, default_value = "50")]
    num_tasks: usize,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn resolve_many(args: ResolveManyArgs) -> Result<()> {
    let cache_dir = CacheDir::try_from(args.cache_args)?;

    let data = fs::read_to_string(&args.list)?;
    let lines = data.lines().map(Requirement::from_str);
    let requirements: Vec<Requirement> = if let Some(limit) = args.limit {
        lines.take(limit).collect::<Result<_, _>>()?
    } else {
        lines.collect::<Result<_, _>>()?
    };

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, Some(cache_dir.path()))?;
    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::new(cache_dir.path().clone()).build(),
        cache_dir.path().clone(),
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
        args.no_build,
    );

    let build_dispatch_arc = Arc::new(build_dispatch);
    let mut tasks = FuturesUnordered::new();
    let semaphore = Arc::new(Semaphore::new(args.num_tasks));

    let header_span = info_span!("resolve many");
    header_span.pb_set_style(&ProgressStyle::default_bar());
    let total = requirements.len();
    header_span.pb_set_length(total as u64);

    let _header_span_enter = header_span.enter();

    for requirement in requirements {
        let build_dispatch_arc = build_dispatch_arc.clone();
        let semaphore = semaphore.clone();
        tasks.push(tokio::spawn(async move {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let span = span!(Level::TRACE, "resolving");
            let _enter = span.enter();
            let start = Instant::now();

            let result = build_dispatch_arc.resolve(&[requirement.clone()]).await;

            drop(permit);
            (requirement.to_string(), start.elapsed(), result)
        }));
    }

    let mut success = 0usize;
    let mut errors = Vec::new();

    while let Some(result) = tasks.next().await {
        let (package, duration, result) = result.unwrap();

        match result {
            Ok(resolution) => {
                info!(
                    "Success ({}/{}, {} ms): {} ({} package(s))",
                    success + errors.len(),
                    total,
                    duration.as_millis(),
                    package,
                    resolution.len(),
                );
                success += 1;
            }
            Err(err) => {
                info!(
                    "Error for {} ({}/{}, {} ms):: {:?}",
                    package,
                    success + errors.len(),
                    total,
                    duration.as_millis(),
                    err,
                );
                errors.push(package);
            }
        }
        Span::current().pb_inc(1);
    }
    info!("Errors: {}", errors.join(", "));
    info!("Success: {}, Error: {}", success, errors.len());
    Ok(())
}
