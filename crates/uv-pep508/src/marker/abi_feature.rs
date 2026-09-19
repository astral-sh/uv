/// An interpreter ABI feature defined by [PEP 780](https://peps.python.org/pep-0780/).
#[derive(
    Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum AbiFeature {
    /// A 32-bit interpreter.
    #[serde(rename = "32-bit")]
    Bits32,
    /// A 64-bit interpreter.
    #[serde(rename = "64-bit")]
    Bits64,
    /// A CPython interpreter built with debugging capabilities.
    Debug,
    /// A free-threaded CPython interpreter.
    FreeThreading,
    /// A CPython interpreter built with the GIL enabled.
    GilEnabled,
}

impl AbiFeature {
    /// Return the spelling used in environment markers.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bits32 => "32-bit",
            Self::Bits64 => "64-bit",
            Self::Debug => "debug",
            Self::FreeThreading => "free-threading",
            Self::GilEnabled => "gil-enabled",
        }
    }
}
