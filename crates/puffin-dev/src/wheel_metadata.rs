use std::str::FromStr;

use anyhow::Result;
use clap::Parser;

use distribution_filename::WheelFilename;
use distribution_types::{BuiltDist, DirectUrlBuiltDist};
use pep508_rs::VerbatimUrl;
use puffin_cache::{Cache, CacheArgs};
use puffin_client::RegistryClientBuilder;

#[derive(Parser)]
pub(crate) struct WheelMetadataArgs {
    url: VerbatimUrl,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn wheel_metadata(args: WheelMetadataArgs) -> Result<()> {
    let cache_dir = Cache::try_from(args.cache_args)?;

    let client = RegistryClientBuilder::new(cache_dir.clone()).build();

    let filename = WheelFilename::from_str(
        args.url
            .path()
            .rsplit_once('/')
            .unwrap_or(("", args.url.path()))
            .1,
    )?;

    let metadata = client
        .wheel_metadata(&BuiltDist::DirectUrl(DirectUrlBuiltDist {
            filename,
            url: args.url,
        }))
        .await?;
    println!("{metadata:?}");
    Ok(())
}
