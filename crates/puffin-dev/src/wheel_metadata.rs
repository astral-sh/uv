use std::str::FromStr;

use clap::Parser;
use url::Url;

use anyhow::Result;
use distribution_filename::WheelFilename;
use puffin_cache::metadata::WheelMetadataCachingIndex;
use puffin_cache::{CacheArgs, CacheDir};
use puffin_client::RegistryClientBuilder;

#[derive(Parser)]
pub(crate) struct WheelMetadataArgs {
    url: Url,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn wheel_metadata(args: WheelMetadataArgs) -> Result<()> {
    let cache_dir = CacheDir::try_from(args.cache_args)?;

    let client = RegistryClientBuilder::new(cache_dir.path().clone()).build();

    let filename = WheelFilename::from_str(
        args.url
            .path()
            .rsplit_once('/')
            .unwrap_or(("", args.url.path()))
            .1,
    )?;

    let metadata = client
        .wheel_metadata_no_pep658(&filename, &args.url, WheelMetadataCachingIndex::Url)
        .await?;
    println!("{metadata:?}");
    Ok(())
}
