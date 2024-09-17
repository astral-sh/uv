use std::str::FromStr;

use serde::ser::SerializeSeq;

use pep508_rs::MarkerTree;

/// A list of supported marker environments.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct SupportedEnvironments(Vec<MarkerTree>);

impl SupportedEnvironments {
    /// Return the list of marker trees.
    pub fn as_markers(&self) -> &[MarkerTree] {
        &self.0
    }

    /// Convert the [`SupportedEnvironments`] struct into a list of marker trees.
    pub fn into_markers(self) -> Vec<MarkerTree> {
        self.0
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
