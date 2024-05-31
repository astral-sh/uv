use std::path::Path;

use anstream::eprint;
use anyhow::Result;

use requirements_txt::RequirementsTxt;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::Upgrade;
use uv_distribution::ProjectWorkspace;
use uv_resolver::{Lock, Preference, PreferenceError};

/// Load the preferred requirements from an existing `requirements.txt`, applying the upgrade strategy.
pub async fn read_requirements_txt(
    output_file: Option<&Path>,
    upgrade: &Upgrade,
) -> Result<Vec<Preference>> {
    // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(Vec::new());
    }

    // If the lockfile doesn't exist, don't respect any pinned versions.
    let Some(output_file) = output_file.filter(|path| path.exists()) else {
        return Ok(Vec::new());
    };

    // Parse the requirements from the lockfile.
    let requirements_txt = RequirementsTxt::parse(
        output_file,
        std::env::current_dir()?,
        &BaseClientBuilder::new().connectivity(Connectivity::Offline),
    )
    .await?;

    // Map each entry in the lockfile to a preference.
    let preferences = requirements_txt
        .requirements
        .into_iter()
        .map(Preference::from_entry)
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, PreferenceError>>()?;

    // Apply the upgrade strategy to the requirements.
    Ok(match upgrade {
        // Respect all pinned versions from the existing lockfile.
        Upgrade::None => preferences,
        // Ignore all pinned versions from the existing lockfile.
        Upgrade::All => vec![],
        // Ignore pinned versions for the specified packages.
        Upgrade::Packages(packages) => preferences
            .into_iter()
            .filter(|preference| !packages.contains(preference.name()))
            .collect(),
    })
}

/// Load the preferred requirements from an existing lockfile, applying the upgrade strategy.
pub async fn read_lockfile(
    project: &ProjectWorkspace,
    upgrade: &Upgrade,
) -> Result<Vec<Preference>> {
    // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(Vec::new());
    }

    // If an existing lockfile exists, build up a set of preferences.
    let lockfile = project.workspace().root().join("uv.lock");
    let lock = match fs_err::tokio::read_to_string(&lockfile).await {
        Ok(encoded) => match toml::from_str::<Lock>(&encoded) {
            Ok(lock) => lock,
            Err(err) => {
                eprint!("Failed to parse lockfile; ignoring locked requirements: {err}");
                return Ok(Vec::new());
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(err) => return Err(err.into()),
    };

    // Map each entry in the lockfile to a preference.
    let preferences: Vec<Preference> = lock
        .distributions()
        .iter()
        .map(Preference::from_lock)
        .collect();

    // Apply the upgrade strategy to the requirements.
    Ok(match upgrade {
        // Respect all pinned versions from the existing lockfile.
        Upgrade::None => preferences,
        // Ignore all pinned versions from the existing lockfile.
        Upgrade::All => vec![],
        // Ignore pinned versions for the specified packages.
        Upgrade::Packages(packages) => preferences
            .into_iter()
            .filter(|preference| !packages.contains(preference.name()))
            .collect(),
    })
}
