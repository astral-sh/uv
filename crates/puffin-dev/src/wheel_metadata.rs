use std::str::FromStr;

use clap::Parser;
use url::Url;

use distribution_filename::WheelFilename;
use puffin_cache::CacheArgs;
use puffin_client::RegistryClientBuilder;

#[derive(Parser)]
pub(crate) struct WheelMetadataArgs {
    url: Url,
    #[command(flatten)]
    cache_args: CacheArgs,
}

pub(crate) async fn wheel_metadata(args: WheelMetadataArgs) -> anyhow::Result<()> {
    let (_temp_dir, cache) = args.cache_args.get_cache_dir()?;

    let client = RegistryClientBuilder::new(cache).build();

    let filename = WheelFilename::from_str(
        args.url
            .path()
            .rsplit_once('/')
            .unwrap_or(("", args.url.path()))
            .1,
    )?;

    let metadata = client.wheel_metadata_no_index(&filename, &args.url).await?;
    println!("{metadata:?}");
    Ok(())
}
