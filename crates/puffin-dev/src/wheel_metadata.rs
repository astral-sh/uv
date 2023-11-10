use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::Parser;
use directories::ProjectDirs;
use tempfile::tempdir;
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
    // https://github.com/astral-sh/puffin/issues/366
    let cache_dir = if args.no_cache {
        Cow::Owned(tempdir()?.into_path())
    } else if let Some(cache_dir) = args.cache_dir {
        Cow::Owned(cache_dir)
    } else if let Some(project_dirs) = project_dirs.as_ref() {
        Cow::Borrowed(project_dirs.cache_dir())
    } else {
        Cow::Borrowed(Path::new(".puffin_cache"))
    };
    let client = RegistryClientBuilder::new(cache_dir).build();

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
