use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

/// Preserves the verbatim string representation when deserializing `T`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SerdeVerbatim<T> {
    verbatim: String,
    inner: T,
}

impl<T> SerdeVerbatim<T> {
    pub(crate) fn verbatim(&self) -> &str {
        &self.verbatim
    }
}

impl<T> Deref for SerdeVerbatim<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Display> Display for SerdeVerbatim<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'de, T: FromStr> Deserialize<'de> for SerdeVerbatim<T>
where
    <T as FromStr>::Err: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let verbatim = String::deserialize(deserializer)?;
        let inner = T::from_str(&verbatim).map_err(serde::de::Error::custom)?;
        Ok(Self { verbatim, inner })
    }
}

impl<T: Serialize> Serialize for SerdeVerbatim<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}
