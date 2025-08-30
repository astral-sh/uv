use anstream::println;
use owo_colors::OwoColorize;

use uv_auth::{PyxTokenStore, Service, TextCredentialStore};
use uv_fs::Simplified;

/// Show the credentials directory.
pub(crate) fn dir(service: Option<&Service>) -> anyhow::Result<()> {
    if let Some(service) = service {
        let pyx_store = PyxTokenStore::from_settings()?;
        if pyx_store.is_known_domain(service.url()) {
            println!("{}", pyx_store.root().simplified_display().cyan());
            return Ok(());
        }
    }

    let root = TextCredentialStore::directory_path()?;
    println!("{}", root.simplified_display().cyan());
    Ok(())
}
