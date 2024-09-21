use crate::commands::{human_readable_bytes, ExitStatus};
use crate::printer::Printer;
use anyhow::{bail, Result};
use owo_colors::OwoColorize;
use std::fmt::Write;
use tracing::info;
use url::Url;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{KeyringProviderType, TrustedHost};
use uv_publish::{files_for_publishing, upload};

pub(crate) async fn publish(
    paths: Vec<String>,
    publish_url: Url,
    keyring_provider: KeyringProviderType,
    allow_insecure_host: Vec<TrustedHost>,
    username: Option<String>,
    password: Option<String>,
    connectivity: Connectivity,
    native_tls: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    if connectivity.is_offline() {
        bail!("You cannot publish files in offline mode");
    }

    let files = files_for_publishing(paths)?;
    match files.len() {
        0 => bail!("No files found to publish"),
        1 => writeln!(printer.stderr(), "Publishing 1 file")?,
        n => writeln!(printer.stderr(), "Publishing {n} files")?,
    }

    let client = BaseClientBuilder::new()
        // https://github.com/seanmonstar/reqwest/issues/2416
        .retries(0)
        .keyring(keyring_provider)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host)
        // Don't try cloning the request to make an unauthenticated request first.
        // https://github.com/seanmonstar/reqwest/issues/2416
        .only_authenticated(true)
        .build();

    for (file, filename) in files {
        let size = fs_err::metadata(&file)?.len();
        let (bytes, unit) = human_readable_bytes(size);
        writeln!(
            printer.stderr(),
            "{} {filename} {}",
            "Uploading".bold().green(),
            format!("({bytes:.1}{unit})").dimmed()
        )?;
        let uploaded = upload(
            &file,
            &filename,
            &publish_url,
            &client,
            username.as_deref(),
            password.as_deref(),
        )
        .await?; // Filename and/or URL are already attached, if applicable.
        info!("Upload succeeded");
        if !uploaded {
            writeln!(
                printer.stderr(),
                "{}",
                "File already existed, skipping".dimmed()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
