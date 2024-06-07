use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum PointerSize {
    /// 32-bit architecture.
    #[serde(rename = "32")]
    _32,
    /// 64-bit architecture.
    #[serde(rename = "64")]
    _64,
}

impl PointerSize {
    pub const fn is_32(self) -> bool {
        matches!(self, Self::_32)
    }

    pub const fn is_64(self) -> bool {
        matches!(self, Self::_64)
    }
}
