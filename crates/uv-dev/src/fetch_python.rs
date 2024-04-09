use std::{
    path::{self, Path, PathBuf},
    str::FromStr,
};

use anyhow::Result;
use clap::Parser;
use futures::{FutureExt, TryStreamExt};
use reqwest::Response;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use url::Url;
use uv_interpreter::PythonVersion;
use uv_toolchain::{Error, PythonDownloadMetadata, PythonDownloadRequest};

#[derive(Parser, Debug)]
pub(crate) struct FetchPythonArgs {
    versions: Vec<String>,
}

pub(crate) async fn fetch_python(args: FetchPythonArgs) -> Result<()> {
    let bootstrap_dir = std::env::var_os("UV_BOOTSTRAP_DIR")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?.join("bin"));

    let versions = if args.versions.is_empty() {
        println!("Reading versions from file...");
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

    // dbg!(&requests);

    let downloads = requests
        .iter()
        .map(
            |request| match PythonDownloadMetadata::from_request(request) {
                Some(download) => download,
                None => panic!("No download found for request {request:?}"),
            },
        )
        .collect::<Vec<_>>();

    for download in downloads {
        download.fetch().await?;
    }

    Ok(())
}

async fn read_versions_file() -> Result<Vec<String>> {
    let mut file = File::open(".python-versions").await?;

    // Since the file is small, just read the whole thing into a buffer then parse
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();

    Ok(lines)
}
