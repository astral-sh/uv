use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct CachedByTimestamp<T> {
    pub timestamp: SystemTime,
    pub data: T,
}
