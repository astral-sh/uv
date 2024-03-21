use std::path::Path;

use anyhow::Result;
use rustc_hash::FxHashSet;

use requirements_txt::RequirementsTxt;
use uv_client::Connectivity;
use uv_normalize::PackageName;
use uv_resolver::{Preference, PreferenceError};

/// Whether to allow package upgrades.
#[derive(Debug)]
pub enum Upgrade {
    /// Prefer pinned versions from the existing lockfile, if possible.
    None,

    /// Allow package upgrades for all packages, ignoring the existing lockfile.
    All,

    /// Allow package upgrades, but only for the specified packages.
    Packages(FxHashSet<PackageName>),
}

impl Upgrade {
    /// Determine the upgrade strategy from the command-line arguments.
    pub fn from_args(upgrade: bool, upgrade_package: Vec<PackageName>) -> Self {
        if upgrade {
            Self::All
        } else if !upgrade_package.is_empty() {
            Self::Packages(upgrade_package.into_iter().collect())
        } else {
            Self::None
        }
    }

    /// Returns `true` if no packages should be upgraded.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if all packages should be upgraded.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}

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
    let requirements_txt =
        RequirementsTxt::parse(output_file, std::env::current_dir()?, Connectivity::Offline)
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
