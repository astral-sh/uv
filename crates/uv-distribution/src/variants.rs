use std::hash::BuildHasherDefault;
use std::sync::Arc;

use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHasher};
use tracing::{debug, trace};
use uv_distribution_filename::BuildTag;
use uv_distribution_types::{
    BuiltDist, Dist, DistributionId, GlobalVersionId, Identifier, Name, Node, RegistryBuiltDist,
    RegistryVariantsJson, Resolution, ResolvedDist,
};
use uv_once_map::OnceMap;
use uv_platform_tags::{TagCompatibility, Tags};
use uv_pypi_types::ResolverMarkerEnvironment;
use uv_types::BuildContext;
use uv_variants::resolved_variants::ResolvedVariants;

use crate::{DistributionDatabase, Error};

type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

/// An in-memory cache from package to resolved variants.
#[derive(Default)]
pub struct PackageVariantCache(FxOnceMap<GlobalVersionId, Arc<ResolvedVariants>>);

impl std::ops::Deref for PackageVariantCache {
    type Target = FxOnceMap<GlobalVersionId, Arc<ResolvedVariants>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Resolve all variants for the given resolution.
pub async fn resolve_variants<Context: BuildContext>(
    resolution: Resolution,
    marker_env: &ResolverMarkerEnvironment,
    distribution_database: DistributionDatabase<'_, Context>,
    cache: &PackageVariantCache,
    tags: &Tags,
) -> Result<Resolution, Error> {
    // Fetch variants.json and then query providers, running in parallel for all distributions.
    let dist_resolved_variants: FxHashMap<GlobalVersionId, Arc<ResolvedVariants>> =
        futures::stream::iter(
            resolution
                .graph()
                .node_weights()
                .filter_map(|node| extract_variants(node)),
        )
        .map(async |(variants_json, ..)| {
            let id = variants_json.version_id();

            let resolved_variants = if cache.register(id.clone()) {
                let resolved_variants = distribution_database
                    .fetch_and_query_variants(variants_json, marker_env)
                    .await?;

                let resolved_variants = Arc::new(resolved_variants);
                cache.done(id.clone(), resolved_variants.clone());
                resolved_variants
            } else {
                cache
                    .wait(&id)
                    .await
                    .expect("missing value for registered task")
            };

            Ok::<_, Error>((id, resolved_variants))
        })
        // TODO(konsti): Buffer size
        .buffered(8)
        .try_collect()
        .await?;

    // Determine modification to the resolutions to select variant wheels, or error if there
    // is no matching variant wheel and no matching non-variant wheel.
    let mut new_best_wheel_index: FxHashMap<DistributionId, usize> = FxHashMap::default();
    for node in resolution.graph().node_weights() {
        let Some((json, dist)) = extract_variants(node) else {
            continue;
        };
        let resolved_variants = &dist_resolved_variants[&json.version_id()];

        // Select best wheel
        let mut highest_priority_variant_wheel: Option<(
            usize,
            Vec<usize>,
            TagCompatibility,
            Option<&BuildTag>,
        )> = None;
        for (wheel_index, wheel) in dist.wheels.iter().enumerate() {
            let build_tag = wheel.filename.build_tag();
            let compatibility = wheel.filename.compatibility(tags);
            if !compatibility.is_compatible() {
                continue;
            }

            let Some(variant) = wheel.filename.variant() else {
                // The non-variant wheel is already supported
                continue;
            };

            let Some(scores) = resolved_variants.score_variant(variant) else {
                continue;
            };

            if let Some((_, old_scores, old_compatibility, old_build_tag)) =
                &highest_priority_variant_wheel
            {
                if (&scores, &compatibility, &build_tag)
                    > (old_scores, old_compatibility, old_build_tag)
                {
                    highest_priority_variant_wheel =
                        Some((wheel_index, scores, compatibility, build_tag));
                }
            } else {
                highest_priority_variant_wheel =
                    Some((wheel_index, scores, compatibility, build_tag));
            }
        }

        // Determine if we need to modify the resolution
        if let Some((wheel_index, ..)) = highest_priority_variant_wheel {
            debug!(
                "{} for {}: {}",
                "Using variant wheel".red(),
                dist.name(),
                dist.wheels[wheel_index].filename,
            );
            new_best_wheel_index.insert(dist.distribution_id(), wheel_index);
        } else if dist.best_wheel().filename.variant().is_some() {
            return Err(Error::WheelVariantMismatch {
                name: dist.name().clone(),
                variants: dist
                    .wheels
                    .iter()
                    .filter_map(|wheel| wheel.filename.variant())
                    .join(", "),
            });
        } else {
            trace!(
                "No matching variant wheel, but matching non-variant wheel for {}",
                dist.name()
            );
        }
    }
    let resolution = resolution.map(|dist| {
        let ResolvedDist::Installable {
            dist,
            version,
            variants_json,
        } = dist
        else {
            return None;
        };
        let Dist::Built(BuiltDist::Registry(dist)) = &**dist else {
            return None;
        };
        // Check whether there is a matching variant wheel we want to use instead of the default.
        let best_wheel_index = new_best_wheel_index.get(&dist.distribution_id())?;
        Some(ResolvedDist::Installable {
            dist: Arc::new(Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                wheels: dist.wheels.clone(),
                best_wheel_index: *best_wheel_index,
                sdist: dist.sdist.clone(),
            }))),
            variants_json: variants_json.clone(),
            version: version.clone(),
        })
    });

    Ok(resolution)
}

fn extract_variants(node: &Node) -> Option<(&RegistryVariantsJson, &RegistryBuiltDist)> {
    let Node::Dist { dist, .. } = node else {
        // The root node has no variants
        return None;
    };
    let ResolvedDist::Installable {
        dist,
        variants_json,
        ..
    } = dist
    else {
        // TODO(konsti): Installed dists? Or is that not a thing here?
        return None;
    };
    let Some(variants_json) = variants_json else {
        return None;
    };
    let Dist::Built(BuiltDist::Registry(dist)) = &**dist else {
        return None;
    };
    if !dist
        .wheels
        .iter()
        .any(|wheel| wheel.filename.variant().is_some())
    {
        return None;
    }
    Some((variants_json, dist))
}
