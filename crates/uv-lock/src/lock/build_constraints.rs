use std::collections::BTreeSet;
use std::str::FromStr;

use rustc_hash::FxHashMap;
use uv_distribution_types::{NameRequirementSpecification, RequirementSource, VersionId};
use uv_pep508::{MarkerTree, VerbatimUrl};
use uv_pypi_types::{HashDigest, HashDigests};
use uv_types::{HashStrategy, HashStrategyError};

/// Preserve effective hash precedence in a sorted lockfile.
///
/// A hash list applies only where no later hash list for the same identity applies. Keep the
/// original version restriction separately when narrowing its marker, so readers that sort the
/// declarations still apply the same hashes without losing any version constraints.
pub(super) fn canonicalize(
    constraints: impl IntoIterator<Item = NameRequirementSpecification>,
) -> Result<BTreeSet<NameRequirementSpecification>, HashStrategyError> {
    let constraints = constraints
        .into_iter()
        .map(materialize_hashes)
        .collect::<Result<Vec<_>, _>>()?;
    let mut later_markers = FxHashMap::<VersionId, MarkerTree>::default();
    let mut result = BTreeSet::new();

    for mut constraint in constraints.into_iter().rev() {
        if constraint.hashes.is_empty() {
            result.insert(constraint);
            continue;
        }
        let Some(id) = HashStrategy::pin(&constraint.requirement) else {
            result.insert(constraint);
            continue;
        };
        match &id {
            // Remote archive hashes accumulate instead of using last-wins precedence.
            VersionId::ArchiveUrl { .. } => {
                result.insert(constraint);
                continue;
            }
            VersionId::NameVersion(..)
            | VersionId::Git { .. }
            | VersionId::Path(..)
            | VersionId::Directory(..)
            | VersionId::Unknown(..) => {}
        }
        let marker = constraint.requirement.marker;
        let later = later_markers.entry(id).or_insert(MarkerTree::FALSE);
        let effective = marker.and(later.negate());
        *later = later.or(marker);

        if effective != marker {
            // The version restriction still applies where a later hash list takes precedence.
            let mut hashless = constraint.clone();
            hashless.hashes.clear();
            result.insert(hashless);
            constraint.requirement.marker = effective;
        }
        if !effective.is_false() {
            result.insert(constraint);
        }
    }
    Ok(result)
}

/// Persist URL-fragment hashes explicitly, since the serialized source omits the fragment.
pub(super) fn materialize_hashes(
    mut constraint: NameRequirementSpecification,
) -> Result<NameRequirementSpecification, HashStrategyError> {
    let Some(hashes) = constraint.requirement.hashes()? else {
        return Ok(constraint);
    };
    let hashes = HashDigests::from(hashes);
    if hashes.is_empty() {
        return Ok(constraint);
    }
    let existing = constraint
        .hashes
        .iter()
        .map(|digest| HashDigest::from_str(digest))
        .collect::<Result<Vec<_>, _>>()?;
    for digest in hashes.iter() {
        match existing
            .iter()
            .find(|candidate| candidate.algorithm() == digest.algorithm())
        {
            Some(candidate) if candidate == digest => {}
            Some(conflict) => {
                return Err(HashStrategyError::ConflictingArchiveUrlHashes(
                    constraint.requirement.to_string(),
                    conflict.clone(),
                    digest.clone(),
                ));
            }
            None => constraint.hashes.push(digest.to_string()),
        }
    }
    if let RequirementSource::Url { url, .. } | RequirementSource::Path { url, .. } =
        &mut constraint.requirement.source
    {
        let prefers_relative = url.prefers_relative();
        let mut unfragmented = url.to_url();
        unfragmented.set_fragment(None);
        *url = VerbatimUrl::from_url(unfragmented.clone())
            .with_given(unfragmented.as_str())
            .with_force_relative(prefers_relative);
    }
    Ok(constraint)
}
