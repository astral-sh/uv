use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;
use uv_distribution_types::GlobalVersionId;
use uv_once_map::OnceMap;
use uv_variants::resolved_variants::ResolvedVariants;

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
