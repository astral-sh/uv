use anyhow::Result;
use clap::Parser;
use fs_err as fs;
use futures::StreamExt;
use tokio::time::Instant;
use tracing::{info, info_span, Instrument};
use uv_toolchain::ToolchainRequest;

use uv_fs::Simplified;
use uv_toolchain::downloads::{DownloadResult, Error, PythonDownload, PythonDownloadRequest};
use uv_toolchain::managed::{InstalledToolchain, InstalledToolchains};

#[derive(Parser, Debug)]
pub(crate) struct FetchPythonArgs {
    versions: Vec<String>,
}

pub(crate) async fn fetch_python(args: FetchPythonArgs) -> Result<()> {
    let start = Instant::now();

    let toolchains = InstalledToolchains::from_settings()?.init()?;
    let toolchain_dir = toolchains.root();

    let versions = if args.versions.is_empty() {
        info!("Reading versions from file...");
        read_versions_file().await?
    } else {
        args.versions
    };

    let requests = versions
        .iter()
        .map(|version| {
            PythonDownloadRequest::from_request(ToolchainRequest::parse(version))
                // Populate platform information on the request
                .and_then(PythonDownloadRequest::fill)
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let downloads = requests
        .iter()
        .map(PythonDownload::from_request)
        .collect::<Result<Vec<_>, Error>>()?;

    let client = uv_client::BaseClientBuilder::new().build();

    info!("Fetching requested versions...");
    let mut tasks = futures::stream::iter(downloads.iter())
        .map(|download| {
            async {
                let result = download.fetch(&client, toolchain_dir).await;
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

    for (_, path) in results {
        let installed = InstalledToolchain::new(path)?;
        installed.ensure_externally_managed()?;
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
