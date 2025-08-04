use anyhow::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uv_cache::{Cache, CacheBucket, CacheEntry, Freshness};

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseMetadata {
    pub last_updated: Timestamp,
    pub version: String,
    pub advisory_count: usize,
}

pub struct AuditCache {
    cache: Cache,
}

impl AuditCache {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }

    pub fn database_entry(&self) -> CacheEntry {
        self.cache
            .entry(CacheBucket::VulnerabilityDatabase, "", "osv-database.json")
    }

    pub fn metadata_entry(&self) -> CacheEntry {
        self.cache
            .entry(CacheBucket::VulnerabilityDatabase, "", "meta.json")
    }

    pub fn index_entry(&self) -> CacheEntry {
        self.cache
            .entry(CacheBucket::VulnerabilityDatabase, "", "index.json")
    }

    pub fn should_refresh(&self, ttl_hours: u64) -> Result<bool> {
        let meta_entry = self.metadata_entry();

        fs_err::create_dir_all(meta_entry.dir())?;

        let Ok(freshness) = self.cache.freshness(&meta_entry, None, None) else {
            // If freshness check fails (e.g., due to permission/directory issues),
            // treat as missing/stale and refresh
            return Ok(true);
        };

        match freshness {
            Freshness::Fresh => {
                // Check if metadata is older than TTL
                if let Ok(metadata_content) = fs_err::read_to_string(meta_entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<DatabaseMetadata>(&metadata_content) {
                        let now = Timestamp::now();
                        let age = now.duration_since(meta.last_updated);
                        return Ok(age.as_hours() > i64::try_from(ttl_hours).unwrap_or(i64::MAX));
                    }
                }
                Ok(false)
            }
            Freshness::Stale | Freshness::Missing => Ok(true),
        }
    }

    pub fn save_metadata(&self, metadata: &DatabaseMetadata) -> Result<()> {
        let meta_entry = self.metadata_entry();
        fs_err::create_dir_all(meta_entry.dir())?;
        let content = serde_json::to_string_pretty(metadata)?;
        fs_err::write(meta_entry.path(), content)?;
        Ok(())
    }

    pub fn load_metadata(&self) -> Result<Option<DatabaseMetadata>> {
        let meta_entry = self.metadata_entry();
        if !meta_entry.path().exists() {
            return Ok(None);
        }

        let content = fs_err::read_to_string(meta_entry.path())?;
        let metadata = serde_json::from_str(&content)?;
        Ok(Some(metadata))
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }
}
