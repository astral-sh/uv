use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;
use directories::ProjectDirs;
use url::Url;

use distribution_filename::WheelFilename;
use puffin_client::RegistryClientBuilder;

#[derive(Parser)]
pub(crate) struct WheelMetadataArgs {
    url: Url,
    /// Avoid reading from or writing to the cache.
    #[arg(global = true, long, short)]
    no_cache: bool,
    /// Path to the cache directory.
    #[arg(global = true, long, env = "PUFFIN_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
}

pub(crate) async fn wheel_metadata(args: WheelMetadataArgs) -> anyhow::Result<()> {
    let project_dirs = ProjectDirs::from("", "", "puffin");
    let cache_dir = (!args.no_cache)
        .then(|| {
            args.cache_dir
                .as_deref()
                .or_else(|| project_dirs.as_ref().map(ProjectDirs::cache_dir))
        })
        .flatten();
    let client = RegistryClientBuilder::default().cache(cache_dir).build();

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
