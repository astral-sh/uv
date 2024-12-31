use std::str::FromStr;

use serde::ser::SerializeSeq;

use uv_pep508::MarkerTree;

/// A list of supported marker environments.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct SupportedEnvironments(Vec<MarkerTree>);

impl SupportedEnvironments {
    /// Create a new [`SupportedEnvironments`] struct from a list of marker trees.
    pub fn from_markers(markers: Vec<MarkerTree>) -> Self {
        SupportedEnvironments(markers)
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
    pub fn iter(&self) -> std::slice::Iter<MarkerTree> {
        self.0.iter()
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
    fn deserialize<D>(deserializer: D) -> Result<SupportedEnvironments, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StringOrVecVisitor;

        impl<'de> serde::de::Visitor<'de> for StringOrVecVisitor {
            type Value = SupportedEnvironments;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or a list of strings")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let marker = MarkerTree::from_str(value).map_err(serde::de::Error::custom)?;
                Ok(SupportedEnvironments(vec![marker]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut markers = Vec::new();

                while let Some(elem) = seq.next_element::<String>()? {
                    let marker = MarkerTree::from_str(&elem).map_err(serde::de::Error::custom)?;
                    markers.push(marker);
                }

                Ok(SupportedEnvironments(markers))
            }
        }

        deserializer.deserialize_any(StringOrVecVisitor)
    }
}
