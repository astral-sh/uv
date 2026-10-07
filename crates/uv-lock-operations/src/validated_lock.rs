use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use owo_colors::OwoColorize;
use tracing::debug;
use uv_cache::Refresh;
use uv_command_support::Printer;
use uv_configuration::{Constraints, ExcludeDependency, Override, Upgrade};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{DependencyMetadata, IndexLocations, Requirement, RequiresPython};
use uv_lock::{GroupMetadata, Lock, SatisfiesResult};
use uv_normalize::{DefaultGroups, GroupName, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{Conflicts, SupportedEnvironments};
use uv_python_interpreter::Interpreter;
use uv_resolver::{InMemoryIndex, Options};
use uv_types::HashStrategy;
use uv_warnings::warn_user;
use uv_workspace::{Editability, WorkspaceMember};

use crate::LockValidationError;

/// Whether an existing lockfile can satisfy or guide a new resolution.
#[derive(Debug)]
pub enum ValidatedLock {
    /// An existing lockfile was provided, but its contents should be ignored.
    Unusable(Lock),
    /// An existing lockfile was provided, and the locked versions should be preferred if possible,
    /// though the forks should be ignored.
    Versions(Lock),
    /// An existing lockfile was provided, and the locked versions and forks should be preferred if
    /// possible, even though the lockfile does not satisfy the workspace requirements.
    Preferable(Lock),
    /// An existing lockfile was provided, and it satisfies the workspace requirements.
    Satisfies(Lock),
}

impl ValidatedLock {
    /// Validate a [`Lock`] against its requirements and resolution policy.
    pub async fn validate(
        lock: Lock,
        install_path: &Path,
        packages: &BTreeMap<PackageName, WorkspaceMember>,
        members: &[PackageName],
        required_members: &BTreeMap<PackageName, Editability>,
        requirements: &[Requirement],
        dependency_groups: &BTreeMap<GroupName, Vec<Requirement>>,
        workspace_group_metadata: &BTreeMap<GroupName, GroupMetadata>,
        workspace_default_groups: Option<&DefaultGroups>,
        constraints: &[Requirement],
        overrides: &[Override<Requirement>],
        excludes: &[ExcludeDependency],
        build_constraints: &Constraints,
        conflicts: &Conflicts,
        environments: Option<&SupportedEnvironments>,
        required_environments: Option<&SupportedEnvironments>,
        dependency_metadata: &DependencyMetadata,
        interpreter: &Interpreter,
        requires_python: &RequiresPython,
        index_locations: &IndexLocations,
        upgrade: &Upgrade,
        refresh: Option<&Refresh>,
        options: &Options,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'_, BuildDispatch<'_>>,
        preview: Preview,
        printer: Printer,
    ) -> Result<Self, LockValidationError> {
        // Perform checks in a deliberate order, such that the most extreme conditions are tested
        // first (i.e., every check that returns `Self::Unusable`, followed by every check that
        // returns `Self::Versions`, followed by every check that returns `Self::Preferable`, and
        // finally `Self::Satisfies`).
        if lock.resolution_mode() != options.resolution_mode {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in resolution mode: `{}` vs. `{}`",
                lock.resolution_mode().cyan(),
                options.resolution_mode.cyan()
            );
            return Ok(Self::Unusable(lock));
        }
        if lock.fork_strategy() != options.fork_strategy {
            let _ = writeln!(
                printer.stderr(),
                "Ignoring existing lockfile due to change in fork strategy: `{}` vs. `{}`",
                lock.fork_strategy().cyan(),
                options.fork_strategy.cyan()
            );
            return Ok(Self::Unusable(lock));
        }
        // Stored cutoffs can belong to packages considered during backtracking. New cutoffs for
        // packages outside the lock take effect when another change triggers resolution.
        let exclude_newer = lock.filter_exclude_newer(options.exclude_newer.clone());
        if let Some(change) = lock.exclude_newer().compare(&exclude_newer) {
            // If a relative value is used, we won't invalidate on every tick of the clock unless
            // the span duration changed or some other operation causes a new resolution
            if !change.is_relative_timestamp_change() {
                let _ = writeln!(
                    printer.stderr(),
                    "Resolving despite existing lockfile due to {change}",
                );
                return Ok(Self::Preferable(lock));
            }
        }

        if upgrade.is_all() {
            // If the user specified `--upgrade`, then we can't use the existing lockfile.
            //
            // If the user is upgrading a subset of packages, we handle it below, after some checks
            // regarding fork markers. In particular, we'd like to return `Preferable` here, but we
            // shouldn't if the fork markers cannot be reused.
            debug!("Ignoring existing lockfile due to `--upgrade`");
            return Ok(Self::Unusable(lock));
        }

        // NOTE: It's important that this appears before any possible path that
        // returns `Self::Preferable`. In particular, if our fork markers are
        // bunk, then we shouldn't return a result that indicates we should try
        // to re-use the existing fork markers.
        if let Err((fork_markers_union, environments_union)) = lock.check_marker_coverage() {
            warn_user!(
                "Resolving despite existing lockfile due to fork markers not covering the supported environments: `{}` vs `{}`",
                fork_markers_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
                environments_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
            );
            return Ok(Self::Versions(lock));
        }

        // NOTE: Similarly as above, this should also appear before any
        // possible code path that can return `Self::Preferable`.
        if let Err((fork_markers_union, requires_python_marker)) =
            lock.requires_python_coverage(requires_python)
        {
            warn_user!(
                "Resolving despite existing lockfile due to fork markers being disjoint with `requires-python`: `{}` vs `{}`",
                fork_markers_union
                    .try_to_string()
                    .unwrap_or("true".to_string()),
                requires_python_marker
                    .try_to_string()
                    .unwrap_or("true".to_string()),
            );
            return Ok(Self::Versions(lock));
        }

        // If the set of supported environments has changed, we have to perform a clean resolution.
        let expected = lock.simplified_supported_environments();
        let actual = environments
            .map(SupportedEnvironments::as_markers)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(|marker| lock.simplify_environment(marker))
            .collect::<Vec<_>>();
        if expected != actual {
            debug!(
                "Resolving despite existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
            );
            return Ok(Self::Versions(lock));
        }

        // If the set of required platforms has changed, we have to perform a clean resolution.
        let expected = lock.simplified_required_environments();
        let actual = required_environments
            .map(SupportedEnvironments::as_markers)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(|marker| lock.simplify_environment(marker))
            .collect::<Vec<_>>();
        if expected != actual {
            debug!(
                "Resolving despite existing lockfile due to change in supported environments: `{:?}` vs. `{:?}`",
                expected, actual
            );
            return Ok(Self::Versions(lock));
        }

        // Different libc requirements can change which versions cover the required platforms.
        if lock.minimum_libc_version() != options.minimum_libc_version {
            debug!(
                "Resolving despite existing lockfile due to change in minimum libc version: {:?} vs. {:?}",
                lock.minimum_libc_version(),
                options.minimum_libc_version,
            );
            return Ok(Self::Versions(lock));
        }

        // If the conflicting group config has changed, we have to perform a clean resolution.
        if conflicts != lock.conflicts() {
            debug!(
                "Resolving despite existing lockfile due to change in conflicting groups: `{:?}` vs. `{:?}`",
                conflicts,
                lock.conflicts(),
            );
            return Ok(Self::Versions(lock));
        }

        // If the Requires-Python bound has changed, we have to perform a clean resolution, since
        // the set of `resolution-markers` may no longer cover the entire supported Python range.
        if lock.requires_python().range() != requires_python.range() {
            debug!(
                "Resolving despite existing lockfile due to change in Python requirement: `{}` vs. `{}`",
                lock.requires_python(),
                requires_python,
            );
            return if lock.fork_markers().is_empty() {
                Ok(Self::Preferable(lock))
            } else {
                Ok(Self::Versions(lock))
            };
        }

        // If the pre-release mode has changed, we have to re-resolve, but can retain the existing
        // versions and forks.
        if lock.prerelease() != &options.prerelease {
            if lock.prerelease_mode() != options.prerelease.global {
                let _ = writeln!(
                    printer.stderr(),
                    "Resolving despite existing lockfile due to change in pre-release mode: `{}` vs. `{}`",
                    lock.prerelease_mode().cyan(),
                    options.prerelease.global.cyan()
                );
            } else {
                debug!(
                    "Resolving despite existing lockfile due to change in package-specific pre-release modes"
                );
            }
            return Ok(Self::Preferable(lock));
        }

        // If the user specified `--upgrade-package` or `--upgrade-group`, then at best we can
        // prefer some of the existing versions.
        if !(upgrade.is_none() || upgrade.is_all()) {
            debug!(
                "Resolving despite existing lockfile due to `--upgrade-package` or `--upgrade-group`"
            );
            return Ok(Self::Preferable(lock));
        }

        if !lock.satisfies_hash_algorithms(install_path, index_locations)? {
            debug!("Resolving despite existing lockfile due to mismatched hash algorithm");
            return Ok(Self::Preferable(lock));
        }

        // If the user specified `--refresh`, then we have to re-resolve.
        if matches!(refresh, Some(Refresh::All(..) | Refresh::Packages(..))) {
            debug!("Resolving despite existing lockfile due to `--refresh`");
            return Ok(Self::Preferable(lock));
        }

        // If the user provided at least one index URL (from the command line, or from a configuration
        // file), don't use the existing lockfile if it references any registries that are no longer
        // included in the current configuration.
        //
        // However, if _no_ indexes were provided, we assume that the user wants to reuse the existing
        // distributions, even though a failure to reuse the lockfile will result in re-resolving
        // against PyPI by default.
        let indexes = if index_locations.is_none() {
            None
        } else {
            Some(index_locations)
        };

        // Determine whether the lockfile satisfies the workspace requirements.
        match lock
            .satisfies(
                install_path,
                packages,
                members,
                required_members,
                requirements,
                constraints,
                overrides,
                excludes,
                build_constraints,
                dependency_groups,
                workspace_group_metadata,
                workspace_default_groups,
                dependency_metadata,
                indexes,
                interpreter.tags()?,
                interpreter.markers(),
                &options.build_options,
                hasher,
                index.distributions(),
                database,
                preview.is_enabled(PreviewFeature::LockWithoutMetadata),
            )
            .await?
        {
            SatisfiesResult::Satisfied => {
                debug!("Existing `uv.lock` satisfies workspace requirements");
                Ok(Self::Satisfies(lock))
            }
            SatisfiesResult::MismatchedMembers(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched members:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedMemberDefaultGroups(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched member default groups:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedWorkspaceGroupMetadata(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched workspace group metadata:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedWorkspaceDefaultGroups(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched workspace default groups:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedMemberGroupMetadata(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched group metadata:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedEditable(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (expected: `editable`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (unexpected: `editable`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVirtual(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (expected: `virtual`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched source: `{name}` (unexpected: `virtual`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDynamic(name, expected) => {
                if expected {
                    debug!(
                        "Resolving despite existing lockfile due to static version: `{name}` (expected a dynamic version)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to dynamic version: `{name}` (expected a static version)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedVersion(name, expected, actual) => {
                if let Some(actual) = actual {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched version: `{name}` (expected: `{expected}`, found: `{actual}`)"
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched version: `{name}` (expected: `{expected}`)"
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedRequirements(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched requirements:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedConstraints(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched constraints:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedOverrides(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched overrides:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedExcludes(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched excludes:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedBuildConstraints(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched build constraints:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedDependencyGroups(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched dependency groups:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedStaticMetadata(expected, actual) => {
                debug!(
                    "Resolving despite existing lockfile due to mismatched static metadata:\n  Requested: {:?}\n  Existing: {:?}",
                    expected, actual
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRoot(name) => {
                debug!("Resolving despite existing lockfile due to missing root package: `{name}`");
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingRemoteIndex(name, version, index) => {
                debug!(
                    "Resolving despite existing lockfile due to missing remote index: `{name}` `{version}` from `{index}`"
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingLocalIndex(name, version, index) => {
                debug!(
                    "Resolving despite existing lockfile due to missing local index: `{name}` `{version}` from `{}`",
                    index.display()
                );
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageRequirements(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched requirements for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched requirements for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageDependencies(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched resolved dependencies for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched resolved dependencies for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageDependencyGroups(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched dependency groups for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched dependency groups for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MismatchedPackageProvidesExtra(name, version, expected, actual) => {
                if let Some(version) = version {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched extras for: `{name}=={version}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                } else {
                    debug!(
                        "Resolving despite existing lockfile due to mismatched extras for: `{name}`\n  Requested: {:?}\n  Existing: {:?}",
                        expected, actual
                    );
                }
                Ok(Self::Preferable(lock))
            }
            SatisfiesResult::MissingVersion(name) => {
                debug!("Resolving despite existing lockfile due to missing version: `{name}`");
                Ok(Self::Preferable(lock))
            }
        }
    }

    /// Return whether the existing lock satisfies the current inputs.
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfies(_))
    }

    /// Return whether the existing lock can provide version preferences.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        !matches!(self, Self::Unusable(_))
    }

    /// Convert the [`ValidatedLock`] into a [`Lock`].
    #[must_use]
    pub fn into_lock(self) -> Lock {
        match self {
            Self::Unusable(lock) => lock,
            Self::Satisfies(lock) => lock,
            Self::Preferable(lock) => lock,
            Self::Versions(lock) => lock,
        }
    }
}
