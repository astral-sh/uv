use serde::{Deserialize, Serialize};
use uv_cache_info::Timestamp;

#[derive(Deserialize, Serialize)]
pub struct CachedByTimestamp<Data> {
    pub timestamp: Timestamp,
    pub data: Data,
}
