use std::collections::BTreeSet;
use std::path::Path;

use uv_configuration::{
    NormalizedConstraints, NormalizedOverrideEntries, NormalizedRequirements, Override,
    PackageOverride,
};
use uv_distribution_types::{
    IndexMetadata, IndexUrl, NameRequirementSpecification, Requirement, RequirementScope,
    RequirementSource, RequiresPython, StaticMetadata,
};
use uv_fs::normalize_path;
use uv_git_types::GitUrl;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitDirectoryUrl, ParsedGitPathUrl};
use uv_redacted::DisplaySafeUrl;

use super::{LockError, LockErrorKind};

/// Compare build constraints without requiring equivalent markers to share a representation.
pub(super) fn same_build_constraints(
    left: &BTreeSet<NameRequirementSpecification>,
    right: &BTreeSet<NameRequirementSpecification>,
) -> bool {
    same_entries(left, right, |left, right| {
        if !left
            .requirement
            .marker
            .is_equivalent(right.requirement.marker)
        {
            return false;
        }
        let mut left = left.clone();
        left.requirement.marker = right.requirement.marker;
        left == *right
    })
}

/// Compare static metadata while allowing equivalent dependency marker representations.
pub(super) fn same_static_metadata(
    left: &BTreeSet<StaticMetadata>,
    right: &BTreeSet<StaticMetadata>,
) -> bool {
    same_entries(left, right, |left, right| {
        if left.requires_dist.len() != right.requires_dist.len() {
            return false;
        }
        let mut left = left.clone();
        for (left, right) in left.requires_dist.iter_mut().zip(&right.requires_dist) {
            if !left.marker.is_equivalent(right.marker) {
                return false;
            }
            left.marker = right.marker;
        }
        left == *right
    })
}

/// Compare sets by semantic membership without changing their structural equality or ordering.
fn same_entries<T: Ord>(
    left: &BTreeSet<T>,
    right: &BTreeSet<T>,
    equivalent: impl Fn(&T, &T) -> bool,
) -> bool {
    left == right
        || (left
            .iter()
            .all(|left| right.contains(left) || right.iter().any(|right| equivalent(left, right)))
            && right.iter().all(|right| {
                left.contains(right) || left.iter().any(|left| equivalent(left, right))
            }))
}

/// Prepare dependency inputs for comparison with a lockfile's paths and supported Python versions.
///
/// Source and marker normalization precede collection normalization on both the stored declarations
/// and the current inputs. This also permits semantic comparison with older, unnormalized locks.
pub(super) struct RequirementNormalizer<'a> {
    root: &'a Path,
    requires_python: &'a RequiresPython,
}

impl<'a> RequirementNormalizer<'a> {
    pub(super) fn new(root: &'a Path, requires_python: &'a RequiresPython) -> Self {
        Self {
            root,
            requires_python,
        }
    }

    pub(super) fn requirements(
        &self,
        requirements: impl IntoIterator<Item = Requirement>,
    ) -> Result<NormalizedRequirements, LockError> {
        self.declarations(requirements)
            .map(NormalizedRequirements::from)
    }

    pub(super) fn constraints(
        &self,
        constraints: impl IntoIterator<Item = Requirement>,
    ) -> Result<NormalizedConstraints, LockError> {
        self.declarations(constraints)
            .map(NormalizedConstraints::from)
    }

    /// Normalize each override within its package scope, retaining empty scopes that shadow others.
    pub(super) fn overrides(
        &self,
        overrides: impl IntoIterator<Item = Override<Requirement>>,
    ) -> Result<NormalizedOverrideEntries, LockError> {
        overrides
            .into_iter()
            .map(|entry| match entry {
                Override::Requirement(requirement) => Ok(Override::Requirement(
                    normalize_requirement(requirement, self.root, self.requires_python)?,
                )),
                Override::Package(package) => Ok(Override::Package(PackageOverride {
                    package: package.package,
                    dependencies: self.declarations(package.dependencies)?.into_boxed_slice(),
                })),
            })
            .collect::<Result<Vec<_>, LockError>>()
            .map(NormalizedOverrideEntries::from)
    }

    fn declarations(
        &self,
        requirements: impl IntoIterator<Item = Requirement>,
    ) -> Result<Vec<Requirement>, LockError> {
        requirements
            .into_iter()
            .map(|requirement| normalize_requirement(requirement, self.root, self.requires_python))
            .collect()
    }
}

/// Prepare declarations for serialization, combining equivalent declarations in preview mode.
pub(super) fn normalize_collection<T: Ord, N: From<Vec<T>> + IntoIterator<Item = T>>(
    declarations: impl IntoIterator<Item = T>,
    normalize: bool,
) -> BTreeSet<T> {
    if normalize {
        N::from(declarations.into_iter().collect())
            .into_iter()
            .collect()
    } else {
        declarations.into_iter().collect()
    }
}

