use std::marker::PhantomData;
use std::str::FromStr;

use serde::ser::SerializeSeq;

use uv_pep508::MarkerTree;

/// A list of supported marker environments.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct SupportedEnvironments(Vec<MarkerTree>);

impl SupportedEnvironments {
    /// Create a new [`SupportedEnvironments`] struct from a list of marker trees.
    pub fn from_markers(markers: Vec<MarkerTree>) -> Self {
        Self(markers)
    }

    /// Return the list of marker trees.
    pub fn as_markers(&self) -> &[MarkerTree] {
        &self.0
    }

    /// Convert the [`SupportedEnvironments`] struct into a list of marker trees.
    pub fn into_markers(self) -> Vec<MarkerTree> {
        self.0
    }

    /// Returns an iterator over the marker trees.
    pub fn iter(&self) -> std::slice::Iter<'_, MarkerTree> {
        self.0.iter()
    }

    /// Returns `true` if there are no supported environments.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a SupportedEnvironments {
    type IntoIter = std::slice::Iter<'a, MarkerTree>;
    type Item = &'a MarkerTree;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Serialize a [`SupportedEnvironments`] struct into a list of marker strings.
impl serde::Serialize for SupportedEnvironments {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for element in &self.0 {
            if let Some(contents) = element.contents() {
                seq.serialize_element(&contents)?;
            }
        }
        seq.end()
    }
}

/// Deserialize a marker string or list of marker strings into a [`SupportedEnvironments`] struct.
impl<'de> serde::Deserialize<'de> for SupportedEnvironments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_marker_entries(deserializer).map(Self)
    }
}

/// Deserialize a single marker string or a list of environment entries.
pub fn deserialize_marker_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + From<MarkerTree>,
{
    struct MarkerEntries<T>(PhantomData<T>);

    impl<'de, T: serde::Deserialize<'de> + From<MarkerTree>> serde::de::Visitor<'de>
        for MarkerEntries<T>
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a marker string or a list of environments")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            let marker = MarkerTree::from_str(value).map_err(E::custom)?;
            Ok(vec![T::from(marker)])
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = seq.next_element()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_any(MarkerEntries(PhantomData))
}
