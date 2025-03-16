use std::cmp;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use itertools::Itertools;
use sha3::{
    digest::{ExtendableOutput, Update},
    Shake128,
};

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct VariantTag {
    data: String,
}

impl VariantTag {
    pub fn new(data: String) -> Self {
        Self { data }
    }
}

impl std::fmt::Display for VariantTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.data)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VariantError {
    #[error("Invalid priority: `{0}`")]
    InvalidPriority(usize, #[source] std::num::TryFromIntError),
}

/// In `variantlib`, this is called [`VariantKeyConfig`].
#[derive(Debug, Clone, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
pub struct VariantKeyConfig {
    key: String,
    values: Vec<String>,
}

/// In `variantlib`, this is called [`VariantProviderConfig`].
#[derive(Debug, Clone, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
pub struct VariantProviderConfig {
    pub provider: String,
    pub configs: Vec<VariantKeyConfig>,
}

/// In `variantlib`, this is called [`VariantMeta`].
#[derive(Debug, Clone, Eq, Ord, PartialOrd, PartialEq, Hash, serde::Deserialize)]
pub struct VariantMeta {
    provider: String,
    key: String,
    value: String,
}

/// In `variantlib`, this is called [`VariantDescription`].
#[derive(Default, Debug, Clone, Eq, Ord, PartialOrd, PartialEq, Hash)]
pub struct VariantDescription {
    data: Vec<VariantMeta>,
}

impl VariantDescription {
    pub fn tag(&self) -> VariantTag {
        const HASH_LENGTH: usize = 8;

        let mut hasher = Shake128::default();

        for key_value in &self.data {
            hasher.update(key_value.provider.as_bytes());
            hasher.update(" :: ".as_bytes());
            hasher.update(key_value.key.as_bytes());
            hasher.update(" :: ".as_bytes());
            hasher.update(key_value.value.as_bytes());
        }

        let hash = hasher.finalize_boxed(HASH_LENGTH / 2);
        let hex_digest = hex::encode(hash);

        VariantTag::new(hex_digest)
    }
}

#[derive(Default, Debug, Clone)]
pub struct VariantSet {
    map: Arc<HashMap<VariantTag, VariantPriority>>,
}

impl VariantSet {
    pub fn new(data: &[VariantDescription]) -> Result<Self, VariantError> {
        let mut map = HashMap::new();
        for (index, description) in data.iter().enumerate() {
            map.insert(description.tag(), VariantPriority::try_from(index)?);
        }
        Ok(Self { map: Arc::new(map) })
    }

    pub fn compatibility(&self, variant: &VariantTag) -> VariantCompatibility {
        self.map
            .get(variant)
            .map(|&priority| VariantCompatibility::Compatible(priority))
            .unwrap_or(VariantCompatibility::Incompatible)
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum VariantCompatibility {
    Incompatible,
    Compatible(VariantPriority),
}

impl Ord for VariantCompatibility {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match (self, other) {
            (Self::Compatible(p_self), Self::Compatible(p_other)) => p_self.cmp(p_other),
            (Self::Incompatible, Self::Compatible(_)) => cmp::Ordering::Less,
            (Self::Compatible(_), Self::Incompatible) => cmp::Ordering::Greater,
            (Self::Incompatible, Self::Incompatible) => cmp::Ordering::Equal,
        }
    }
}

impl PartialOrd for VariantCompatibility {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(Self::cmp(self, other))
    }
}

impl VariantCompatibility {
    /// Returns `true` if the tag is compatible.
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_))
    }
}

/// The priority of a variant.
///
/// A wrapper around [`NonZeroU32`]. Higher values indicate higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantPriority(NonZeroU32);

impl TryFrom<usize> for VariantPriority {
    type Error = VariantError;

    /// Create a [`VariantPriority`] from a `usize`, where higher `usize` values are given higher
    /// priority.
    fn try_from(priority: usize) -> Result<Self, VariantError> {
        match u32::try_from(priority).and_then(|priority| NonZeroU32::try_from(1 + priority)) {
            Ok(priority) => Ok(Self(priority)),
            Err(err) => Err(VariantError::InvalidPriority(priority, err)),
        }
    }
}

/// Return all possible combinations based on the given [`VariantProviderConfig`] entities.
pub fn get_combinations(data: Vec<VariantProviderConfig>) -> Vec<VariantDescription> {
    if data.is_empty() {
        return Vec::new();
    }

    let transformed_data: Vec<Vec<VariantMeta>> = data
        .into_iter()
        .flat_map(|provider_cnf| {
            provider_cnf.configs.into_iter().map(move |key_config| {
                key_config
                    .values
                    .into_iter()
                    .map(|value| VariantMeta {
                        provider: provider_cnf.provider.clone(),
                        key: key_config.key.clone(),
                        value,
                    })
                    .collect::<Vec<VariantMeta>>()
            })
        })
        .collect();

    let mut combinations = Vec::new();

    for r in (1..=transformed_data.len()).rev() {
        for combo in transformed_data.iter().combinations(r) {
            for vmetas in combo.iter().copied().multi_cartesian_product() {
                let mut sorted_vmetas: Vec<VariantMeta> = vmetas.into_iter().cloned().collect();
                sorted_vmetas
                    .sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.key.cmp(&b.key)));
                let desc = VariantDescription {
                    data: sorted_vmetas,
                };
                combinations.push(desc);
            }
        }
    }

    combinations
}
