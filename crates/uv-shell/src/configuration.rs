use std::path::Path;

use tokio::io::AsyncWriteExt;
use tracing::debug;

use uv_fs::Simplified;

/// The change made while ensuring a command is present in a shell configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationUpdate {
    /// The configuration file was created.
    Created,
    /// The command was appended to an existing configuration file.
    Updated,
    /// The command was already present in an uncommented line.
    Unchanged,
}

/// Append a command to a shell configuration file if no uncommented line contains it.
pub async fn update_configuration_file(
    path: &Path,
    command: &str,
) -> std::io::Result<ConfigurationUpdate> {
    let (contents, update) = match fs_err::tokio::read_to_string(path).await {
        Ok(contents) => {
            if contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.starts_with('#'))
                .any(|line| line.contains(command))
            {
                debug!(
                    "Skipping already-updated configuration file: {}",
                    path.simplified_display()
                );
                return Ok(ConfigurationUpdate::Unchanged);
            }
            (
                format!("{contents}\n# uv\n{command}\n"),
                ConfigurationUpdate::Updated,
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs_err::tokio::create_dir_all(parent).await?;
            }
            (format!("# uv\n{command}\n"), ConfigurationUpdate::Created)
        }
        Err(error) => return Err(error),
    };

    let mut configuration_file = fs_err::tokio::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .await?;
    configuration_file.write_all(contents.as_bytes()).await?;
    configuration_file.flush().await?;
    Ok(update)
}
