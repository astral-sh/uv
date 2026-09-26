use std::path::Path;

use anyhow::Result;
use tracing::info_span;

use uv_configuration::Upgrade;
use uv_distribution_types::IndexUrl;
use uv_fs::CWD;
use uv_git::ResolvedRepositoryReference;
use uv_lock::{Lock, LockError, PylockToml, PylockTomlErrorKind};
use uv_pep508::VerbatimUrl;
use uv_requirements_txt::RequirementsTxt;
use uv_resolver::{Preference, PreferenceError, UpgradePackages};

#[derive(Debug, Default)]
pub(crate) struct LockedRequirements {
    /// The pinned versions from the lockfile.
    pub(crate) preferences: Vec<Preference>,
    /// The pinned Git SHAs from the lockfile.
    pub(crate) git: Vec<ResolvedRepositoryReference>,
}

impl LockedRequirements {
    /// Create a [`LockedRequirements`] from a list of preferences.
    pub(crate) fn from_preferences(preferences: Vec<Preference>) -> Self {
        Self {
            preferences,
            ..Self::default()
        }
    }
}

/// Load the preferred requirements from an existing `requirements.txt`, applying the upgrade strategy.
pub(crate) async fn read_requirements_txt(
    output_file: &Path,
    upgrade: &Upgrade,
) -> Result<Vec<Preference>> {
    // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(Vec::new());
    }

    // Parse the requirements from the lockfile.
    let requirements_txt = RequirementsTxt::parse(output_file, &*CWD).await?;

    // Map each entry in the lockfile to a preference.
    let preferences = requirements_txt
        .requirements
        .into_iter()
        .map(Preference::from_entry)
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, PreferenceError>>()?;

    // Apply the upgrade strategy to the requirements.
    let upgrade_packages = UpgradePackages::for_non_project(upgrade);

    Ok(if upgrade.is_none() {
        // Respect all pinned versions from the existing lockfile.
        preferences
    } else {
        // Ignore all pinned versions for packages that should be upgraded.
        preferences
            .into_iter()
            .filter(|preference| !upgrade_packages.contains(preference.name()))
            .collect()
    })
}

/// Load the preferred requirements from an existing lockfile, applying the upgrade strategy.
pub(crate) fn read_lock_requirements(
    lock: &Lock,
    install_path: &Path,
    upgrade: &Upgrade,
) -> Result<LockedRequirements, LockError> {
    // As an optimization, skip iterating over the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(LockedRequirements::default());
    }

    // Resolve the full set of packages to upgrade, combining `--upgrade-package` and
    // `--upgrade-group`.
    let mut upgrade_packages = upgrade.packages().cloned().unwrap_or_default();
    if upgrade.packages().is_some()
        && let Some(groups) = upgrade.groups()
    {
        // Check package-level dependency groups (the standard case for projects with
        // a `[project]` table).
        for package in lock.packages() {
            for (group_name, dependencies) in package.resolved_dependency_groups() {
                if groups.contains(group_name) {
                    for dependency in dependencies {
                        upgrade_packages.insert(dependency.package_name().clone());
                    }
                }
            }
        }

        // Check manifest-level dependency groups, which cover projects without a
        // `[project]` table (e.g., virtual workspace roots or PEP 723 scripts).
        for (group_name, requirements) in lock.dependency_groups() {
            if groups.contains(group_name) {
                for requirement in requirements {
                    upgrade_packages.insert(requirement.name.clone());
                }
            }
        }
    }

    let mut preferences = Vec::new();
    let mut git = Vec::new();

    for package in lock.packages() {
        // Skip the distribution if it's included in the upgrade strategy (either by explicit
        // package name or via a dependency group).
        if upgrade_packages.contains(package.name()) {
            continue;
        }

        // Map each entry in the lockfile to a preference.
        if let Some(version) = package.version() {
            preferences.push(Preference::from_locked(
                package.name().clone(),
                version.clone(),
                package.index(install_path)?,
                package.fork_markers().to_vec(),
            ));
        }

        // Map each entry in the lockfile to a Git SHA.
        if let Some(git_ref) = package.as_git_ref()? {
            git.push(git_ref);
        }
    }

    Ok(LockedRequirements { preferences, git })
}

/// Load the preferred requirements from an existing `pylock.toml` file, applying the upgrade strategy.
pub(crate) async fn read_pylock_toml_requirements(
    output_file: &Path,
    upgrade: &Upgrade,
) -> Result<LockedRequirements, PylockTomlErrorKind> {
    // As an optimization, skip iterating over the lockfile is we're upgrading all packages anyway.
    if upgrade.is_all() {
        return Ok(LockedRequirements::default());
    }

    // Read the `pylock.toml` from disk, and deserialize it from TOML.
    let content = fs_err::tokio::read_to_string(&output_file).await?;
    let lock = info_span!("toml::from_str upgrade", path = %output_file.display())
        .in_scope(|| PylockToml::parse(&content))?;

    let upgrade_packages = UpgradePackages::for_non_project(upgrade);

    let mut preferences = Vec::new();
    let mut git = Vec::new();

    for package in &lock.packages {
        // Skip the distribution if it's not included in the upgrade strategy.
        if upgrade_packages.contains(&package.name) {
            continue;
        }

        // Map each entry in the lockfile to a preference.
        if let Some(version) = package.version.as_ref() {
            preferences.push(Preference::from_locked(
                package.name.clone(),
                version.clone(),
                package
                    .index
                    .as_ref()
                    .map(|index| IndexUrl::from(VerbatimUrl::from(index.clone()))),
                vec![],
            ));
        }

        // Map each entry in the lockfile to a Git SHA.
        if let Some(git_ref) = package.as_git_ref()? {
            git.push(git_ref);
        }
    }

    Ok(LockedRequirements { preferences, git })
}
