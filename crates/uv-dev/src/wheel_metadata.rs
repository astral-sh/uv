use std::str::FromStr;

use anstream::println;
use anyhow::{Result, bail};
use clap::Parser;

use uv_cache::{Cache, CacheArgs};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuiltDist, DirectUrlBuiltDist, IndexCapabilities, RemoteSource};
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
        BaseClientBuilder::default().timeout(environment.http_timeout),
        cache,
    )
    .build();
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
            }),
            &capabilities,
        )
        .await?;
    println!("{metadata:?}");
    Ok(())
}
