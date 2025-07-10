use std::sync::Arc;
use uv_distribution_types::{CachedDist, DistributionId};
use uv_once_map::OnceMap;

#[derive(Default, Clone)]
pub struct InFlight {
    /// The in-flight distribution downloads.
    pub downloads: Arc<OnceMap<DistributionId, Result<CachedDist, String>>>,
}
