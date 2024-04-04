use std::str::FromStr;

use rustc_hash::{FxHashMap, FxHashSet};

use pep508_rs::{MarkerEnvironment, RequirementsTxtRequirement};
use pypi_types::{HashError, Hashes};
use requirements_txt::RequirementEntry;
use uv_normalize::PackageName;

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct RequiredHashes(FxHashMap<PackageName, FxHashSet<Hashes>>);

impl RequiredHashes {
    /// Generate the [`RequiredHashes`] from a set of requirement entries.
    pub fn from_entries(
        entries: &[RequirementEntry],
        markers: &MarkerEnvironment,
    ) -> Result<Self, RequiredHashesError> {
        let mut allowed_hashes = FxHashMap::<PackageName, FxHashSet<Hashes>>::default();

        // For each requirement, map from name to allowed hashes. We use the last entry for each
        // package.
        //
        // For now, unnamed requirements are unsupported. This should be fine, since `--require-hashes`
        // tends to be used after `pip-compile`, which will always output named requirements.
        //
        // TODO(charlie): Preserve hashes from `requirements.txt` through to this pass, so that we
        // can iterate over requirements directly, rather than iterating over the entries.
        for entry in entries
            .iter()
            .filter(|entry| entry.requirement.evaluate_markers(markers, &[]))
        {
            // Extract the requirement name.
            let name = match &entry.requirement {
                RequirementsTxtRequirement::Pep508(requirement) => requirement.name.clone(),
                RequirementsTxtRequirement::Unnamed(_) => {
                    return Err(RequiredHashesError::UnnamedRequirement)
                }
            };

            // Parse the hashes.
            let hashes = entry
                .hashes
                .iter()
                .map(|hash| Hashes::from_str(hash))
                .collect::<Result<FxHashSet<_>, _>>()
                .unwrap();

            // TODO(charlie): Extract hashes from URL fragments.
            allowed_hashes.insert(name, hashes);
        }

        Ok(Self(allowed_hashes))
    }

    /// Returns versions for the given package which are allowed even if marked as yanked by the
    /// relevant index.
    pub fn get(&self, package_name: &PackageName) -> Option<&FxHashSet<Hashes>> {
        self.0.get(package_name)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequiredHashesError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error("Unnamed requirements are not supported in `--require-hashes`")]
    UnnamedRequirement,
}
