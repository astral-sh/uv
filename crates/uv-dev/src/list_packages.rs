use anstream::println;
use anyhow::Result;
use clap::Parser;

use uv_cache::{Cache, CacheArgs};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_distribution_types::IndexUrl;
use uv_settings::EnvironmentOptions;

#[derive(Parser)]
pub(crate) struct ListPackagesArgs {
    /// The Simple API index URL (e.g., <https://pypi.org/simple>/)
    url: String,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn list_packages(
    args: ListPackagesArgs,
    environment: EnvironmentOptions,
) -> Result<()> {
    let cache = Cache::try_from(args.cache_args)?.init().await?;
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default().timeout(environment.http_timeout),
        cache,
    )
    .build();

    let index_url = IndexUrl::parse(&args.url, None)?;
    let index = client.fetch_simple_index(&index_url).await?;

    for package_name in index.iter() {
        println!("{}", package_name);
    }

    Ok(())
}
