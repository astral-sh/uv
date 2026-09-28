use std::path::Path;

use jiff::Timestamp;

use uv_configuration::{ExcludeNewer, ExcludeNewerChange};
use uv_distribution_types::{ExcludeNewerOverride, IndexLocations};
use uv_preview::PreviewFeature;

use super::{Lock, LockError, Package, Source, SourceDist};

impl Lock {
    /// Compare upload cutoffs, using recorded artifacts to validate redundant absolute settings.
    pub fn compare_exclude_newer(
        &self,
        exclude_newer: &ExcludeNewer,
        root: &Path,
        indexes: &IndexLocations,
    ) -> Result<Option<ExcludeNewerChange>, LockError> {
        let change = self.options.exclude_newer.compare(exclude_newer);

        // Avoid reconstructing package indexes and scanning artifacts when no cutoff can apply.
        let has_cutoffs = exclude_newer.global.is_some()
            || exclude_newer.package.values().any(|setting| match setting {
                ExcludeNewerOverride::Enabled(_) => true,
                ExcludeNewerOverride::Disabled => false,
            })
            || indexes.has_exclude_newer();

        if has_cutoffs {
            // Check effective cutoffs even when a package or global setting was omitted.
            // Removing an override can reactivate a stricter index cutoff.
            for package in &self.packages {
                let Some(index) = package.index(root)? else {
                    continue;
                };
                let Some(cutoff) = exclude_newer.exclude_newer_package_for_index(
                    package.name(),
                    indexes.exclude_newer_for(&index),
                ) else {
                    continue;
                };
                if package
                    .upload_times()
                    .flatten()
                    .any(|upload_time| upload_time >= cutoff)
                {
                    return Ok(Some(match change {
                        Some(change) if !change.is_relative_timestamp_change() => change,
                        Some(_) | None => {
                            ExcludeNewerChange::ExcludedArtifact(package.name().clone())
                        }
                    }));
                }
            }
        }

        // A relaxed cutoff already permits every artifact in the existing lock.
        if change.is_none() {
            return Ok(None);
        }
        let mut expected = self.prune_exclude_newer(exclude_newer, root, indexes)?;
        if !uv_preview::is_enabled(PreviewFeature::ResolutionInputs) {
            // Omitted settings remain valid when the preview is disabled. Keep comparing any
            // recorded declarations normally until omission is explicitly enabled.
            if self.options.exclude_newer.global.is_some() {
                expected.global.clone_from(&exclude_newer.global);
            }
            for (name, setting) in &exclude_newer.package {
                if self.options.exclude_newer.package.contains_key(name) {
                    expected.package.insert(name.clone(), setting.clone());
                }
            }
            return Ok(self.options.exclude_newer.compare(&expected));
        }
        let actual = self.prune_exclude_newer(&self.options.exclude_newer, root, indexes)?;
        Ok(actual.compare(&expected))
    }

    /// Omit absolute cutoffs established by the recorded artifacts.
    pub(super) fn without_redundant_exclude_newer(
        mut self,
        root: &Path,
        indexes: &IndexLocations,
    ) -> Result<Self, LockError> {
        self.options.exclude_newer =
            self.prune_exclude_newer(&self.options.exclude_newer, root, indexes)?;
        Ok(self)
    }

    /// Retain cutoffs whose effects cannot be established without resolution.
    fn prune_exclude_newer(
        &self,
        exclude_newer: &ExcludeNewer,
        root: &Path,
        indexes: &IndexLocations,
    ) -> Result<ExcludeNewer, LockError> {
        // Build dependencies can affect runtime metadata but are absent from the lock.
        // A virtual project and dependencies distributed only as wheels need no builds.
        if self
            .packages
            .iter()
            .any(|package| match &package.id.source {
                Source::Registry(_) | Source::Direct(..) | Source::Path(_) => {
                    package.sdist.is_some() || package.wheels.is_empty()
                }
                Source::Git(..) | Source::Directory(_) | Source::Editable(_) => true,
                Source::Virtual(_) => false,
            })
        {
            return Ok(exclude_newer.clone());
        }

        let mut retained = exclude_newer.clone();
        for (name, setting) in &exclude_newer.package {
            let ExcludeNewerOverride::Enabled(value) = setting else {
                continue;
            };
            if value.span().is_some() {
                continue;
            }
            let mut redundant = true;
            for package in self.packages_for_name(name) {
                if !package.satisfies_cutoff(value.timestamp()) {
                    redundant = false;
                    break;
                }
                // Retain exceptions that permit artifacts forbidden by the inherited cutoff.
                let index = package.index(root)?;
                let inherited = index
                    .as_ref()
                    .and_then(|index| indexes.exclude_newer_for(index));
                let inherited = match inherited {
                    Some(ExcludeNewerOverride::Enabled(value)) => Some(value.as_ref()),
                    Some(ExcludeNewerOverride::Disabled) => None,
                    None => exclude_newer.global.as_ref(),
                };
                if inherited.is_some_and(|value| {
                    value.span().is_some() || !package.satisfies_cutoff(value.timestamp())
                }) {
                    redundant = false;
                    break;
                }
            }
            if redundant {
                retained.package.remove(name);
            }
        }

        if exclude_newer.global.as_ref().is_some_and(|value| {
            value.span().is_none()
                && self
                    .packages
                    .iter()
                    .all(|package| package.satisfies_cutoff(value.timestamp()))
        }) {
            retained.global = None;
        }
        Ok(retained)
    }
}

impl Package {
    /// Return the upload time of every artifact, including missing timestamps.
    fn upload_times(&self) -> impl Iterator<Item = Option<Timestamp>> {
        self.sdist
            .iter()
            .map(SourceDist::upload_time)
            .chain(self.wheels.iter().map(|wheel| wheel.upload_time))
    }

    /// Check a fixed cutoff without reconstructing candidate or prerelease eligibility.
    fn satisfies_cutoff(&self, cutoff: Timestamp) -> bool {
        match &self.id.source {
            Source::Registry(_) => {}
            Source::Git(..)
            | Source::Direct(..)
            | Source::Path(_)
            | Source::Directory(_)
            | Source::Editable(_)
            | Source::Virtual(_) => return true,
        }
        self.id
            .version
            .as_ref()
            .is_some_and(|version| !version.any_prerelease())
            && (self.sdist.is_some() || !self.wheels.is_empty())
            && self
                .upload_times()
                .all(|upload_time| upload_time.is_some_and(|upload_time| upload_time < cutoff))
    }
}
