use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use futures::{StreamExt, stream};

use uv_distribution::{ArchiveMetadata, Metadata};
use uv_distribution_filename::{VariantLabel, WheelFilename};
use uv_distribution_types::{
    BuiltDist, Dist, Name, RegistryBuiltDist, RegistryBuiltWheel, Requirement, SourceDist,
};
use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};

use crate::ResolveError;
use crate::resolver::index::ParsedVariantsJson;
use crate::resolver::{MetadataResponse, MetadataUnavailable, ResolverProvider};

const MAX_CONCURRENT_METADATA_REQUESTS: usize = 16;

/// Combine the dependencies of the wheels that a universal resolution can install.
pub(super) async fn combine(
    provider: &impl ResolverProvider,
    dist: &Dist,
    variants: &ParsedVariantsJson,
    mut baseline: ArchiveMetadata,
) -> Result<MetadataResponse, ResolveError> {
    let Some(wheels) = registry_wheels(dist) else {
        return Ok(MetadataResponse::Found(baseline));
    };
    // The lock can retain wheels that this index's sidecar never declared. Their dependencies
    // cannot affect the resolution because a frozen install cannot select those variants.
    let index = &variants.artifact.index;
    let wheels = wheels.iter().filter(|wheel| {
        &wheel.index == index
            && wheel
                .filename
                .variant()
                .is_none_or(|label| variants.content.contains_variant(label))
    });
    if !wheels
        .clone()
        .any(|wheel| wheel.filename.variant().is_some())
    {
        return Ok(MetadataResponse::Found(baseline));
    }

    let mut labels = BTreeMap::<&str, (&WheelFilename, BTreeSet<Requirement>)>::new();
    let mut extras = BTreeSet::new();
    let mut pending = stream::iter(wheels.map(|wheel| async move {
        let dist = single_wheel(wheel);
        (wheel, fetch_metadata(provider, &dist).await)
    }))
    .buffered(MAX_CONCURRENT_METADATA_REQUESTS);
    while let Some((wheel, response)) = pending.next().await {
        let metadata = match response? {
            MetadataResponse::Found(metadata) => metadata,
            response => return Ok(response),
        };
        let Metadata {
            requires_dist,
            provides_extra,
            ..
        } = metadata.metadata;
        extras.extend(provides_extra.into_vec());
        let requirements = requires_dist
            .into_vec()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let label = wheel.filename.variant().map_or("", VariantLabel::as_str);
        if let Some((first, previous)) = labels.get(label) {
            if previous != &requirements {
                return Ok(MetadataResponse::Unavailable(
                    MetadataUnavailable::InconsistentMetadata(Arc::new(
                        uv_distribution::Error::VariantWheelDependencies {
                            first: Box::new((*first).clone()),
                            second: Box::new(wheel.filename.clone()),
                        },
                    )),
                ));
            }
        } else {
            labels.insert(label, (&wheel.filename, requirements));
        }
    }

    let selections = labels.len();
    let mut requirements = BTreeMap::<Requirement, Vec<&str>>::new();
    for (label, (_, dependencies)) in labels {
        for requirement in dependencies {
            requirements.entry(requirement).or_default().push(label);
        }
    }
    baseline.metadata.requires_dist = requirements
        .into_iter()
        .map(|(mut requirement, labels)| {
            if labels.len() < selections {
                let marker = labels.into_iter().fold(MarkerTree::FALSE, |marker, label| {
                    marker.or(MarkerTree::expression(MarkerExpression::String {
                        key: MarkerValueString::VariantLabel,
                        operator: MarkerOperator::Equal,
                        value: label.into(),
                    }))
                });
                requirement.marker = requirement.marker.and(marker);
            }
            requirement
        })
        .collect();
    baseline.metadata.provides_extra = extras.into_iter().collect();
    Ok(MetadataResponse::Found(baseline))
}

fn registry_wheels(dist: &Dist) -> Option<&[RegistryBuiltWheel]> {
    match dist {
        Dist::Built(BuiltDist::Registry(registry)) => Some(&registry.wheels),
        Dist::Source(SourceDist::Registry(registry)) => Some(&registry.wheels),
        Dist::Built(_) | Dist::Source(_) => None,
    }
}

fn single_wheel(wheel: &RegistryBuiltWheel) -> Dist {
    Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
        wheels: vec![wheel.clone()],
        best_wheel_index: 0,
        sdist: None,
    }))
}

async fn fetch_metadata(
    provider: &impl ResolverProvider,
    dist: &Dist,
) -> Result<MetadataResponse, ResolveError> {
    let response = provider.get_or_build_wheel_metadata(dist).await?;
    if let MetadataResponse::Found(metadata) = &response
        && metadata.metadata.name != *dist.name()
    {
        return Err(ResolveError::MismatchedPackageName {
            request: "distribution metadata",
            expected: dist.name().clone(),
            actual: metadata.metadata.name.clone(),
        });
    }
    Ok(response)
}
