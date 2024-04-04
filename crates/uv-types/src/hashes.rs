use std::str::FromStr;

use rustc_hash::{FxHashMap, FxHashSet};

use pep508_rs::{MarkerEnvironment, Requirement};
use pypi_types::{HashError, Hashes};
use uv_normalize::PackageName;

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct RequiredHashes(FxHashMap<PackageName, FxHashSet<Hashes>>);

impl RequiredHashes {
    /// Generate the [`RequiredHashes`] from a set of requirement entries.
    pub fn from_requirements(
        requirements: impl Iterator<Item = (Requirement, Vec<String>)>,
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
        for (requirement, hashes) in requirements {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            // Parse the hashes.
            let hashes = hashes
                .iter()
                .map(|hash| Hashes::from_str(hash))
                .collect::<Result<FxHashSet<_>, _>>()
                .unwrap();

            // TODO(charlie): Extract hashes from URL fragments.
            allowed_hashes.insert(requirement.name, hashes);
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
