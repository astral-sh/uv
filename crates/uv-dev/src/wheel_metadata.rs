use std::str::FromStr;

use anstream::println;
use anyhow::{bail, Result};
use clap::Parser;

use distribution_filename::WheelFilename;
use distribution_types::{BuiltDist, DirectUrlBuiltDist, ParsedUrl};
use pep508_rs::VerbatimUrl;
use uv_cache::{Cache, CacheArgs};
use uv_client::RegistryClientBuilder;

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

    let ParsedUrl::Archive(archive) = ParsedUrl::try_from(args.url.to_url())? else {
        bail!("Only https is supported");
    };

    let metadata = client
        .wheel_metadata(&BuiltDist::DirectUrl(DirectUrlBuiltDist {
            filename,
            location: archive.url,
            subdirectory: archive.subdirectory,
            url: args.url,
        }))
        .await?;
    println!("{metadata:?}");
    Ok(())
}
