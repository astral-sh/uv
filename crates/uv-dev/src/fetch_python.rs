use anyhow::Result;
use clap::Parser;
use fs_err as fs;
#[cfg(unix)]
use fs_err::tokio::symlink;
use futures::StreamExt;
#[cfg(unix)]
use itertools::Itertools;
use std::str::FromStr;
#[cfg(unix)]
use std::{collections::HashMap, path::PathBuf};
use tokio::time::Instant;
use tracing::{info, info_span, Instrument};

use uv_fs::Simplified;
use uv_interpreter::managed::{
    DownloadResult, Error, PythonDownload, PythonDownloadRequest, TOOLCHAIN_DIRECTORY,
};

#[derive(Parser, Debug)]
pub(crate) struct FetchPythonArgs {
    versions: Vec<String>,
}

pub(crate) async fn fetch_python(args: FetchPythonArgs) -> Result<()> {
    let start = Instant::now();

    let bootstrap_dir = &*TOOLCHAIN_DIRECTORY;

    fs_err::create_dir_all(bootstrap_dir)?;

    let versions = if args.versions.is_empty() {
        info!("Reading versions from file...");
        read_versions_file().await?
    } else {
        args.versions
    };

    let requests = versions
        .iter()
        .map(|version| {
            PythonDownloadRequest::from_str(version).and_then(PythonDownloadRequest::fill)
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let downloads = requests
        .iter()
        .map(|request| match PythonDownload::from_request(request) {
            Some(download) => download,
            None => panic!("No download found for request {request:?}"),
        })
        .collect::<Vec<_>>();

    let client = uv_client::BaseClientBuilder::new().build();

    info!("Fetching requested versions...");
    let mut tasks = futures::stream::iter(downloads.iter())
        .map(|download| {
            async {
                let result = download.fetch(&client, bootstrap_dir).await;
                (download.python_version(), result)
            }
            .instrument(info_span!("download", key = %download))
        })
        .buffered(4);

    let mut results = Vec::new();
    let mut downloaded = 0;
    while let Some(task) = tasks.next().await {
        let (version, result) = task;
        let path = match result? {
            DownloadResult::AlreadyAvailable(path) => {
                info!("Found existing download for v{}", version);
                path
            }
            DownloadResult::Fetched(path) => {
                info!("Downloaded v{} to {}", version, path.user_display());
                downloaded += 1;
                path
            }
        };
        results.push((version, path));
    }

    if downloaded > 0 {
        let s = if downloaded == 1 { "" } else { "s" };
        info!(
            "Fetched {} in {}s",
            format!("{} version{}", downloaded, s),
            start.elapsed().as_secs()
        );
    } else {
        info!("All versions downloaded already.");
    };

    // Order matters here, as we overwrite previous links
    info!("Installing to `{}`...", bootstrap_dir.user_display());

    // On Windows, linking the executable generally results in broken installations
    // and each toolchain path will need to be added to the PATH separately in the
    // desired order
    #[cfg(unix)]
    {
        let mut links: HashMap<PathBuf, PathBuf> = HashMap::new();
        for (version, path) in results {
            // TODO(zanieb): This path should be a part of the download metadata
            let executable = path.join("install").join("bin").join("python3");
            for target in [
                bootstrap_dir.join(format!("python{}", version.python_full_version())),
                bootstrap_dir.join(format!("python{}.{}", version.major(), version.minor())),
                bootstrap_dir.join(format!("python{}", version.major())),
                bootstrap_dir.join("python"),
            ] {
                // Attempt to remove it, we'll fail on link if we couldn't remove it for some reason
                // but if it's missing we don't want to error
                let _ = fs::remove_file(&target);
                symlink(&executable, &target).await?;
                links.insert(target, executable.clone());
            }
        }
        for (target, executable) in links.iter().sorted() {
            info!(
                "Linked `{}` to `{}`",
                target.user_display(),
                executable.user_display()
            );
        }
    };

    info!("Installed {} versions", requests.len());

    Ok(())
}

async fn read_versions_file() -> Result<Vec<String>> {
    let lines: Vec<String> = fs::tokio::read_to_string(".python-versions")
        .await?
        .lines()
        .map(ToString::to_string)
        .collect();
    Ok(lines)
}
