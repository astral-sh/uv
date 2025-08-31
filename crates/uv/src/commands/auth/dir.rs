use anstream::println;
use owo_colors::OwoColorize;

use uv_auth::TextCredentialStore;
use uv_fs::Simplified;

/// Show the credentials directory.
pub(crate) fn dir() -> anyhow::Result<()> {
    let root = TextCredentialStore::directory_path()?;
    println!("{}", root.simplified_display().cyan());

    Ok(())
}
