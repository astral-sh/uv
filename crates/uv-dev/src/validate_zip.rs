use std::ops::Deref;

use anyhow::{Result, bail};
use clap::Parser;
use futures::TryStreamExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use uv_cache::{Cache, CacheArgs};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_pep508::VerbatimUrl;
use uv_pypi_types::ParsedUrl;
use uv_settings::EnvironmentOptions;

#[derive(Parser)]
pub(crate) struct ValidateZipArgs {
    url: VerbatimUrl,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn validate_zip(
    args: ValidateZipArgs,
    environment: EnvironmentOptions,
) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?.init().await?;
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default()
            .read_timeout(environment.http_read_timeout)
            .connect_timeout(environment.http_connect_timeout),
        cache,
    )
    .build();

    let ParsedUrl::Archive(archive) = ParsedUrl::try_from(args.url.to_url())? else {
        bail!("Only archive URLs are supported");
    };

    let response = client
        .uncached_client(&archive.url)
        .get(archive.url.deref().clone())
        .send()
        .await?;
    let reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read();

    let target = tempfile::TempDir::new()?;

    uv_extract::stream::unzip(args.url.to_url(), reader.compat(), target.path()).await?;

    Ok(())
}
