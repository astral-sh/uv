use std::path::Path;

use anyhow::Result;

use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::Upgrade;
use uv_fs::CWD;
use uv_git::ResolvedRepositoryReference;
use uv_requirements_txt::RequirementsTxt;
use uv_resolver::{Lock, LockError, Preference, PreferenceError};

#[derive(Debug, Default)]
pub struct LockedRequirements {
    /// The pinned versions from the lockfile.
    pub preferences: Vec<Preference>,
    /// The pinned Git SHAs from the lockfile.
    pub git: Vec<ResolvedRepositoryReference>,
}

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
        &*CWD,
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
            .filter(|preference| !packages.contains_key(preference.name()))
            .collect(),
    })
}

/// Load the preferred requirements from an existing lockfile, applying the upgrade strategy.
pub fn read_lock_requirements(
    lock: &Lock,
    install_path: &Path,
    upgrade: &Upgrade,
) -> Result<LockedRequirements, LockError> {
    // As an optimization, skip iterating over the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(LockedRequirements::default());
    }

    let mut preferences = Vec::new();
    let mut git = Vec::new();

    for package in lock.packages() {
        // Skip the distribution if it's not included in the upgrade strategy.
        if upgrade.contains(package.name()) {
            continue;
        }

        // Map each entry in the lockfile to a preference.
        if let Some(preference) = Preference::from_lock(package, install_path)? {
            preferences.push(preference);
        }

        // Map each entry in the lockfile to a Git SHA.
        if let Some(git_ref) = package.as_git_ref()? {
            git.push(git_ref);
        }
    }

    Ok(LockedRequirements { preferences, git })
}
