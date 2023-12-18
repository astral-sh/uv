use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use indicatif::ProgressStyle;
use tokio::time::Instant;
use tracing::{info, info_span, span, Level, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::{Cache, CacheArgs};
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;
use pypi_types::IndexUrls;

#[derive(Parser)]
pub(crate) struct ResolveManyArgs {
    /// Path to a file containing one requirement per line.
    requirements: PathBuf,
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
    let cache = Cache::try_from(args.cache_args)?;

    let data = fs_err::read_to_string(&args.requirements)?;
    let lines = data.lines().map(Requirement::from_str);
    let requirements: Vec<Requirement> = if let Some(limit) = args.limit {
        lines.take(limit).collect::<Result<_, _>>()?
    } else {
        lines.collect::<Result<_, _>>()?
    };
    let total = requirements.len();

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
    let build_dispatch = Arc::new(build_dispatch);

    let header_span = info_span!("resolve many");
    header_span.pb_set_style(&ProgressStyle::default_bar());
    header_span.pb_set_length(total as u64);
    let _header_span_enter = header_span.enter();

    let mut tasks = futures::stream::iter(requirements)
        .map(|requirement| {
            let build_dispatch = build_dispatch.clone();
            async move {
                let span = span!(Level::TRACE, "fetching");
                let _enter = span.enter();
                let start = Instant::now();

                let result = build_dispatch.resolve(&[requirement.clone()]).await;

                (requirement.to_string(), start.elapsed(), result)
            }
        })
        .buffer_unordered(args.num_tasks);

    let mut success = 0usize;
    let mut errors = Vec::new();
    while let Some(result) = tasks.next().await {
        let (package, duration, result) = result;
        match result {
            Ok(_) => {
                info!(
                    "Success ({}/{}, {} ms): {}",
                    success + errors.len(),
                    total,
                    duration.as_millis(),
                    package,
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
