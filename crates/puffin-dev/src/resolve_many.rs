use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use clap::Parser;
use directories::ProjectDirs;
use fs_err as fs;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indicatif::ProgressStyle;
use tokio::sync::Semaphore;
use tracing::{info, info_span, span, Level, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_traits::BuildContext;

#[derive(Parser)]
pub(crate) struct ResolveManyArgs {
    list: PathBuf,
    #[clap(long)]
    limit: Option<usize>,
    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

pub(crate) async fn resolve_many(args: ResolveManyArgs) -> anyhow::Result<()> {
    let project_dirs = ProjectDirs::from("", "", "puffin");
    let cache = project_dirs
        .as_ref()
        .map(|project_dirs| project_dirs.cache_dir().to_path_buf())
        .or_else(|| Some(tempfile::tempdir().ok()?.into_path()))
        .unwrap_or_else(|| PathBuf::from(".puffin_cache"));

    let data = fs::read_to_string(&args.list)?;
    let lines = data.lines().map(Requirement::from_str);
    let requirements: Vec<Requirement> = if let Some(limit) = args.limit {
        lines.take(limit).collect::<anyhow::Result<_, _>>()?
    } else {
        lines.collect::<anyhow::Result<_, _>>()?
    };

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, Some(&cache))?;
    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::default().cache(Some(&cache)).build(),
        cache.clone(),
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
    );

    let build_dispatch_arc = Arc::new(build_dispatch);
    let mut tasks = FuturesUnordered::new();
    let semaphore = Arc::new(Semaphore::new(50));

    let header_span = info_span!("resolve many");
    header_span.pb_set_style(&ProgressStyle::default_bar());
    header_span.pb_set_length(requirements.len() as u64);

    let _header_span_enter = header_span.enter();

    for requirement in requirements {
        let build_dispatch_arc = build_dispatch_arc.clone();
        let semaphore = semaphore.clone();
        tasks.push(tokio::spawn(async move {
            let span = span!(Level::TRACE, "resolving");
            let _enter = span.enter();

            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let result = build_dispatch_arc.resolve(&[requirement.clone()]).await;
            drop(permit);
            (requirement.to_string(), result)
        }));
    }

    let mut success = 0usize;
    let mut errors = Vec::new();

    while let Some(result) = tasks.next().await {
        let (package, result) = result.unwrap();
        match result {
            Ok(resolution) => {
                info!("Success: {} ({} package(s))", package, resolution.len());
                success += 1;
            }
            Err(err) => {
                info!("Error for {}: {:?}", package, err);
                errors.push(package);
            }
        }
        Span::current().pb_inc(1);
    }
    info!("Errors: {}", errors.join(", "));
    info!("Success: {}, Error: {}", success, errors.len());
    Ok(())
}
