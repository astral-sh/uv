use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;

use uv_once_map::OnceMap;

use crate::VariantProviderOutput;
use crate::variants_json::Provider;

type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

/// An in-memory cache for variant provider outputs.
#[derive(Default)]
pub struct VariantProviderCache(FxOnceMap<Provider, Arc<VariantProviderOutput>>);

impl std::ops::Deref for VariantProviderCache {
    type Target = FxOnceMap<Provider, Arc<VariantProviderOutput>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
