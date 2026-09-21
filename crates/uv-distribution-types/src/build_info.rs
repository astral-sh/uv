use std::fmt::{self, Display, Formatter};

use uv_cache_key::{CacheKey, CacheKeyHasher, cache_digest};
use uv_pypi_types::HashDigest;

use crate::{BuildVariables, ConfigSettings, ExtraBuildRequirement};

/// Content identity of a required build-dependency lock contract.
#[derive(Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct BuildLockFingerprint(HashDigest);

impl BuildLockFingerprint {
    pub fn new(hash: HashDigest) -> Self {
        Self(hash)
    }
}

impl Display for BuildLockFingerprint {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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
    build_lock: Option<BuildLockFingerprint>,
}

impl CacheKey for BuildInfo {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.config_settings.cache_key(state);
        self.extra_build_requires.cache_key(state);
        self.extra_build_variables.cache_key(state);
        if let Some(build_lock) = &self.build_lock {
            "build-lock".cache_key(state);
            build_lock.to_string().cache_key(state);
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
            build_lock: None,
        }
    }

    /// Identify the required build graph used to produce this wheel.
    #[must_use]
    pub fn with_build_lock_fingerprint(
        mut self,
        fingerprint: Option<&BuildLockFingerprint>,
    ) -> Self {
        self.build_lock = fingerprint.cloned();
        self
    }

    pub fn build_lock_fingerprint(&self) -> Option<&BuildLockFingerprint> {
        self.build_lock.as_ref()
    }

    /// Returns `true` if the [`BuildInfo`] is empty, meaning it has no configuration settings,
    fn is_empty(&self) -> bool {
        self.config_settings.is_empty()
            && self.extra_build_requires.is_empty()
            && self.extra_build_variables.is_empty()
            && self.build_lock.is_none()
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
    use std::error::Error;
    use std::str::FromStr;

    use uv_pypi_types::Digest;

    use super::*;
    use crate::ConfigSettingEntry;

    #[test]
    fn build_lock_fingerprint_scopes_only_locked_builds() -> Result<(), Box<dyn Error>> {
        let settings: ConfigSettings = [ConfigSettingEntry::from_str("setting=value")?]
            .into_iter()
            .collect();
        let ordinary = BuildInfo::from_settings(settings.clone(), vec![], None);
        let legacy_key = cache_digest(&(
            settings,
            Vec::<ExtraBuildRequirement>::new(),
            BuildVariables::new(),
        ));
        assert_eq!(ordinary.cache_shard().as_deref(), Some(legacy_key.as_str()));
        assert_eq!(
            serde_json::from_str::<BuildInfo>("{}")?,
            BuildInfo::default()
        );
        let first = BuildLockFingerprint::new(HashDigest::Sha256(Digest::from_bytes([1; 32])));
        let second = BuildLockFingerprint::new(HashDigest::Sha256(Digest::from_bytes([2; 32])));
        let locked = ordinary.clone().with_build_lock_fingerprint(Some(&first));
        assert_ne!(locked.cache_shard(), ordinary.cache_shard());
        assert_ne!(
            locked.cache_shard(),
            ordinary
                .with_build_lock_fingerprint(Some(&second))
                .cache_shard()
        );
        assert_eq!(
            serde_json::from_str::<BuildInfo>(&serde_json::to_string(&locked)?)?,
            locked
        );
        Ok(())
    }
}
