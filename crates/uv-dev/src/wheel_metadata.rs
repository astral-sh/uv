use std::str::FromStr;

use anstream::println;
use anyhow::{Result, bail};
use clap::Parser;

use uv_cache::{Cache, CacheArgs};
use uv_client::RegistryClientBuilder;
use uv_configuration::MetadataRangeRequest;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuiltDist, DirectUrlBuiltDist, IndexCapabilities, RemoteSource};
use uv_git::GitResolver;
use uv_http::BaseClientBuilder;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::ParsedUrl;
use uv_settings::EnvironmentOptions;

#[derive(Parser)]
pub(crate) struct WheelMetadataArgs {
    url: VerbatimUrl,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn wheel_metadata(
    args: WheelMetadataArgs,
    environment: EnvironmentOptions,
) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?.init().await?;
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default()
            .read_timeout(environment.http_read_timeout)
            .connect_timeout(environment.http_connect_timeout),
        cache,
    )
    .metadata_range_request(MetadataRangeRequest::from(
        environment
            .require_metadata_range_requests
            .unwrap_or_default(),
    ))
    .build()?;
    let resolver = GitResolver::default();
    let capabilities = IndexCapabilities::default();

    let filename = WheelFilename::from_str(&args.url.filename()?)?;

    let ParsedUrl::Archive(archive) = ParsedUrl::try_from(args.url.to_url())? else {
        bail!("Only HTTPS is supported");
    };

    let metadata = client
        .wheel_metadata(
            &BuiltDist::DirectUrl(DirectUrlBuiltDist {
                filename,
                location: Box::new(archive.url),
                url: args.url,
                size: None,
            }),
            &resolver,
            &capabilities,
            None,
        )
        .await?;
    println!("{metadata:?}");
    Ok(())
}
