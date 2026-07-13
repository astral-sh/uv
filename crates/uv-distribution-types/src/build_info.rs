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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locked_build_resolution: Option<String>,
}

impl CacheKey for BuildInfo {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.config_settings.cache_key(state);
        self.extra_build_requires.cache_key(state);
        self.extra_build_variables.cache_key(state);
        if let Some(digest) = &self.locked_build_resolution {
            digest.cache_key(state);
        }
    }
}

impl BuildInfo {
    /// Creates a [`BuildInfo`] instance with the given configuration settings, extra build
    /// dependencies, and extra build variables.
    pub fn from_settings(
        config_settings: ConfigSettings,
        extra_build_dependencies: Vec<ExtraBuildRequirement>,
        extra_build_variables: Option<BuildVariables>,
    ) -> Self {
        Self {
            config_settings,
            extra_build_requires: extra_build_dependencies,
            extra_build_variables: extra_build_variables.unwrap_or_default(),
            locked_build_resolution: None,
        }
    }

    /// Attach the digest of the locked build environment used to build the distribution.
    #[must_use]
    pub fn with_locked_build_resolution(mut self, digest: Option<String>) -> Self {
        self.locked_build_resolution = digest;
        self
    }

    /// Returns `true` if the [`BuildInfo`] is empty, meaning it has no configuration settings,
    fn is_empty(&self) -> bool {
        self.config_settings.is_empty()
            && self.extra_build_requires.is_empty()
            && self.extra_build_variables.is_empty()
            && self.locked_build_resolution.is_none()
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

#[cfg(test)]
mod tests {
    use super::*;

    struct LegacyBuildInfo<'a>(&'a BuildInfo);

    impl CacheKey for LegacyBuildInfo<'_> {
        fn cache_key(&self, state: &mut CacheKeyHasher) {
            self.0.config_settings.cache_key(state);
            self.0.extra_build_requires.cache_key(state);
            self.0.extra_build_variables.cache_key(state);
        }
    }

    #[test]
    fn locked_build_resolution_preserves_legacy_cache_key_when_absent() {
        let build_info = BuildInfo::from_settings(ConfigSettings::default(), Vec::new(), None);
        assert_eq!(
            cache_digest(&build_info),
            cache_digest(&LegacyBuildInfo(&build_info))
        );

        let locked = build_info.with_locked_build_resolution(Some("locked".to_string()));
        assert_ne!(
            cache_digest(&locked),
            cache_digest(&LegacyBuildInfo(&locked))
        );
    }
}
