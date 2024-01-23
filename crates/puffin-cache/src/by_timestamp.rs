use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct CachedByTimestamp<Timestamp, Data> {
    pub timestamp: Timestamp,
    pub data: Data,
}
