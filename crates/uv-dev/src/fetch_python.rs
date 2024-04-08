use fs_err as fs;
use std::collections::HashMap;
use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use clap::Parser;

use futures::StreamExt;

use itertools::Itertools;
use tokio::io::AsyncReadExt;
use tokio::{fs::File, time::Instant};

use tracing::{info, info_span, Instrument};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use uv_fs::Simplified;
use uv_toolchain::{Error, PythonDownload, PythonDownloadRequest};

#[derive(Parser, Debug)]
pub(crate) struct FetchPythonArgs {
    versions: Vec<String>,
}

pub(crate) async fn fetch_python(args: FetchPythonArgs) -> Result<()> {
    let start = Instant::now();

    let bootstrap_dir = std::env::var_os("UV_BOOTSTRAP_DIR")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?.join("bin"));

    fs_err::create_dir_all(&bootstrap_dir)?;

    let versions = if args.versions.is_empty() {
        info!("Reading versions from file...");
        read_versions_file().await?
    } else {
        args.versions
    };

    let requests = versions
        .iter()
        .map(|version| match PythonDownloadRequest::from_str(version) {
            Ok(request) => request.fill(),
            err @ Err(_) => err,
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

    let mut tasks = futures::stream::iter(downloads.iter())
        .map(|download| {
            async {
                let result = download.fetch(&client, &bootstrap_dir).await;
                (download.python_version(), result)
            }
            .instrument(info_span!("download", key = %download))
        })
        .buffered(2);

    let mut results = Vec::new();
    while let Some(task) = tasks.next().await {
        let (version, result) = task;
        let path = result?;
        info!("Downloaded {} to {}", version, &path.user_display());
        results.push((version, path));
    }

    let s = if downloads.len() == 1 { "" } else { "s" };
    info!(
        "Fetched {} in {} ms",
        format!("{} version{}", downloads.len(), s),
        start.elapsed().as_millis()
    );

    // Order matters here, as we overwrite previous links
    let mut links = HashMap::new();
    for (version, path) in results {
        // TODO(zanieb): This path should be a part of the download metadata
        let executable = path.join("install").join("bin").join("python3");
        for target in &[
            bootstrap_dir.join(format!("python{}", version.python_full_version())),
            bootstrap_dir.join(format!("python{}.{}", version.major(), version.minor())),
            bootstrap_dir.join(format!("python{}", version.major())),
            bootstrap_dir.join("python"),
        ] {
            // Attempt to remove it, we'll fail on link if we couldn't remove it for some reason
            // but if it's missing we don't want to error
            let _ = fs::remove_file(target);
            if cfg!(unix) {
                symlink(&executable, target)?;
            } else if cfg!(windows) {
                // Windows requires higher permissions for symbolic links
                fs::hard_link(&executable, target)?;
            } else {
                panic!("Only Windows and Unix are supported");
            }
            links.insert(target.clone(), executable.clone());
        }
    }
    for (target, executable) in links.iter().sorted() {
        info!(
            "Linked {} to {}",
            target.user_display(),
            executable.user_display()
        );
    }

    Ok(())
}

async fn read_versions_file() -> Result<Vec<String>> {
    let mut file = File::open(".python-versions").await?;

    // Since the file is small, just read the whole thing into a buffer then parse
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let lines: Vec<String> = contents
        .lines()
        .map(std::string::ToString::to_string)
        .collect();
    Ok(lines)
}
