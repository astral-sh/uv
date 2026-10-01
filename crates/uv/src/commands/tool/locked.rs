use uv_distribution_types::{NameRequirementSpecification, Requirement};

pub(super) fn override_specifications(
    requirements: &[Requirement],
    hashes: &[Vec<String>],
) -> Vec<NameRequirementSpecification> {
    // `resolve_names` returns named requirements first, in their original order.
    requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| NameRequirementSpecification {
            requirement: requirement.clone(),
            hashes: hashes.get(index).cloned().unwrap_or_default(),
        })
        .collect()
}
