use serde::{Deserialize, Serialize};

use crate::timestamp::Timestamp;

#[derive(Deserialize, Serialize)]
pub struct CachedByTimestamp<Data> {
    pub timestamp: Timestamp,
    pub data: Data,
}
