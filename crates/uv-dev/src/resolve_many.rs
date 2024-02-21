use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use indicatif::ProgressStyle;
use itertools::Itertools;
use tokio::time::Instant;
use tracing::{info, info_span, Span};
use tracing_indicatif::span_ext::IndicatifSpanExt;

use distribution_types::IndexLocations;
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{Requirement, VersionOrUrl};
use platform_host::Platform;
use uv_cache::{Cache, CacheArgs};
use uv_client::{FlatIndex, OwnedArchive, RegistryClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_installer::NoBinary;
use uv_interpreter::Virtualenv;
use uv_normalize::PackageName;
use uv_resolver::InMemoryIndex;
use uv_traits::{BuildContext, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};

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
    /// Run this many tasks in parallel.
    #[clap(long, default_value = "50")]
    num_tasks: usize,
    /// Force the latest version when no version is given.
    #[clap(long)]
    latest_version: bool,
    #[command(flatten)]
    cache_args: CacheArgs,
}

/// Try to find the latest version of a package, ignoring error because we report them during resolution properly
async fn find_latest_version(
    client: &RegistryClient,
    package_name: &PackageName,
) -> Option<Version> {
    let (_, raw_simple_metadata) = client.simple(package_name).await.ok()?;
    let simple_metadata = OwnedArchive::deserialize(&raw_simple_metadata);
    let version = simple_metadata.into_iter().next()?.version;
    Some(version.clone())
}

pub(crate) async fn resolve_many(args: ResolveManyArgs) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?;

    let data = fs_err::read_to_string(&args.requirements)?;

    let tf_models_nightly = PackageName::from_str("tf-models-nightly").unwrap();
    let lines = data
        .lines()
        .map(Requirement::from_str)
        .filter_ok(|req| req.name != tf_models_nightly);

    let requirements: Vec<Requirement> = if let Some(limit) = args.limit {
        lines.take(limit).collect::<Result<_, _>>()?
    } else {
        lines.collect::<Result<_, _>>()?
    };
    let total = requirements.len();

    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    let in_flight = InFlight::default();
    let client = RegistryClientBuilder::new(cache.clone()).build();

    let header_span = info_span!("resolve many");
    header_span.pb_set_style(&ProgressStyle::default_bar());
    header_span.pb_set_length(total as u64);
    let _header_span_enter = header_span.enter();

    let no_build = if args.no_build {
        NoBuild::All
    } else {
        NoBuild::None
    };

    let mut tasks = futures::stream::iter(requirements)
        .map(|requirement| {
            async {
                // Use a separate `InMemoryIndex` for each requirement.
                let index = InMemoryIndex::default();
                let index_locations = IndexLocations::default();
                let setup_py = SetupPyStrategy::default();
                let flat_index = FlatIndex::default();
                let config_settings = ConfigSettings::default();

                // Create a `BuildDispatch` for each requirement.
                let build_dispatch = BuildDispatch::new(
                    &client,
                    &cache,
                    venv.interpreter(),
                    &index_locations,
                    &flat_index,
                    &index,
                    &in_flight,
                    venv.python_executable(),
                    setup_py,
                    &config_settings,
                    &no_build,
                    &NoBinary::None,
                );

                let start = Instant::now();

                let requirement = if args.latest_version && requirement.version_or_url.is_none() {
                    if let Some(version) = find_latest_version(&client, &requirement.name).await {
                        let equals_version = VersionOrUrl::VersionSpecifier(
                            VersionSpecifiers::from(VersionSpecifier::equals_version(version)),
                        );
                        Requirement {
                            name: requirement.name,
                            extras: requirement.extras,
                            version_or_url: Some(equals_version),
                            marker: None,
                        }
                    } else {
                        requirement
                    }
                } else {
                    requirement
                };

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
                let err_formatted =
                    if err
                        .source()
                        .and_then(|err| err.source())
                        .is_some_and(|err| {
                            err.to_string() == "Building source distributions is disabled"
                        })
                    {
                        "Building source distributions is disabled".to_string()
                    } else {
                        err.chain()
                            .map(|err| {
                                let formatted = err.to_string();
                                // Cut overly long c/c++ compile output
                                if formatted.lines().count() > 20 {
                                    let formatted: Vec<_> = formatted.lines().collect();
                                    formatted[..20].join("\n")
                                        + "\n[...]\n"
                                        + &formatted[formatted.len() - 20..].join("\n")
                                } else {
                                    formatted
                                }
                            })
                            .join("\n  Caused by: ")
                    };
                info!(
                    "Error for {} ({}/{}, {} ms): {}",
                    package,
                    success + errors.len(),
                    total,
                    duration.as_millis(),
                    err_formatted
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
