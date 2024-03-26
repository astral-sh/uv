use std::path::Path;

use anyhow::Result;

use requirements_txt::RequirementsTxt;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_resolver::{Preference, PreferenceError};
use uv_types::Upgrade;

/// Load the preferred requirements from an existing lockfile, applying the upgrade strategy.
pub async fn read_lockfile(
    output_file: Option<&Path>,
    upgrade: Upgrade,
) -> Result<Vec<Preference>> {
    // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
    let Some(output_file) = output_file
        .filter(|_| !upgrade.is_all())
        .filter(|output_file| output_file.exists())
    else {
        return Ok(Vec::new());
    };

    // Parse the requirements from the lockfile.
    let requirements_txt = RequirementsTxt::parse(
        output_file,
        std::env::current_dir()?,
        &BaseClientBuilder::new().connectivity(Connectivity::Offline),
    )
    .await?;
    let preferences = requirements_txt
        .requirements
        .into_iter()
        .filter(|entry| !entry.editable)
        .map(Preference::from_entry)
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
