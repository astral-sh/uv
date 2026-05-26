//! Shared helpers for reading `pylock.toml` (PEP 751) files and deriving a [`Resolution`] and
//! [`HashStrategy`] from them, used by `uv pip install` and `uv pip sync`.

use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing::info_span;

use uv_client::BaseClientBuilder;
use uv_configuration::{BuildOptions, HashCheckingMode, TargetTriple};
use uv_distribution_types::Resolution;
use uv_fs::Simplified;
use uv_normalize::{ExtraName, GroupName};
use uv_python::{Interpreter, PythonVersion};
use uv_resolver::PylockToml;
use uv_types::HashStrategy;

use crate::commands::pip::{resolution_markers, resolution_tags};

/// Read a `pylock.toml` from a local path or HTTP(S) URL and parse it.
///
/// Returns the `install_path` (used to resolve relative package sources in the lock) alongside
/// the parsed [`PylockToml`]. For HTTP(S) sources, the current working directory is used as the
/// install path.
pub(crate) async fn read_pylock_toml(
    pylock: &Path,
    client_builder: &BaseClientBuilder<'_>,
) -> anyhow::Result<(PathBuf, PylockToml)> {
    let (install_path, content) = if pylock.starts_with("http://") || pylock.starts_with("https://")
    {
        let url = uv_redacted::DisplaySafeUrl::parse(&pylock.to_string_lossy())?;
        let client = client_builder.build()?;
        let response = client
            .for_host(&url)
            .get(url::Url::from(url.clone()))
            .send()
            .await?;
        response.error_for_status_ref()?;
        let content = response.text().await?;
        (std::env::current_dir()?, content)
    } else {
        let absolute = std::path::absolute(pylock)?;
        let install_path = absolute
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(PathBuf::new);
        let content = fs_err::tokio::read_to_string(pylock).await?;
        (install_path, content)
    };

    let lock = info_span!("toml::from_str pylock.toml", path = %pylock.display())
        .in_scope(|| toml::from_str::<PylockToml>(&content))
        .with_context(|| format!("Not a valid `pylock.toml` file: {}", pylock.user_display()))?;

    Ok((install_path, lock))
}

/// Verify Python compatibility and convert a parsed [`PylockToml`] into a [`Resolution`] with its
/// [`HashStrategy`].
pub(crate) fn resolve_pylock_toml(
    lock: PylockToml,
    install_path: &Path,
    interpreter: &Interpreter,
    python_version: Option<&PythonVersion>,
    python_platform: Option<&TargetTriple>,
    extras: &[ExtraName],
    groups: &[GroupName],
    build_options: &BuildOptions,
    hash_checking: Option<HashCheckingMode>,
) -> anyhow::Result<(Resolution, HashStrategy)> {
    if let Some(requires_python) = lock.requires_python.as_ref() {
        if !requires_python.contains(interpreter.python_version()) {
            return Err(anyhow::anyhow!(
                "The requested interpreter resolved to Python {}, which is incompatible with the `pylock.toml`'s Python requirement: `{}`",
                interpreter.python_version(),
                requires_python,
            ));
        }
    }

    let tags = resolution_tags(python_version, python_platform, interpreter)?;
    let marker_env = resolution_markers(python_version, python_platform, interpreter);

    let resolution = lock.to_resolution(
        install_path,
        marker_env.markers(),
        extras,
        groups,
        &tags,
        build_options,
    )?;
    let hasher = if let Some(hash_checking) = hash_checking {
        HashStrategy::from_resolution(&resolution, hash_checking)?
    } else {
        HashStrategy::None
    };

    Ok((resolution, hasher))
}
