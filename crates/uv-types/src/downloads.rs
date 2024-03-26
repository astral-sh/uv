use distribution_types::{CachedDist, DistributionId};
use once_map::OnceMap;

#[derive(Default)]
pub struct InFlight {
    /// The in-flight distribution downloads.
    pub downloads: OnceMap<DistributionId, Result<CachedDist, String>>,
}
