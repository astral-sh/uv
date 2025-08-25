use uv_cache_key::{CacheKey, CacheKeyHasher, cache_digest};

use crate::{BuildVariables, ConfigSettings, ExtraBuildRequirement};

/// A digest representing the build settings, such as build dependencies or other build-time
/// configuration.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct BuildInfo {
    #[serde(default, skip_serializing_if = "ConfigSettings::is_empty")]
    config_settings: ConfigSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra_build_requires: Vec<ExtraBuildRequirement>,
    #[serde(default, skip_serializing_if = "BuildVariables::is_empty")]
    extra_build_variables: BuildVariables,
}

impl CacheKey for BuildInfo {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.config_settings.cache_key(state);
        self.extra_build_requires.cache_key(state);
        self.extra_build_variables.cache_key(state);
    }
}

impl BuildInfo {
    /// Creates a [`BuildInfo`] instance with the given configuration settings, extra build
    /// dependencies, and extra build variables.
    pub fn from_settings(
        config_settings: &ConfigSettings,
        extra_build_dependencies: &[ExtraBuildRequirement],
        extra_build_variables: Option<&BuildVariables>,
    ) -> Self {
        Self {
            config_settings: config_settings.clone(),
            extra_build_requires: extra_build_dependencies.to_vec(),
            extra_build_variables: extra_build_variables.cloned().unwrap_or_default(),
        }
    }

    /// Returns `true` if the [`BuildInfo`] is empty, meaning it has no configuration settings,
    pub fn is_empty(&self) -> bool {
        self.config_settings.is_empty()
            && self.extra_build_requires.is_empty()
            && self.extra_build_variables.is_empty()
    }

    /// Return the cache shard for this [`BuildInfo`].
    pub fn cache_shard(&self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(cache_digest(self))
        }
    }
}
