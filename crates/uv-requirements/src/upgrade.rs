use std::path::Path;

use anstream::eprint;
use anyhow::Result;

use requirements_txt::RequirementsTxt;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::Upgrade;
use uv_distribution::Workspace;
use uv_git::ResolvedRepositoryReference;
use uv_resolver::{Lock, Preference, PreferenceError};

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
pub async fn read_lockfile(workspace: &Workspace, upgrade: &Upgrade) -> Result<LockedRequirements> {
    // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(LockedRequirements::default());
    }

    // If an existing lockfile exists, build up a set of preferences.
    let lockfile = workspace.root().join("uv.lock");
    let lock = match fs_err::tokio::read_to_string(&lockfile).await {
        Ok(encoded) => match toml::from_str::<Lock>(&encoded) {
            Ok(lock) => lock,
            Err(err) => {
                eprint!("Failed to parse lockfile; ignoring locked requirements: {err}");
                return Ok(LockedRequirements::default());
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LockedRequirements::default());
        }
        Err(err) => return Err(err.into()),
    };

    let mut preferences = Vec::new();
    let mut git = Vec::new();

    for dist in lock.distributions() {
        // Skip the distribution if it's not included in the upgrade strategy.
        if match upgrade {
            Upgrade::None => false,
            Upgrade::All => true,
            Upgrade::Packages(packages) => packages.contains(dist.name()),
        } {
            continue;
        }

        // Map each entry in the lockfile to a preference.
        preferences.push(Preference::from_lock(dist));

        // Map each entry in the lockfile to a Git SHA.
        if let Some(git_ref) = dist.as_git_ref() {
            git.push(git_ref);
        }
    }

    Ok(LockedRequirements { preferences, git })
}