/// Put a [`Requirement`] from a lockfile or current configuration into a comparable form.
///
/// Resolve local paths against the workspace root, strip credentials and origin metadata, and
/// simplify markers against [`RequiresPython`]. Clear the dependency-group scope, which is not
/// serialized in the lockfile; callers compare dependency groups and package overrides separately.
///
/// Version constraints are combined separately by the collection normalizers.
pub(super) fn normalize_requirement(
    mut requirement: Requirement,
    root: &Path,
    requires_python: &RequiresPython,
) -> Result<Requirement, LockError> {
    // Sort the extras and groups for consistency.
    requirement.extras.sort();
    requirement.groups.sort();
    requirement.marker = requires_python.simplify_markers(requirement.marker);
    requirement.scope = RequirementScope::Global;
    requirement.origin = None;

    // Normalize the requirement source.
    requirement.source = match requirement.source {
        RequirementSource::GitDirectory {
            git,
            subdirectory,
            url: _,
        } => {
            // Reconstruct the Git URL.
            let git = {
                let mut repository = git.url().clone();

                // Remove the credentials.
                repository.remove_credentials();

                // Remove the fragment and query from the URL; they're already present in the source.
                repository.set_fragment(None);
                repository.set_query(None);

                GitUrl::from_fields(
                    repository,
                    git.reference().clone(),
                    git.precise(),
                    git.lfs(),
                )?
            };

            // Reconstruct the PEP 508 URL from the underlying data.
            let url = DisplaySafeUrl::from(ParsedGitDirectoryUrl {
                url: git.clone(),
                subdirectory: subdirectory.clone(),
            });

            RequirementSource::GitDirectory {
                git,
                subdirectory,
                url: VerbatimUrl::from_url(url),
            }
        }
        RequirementSource::GitPath {
            git,
            install_path,
            ext,
            url: _,
        } => {
            // Reconstruct the Git URL.
            let git = {
                let mut repository = git.url().clone();

                // Remove the credentials.
                repository.remove_credentials();

                // Remove the fragment and query from the URL; they're already present in the source.
                repository.set_fragment(None);
                repository.set_query(None);

                GitUrl::from_fields(
                    repository,
                    git.reference().clone(),
                    git.precise(),
                    git.lfs(),
                )?
            };

            // Reconstruct the PEP 508 URL from the underlying data.
            let url = DisplaySafeUrl::from(ParsedGitPathUrl {
                url: git.clone(),
                install_path: install_path.clone(),
                ext,
            });

            RequirementSource::GitPath {
                git,
                install_path,
                ext,
                url: VerbatimUrl::from_url(url),
            }
        }
        RequirementSource::Path {
            install_path,
            ext,
            url: _,
        } => {
            let path = root.join(&install_path);
            let install_path = normalize_path(path).into_owned().into_boxed_path();
            let url = VerbatimUrl::from_normalized_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            RequirementSource::Path {
                install_path,
                ext,
                url,
            }
        }
        RequirementSource::Directory {
            install_path,
            editable,
            r#virtual,
            url: _,
        } => {
            let path = root.join(&install_path);
            let install_path = normalize_path(path).into_owned().into_boxed_path();
            let url = VerbatimUrl::from_normalized_path(&install_path)
                .map_err(LockErrorKind::RequirementVerbatimUrl)?;

            RequirementSource::Directory {
                install_path,
                editable: Some(editable.unwrap_or(false)),
                r#virtual: Some(r#virtual.unwrap_or(false)),
                url,
            }
        }
        RequirementSource::Registry {
            specifier,
            index,
            conflict,
        } => {
            // Round-trip the index to remove anything apart from the URL.
            let index = index
                .map(|index| index.url.into_url())
                .map(|mut index| {
                    index.remove_credentials();
                    index
                })
                .map(|index| IndexMetadata::from(IndexUrl::from(VerbatimUrl::from_url(index))));
            RequirementSource::Registry {
                specifier,
                index,
                conflict,
            }
        }
        RequirementSource::Url {
            mut location,
            subdirectory,
            ext,
            url: _,
        } => {
            // Remove the credentials.
            location.remove_credentials();

            // Remove the fragment from the URL; it's already present in the source.
            location.set_fragment(None);

            // Reconstruct the PEP 508 URL from the underlying data.
            let url = DisplaySafeUrl::from(ParsedArchiveUrl {
                url: location.clone(),
                subdirectory: subdirectory.clone(),
                ext,
            });

            RequirementSource::Url {
                location,
                subdirectory,
                ext,
                url: VerbatimUrl::from_url(url),
            }
        }
    };
    Ok(requirement)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::error::Error;

    use uv_distribution_types::{NameRequirementSpecification, Requirement};
    use uv_pep508::Requirement as Pep508Requirement;
    use uv_pypi_types::VerbatimParsedUrl;

    use super::same_build_constraints;

    #[test]
    fn build_constraint_marker_equivalence_preserves_hashes() -> Result<(), Box<dyn Error>> {
        let mut left = NameRequirementSpecification {
            requirement: Requirement::from(
                "foo>=1; sys_platform == 'darwin' and platform_release == '24'"
                    .parse::<Pep508Requirement<VerbatimParsedUrl>>()?,
            ),
            hashes: vec!["sha256:1111".to_string()],
        };
        let mut right = left.clone();
        right.requirement.marker =
            "sys_platform == 'darwin' and platform_release >= '24' and platform_release <= '24'"
                .parse()?;
        assert!(same_build_constraints(
            &BTreeSet::from([left.clone()]),
            &BTreeSet::from([right.clone()]),
        ));
        left.hashes = vec!["sha256:2222".to_string()];
        assert!(!same_build_constraints(
            &BTreeSet::from([left]),
            &BTreeSet::from([right]),
        ));
        Ok(())
    }
}
