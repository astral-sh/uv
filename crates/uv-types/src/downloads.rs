use distribution_types::{CachedDist, DistributionId};
use once_map::OnceMap;
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct InFlight {
    /// The in-flight distribution downloads.
    pub downloads: Arc<OnceMap<DistributionId, Result<CachedDist, String>>>,
}
