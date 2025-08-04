use crate::osv::{OsvAdvisory, OsvClient};
use crate::pypa::PypaParser;
use crate::vulnerability::{Severity, VersionRange, Vulnerability, VulnerabilityDatabase};
use crate::{AuditCache, AuditError, DatabaseMetadata, Result};
use async_zip::tokio::read::fs::ZipFileReader;
use futures::io::AsyncReadExt;
use jiff::Timestamp;
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, info, warn};
use uv_normalize::PackageName;
use uv_pep440::Version;

type VulnerabilityConversionResult = (Vec<Vulnerability>, HashMap<usize, Vec<PackageName>>);

pub struct DatabaseManager {
    cache: AuditCache,
    client: OsvClient,
    test_mode: bool,
}

impl DatabaseManager {
    pub fn new(cache: AuditCache) -> Self {
        let test_mode = std::env::var("UV_AUDIT_TEST_MODE").is_ok() || cfg!(test);
        Self {
            cache,
            client: OsvClient::new(),
            test_mode,
        }
    }

    /// Create a new `DatabaseManager` in test mode (uses fixtures instead of network)
    #[cfg(test)]
    pub fn new_test_mode(cache: AuditCache) -> Self {
        Self {
            cache,
            client: OsvClient::new(),
            test_mode: true,
        }
    }

    /// Get or refresh the vulnerability database
    pub async fn get_database(&self, force_refresh: bool) -> Result<VulnerabilityDatabase> {
        debug!(
            "get_database called with force_refresh={}, test_mode={}",
            force_refresh, self.test_mode
        );

        // In test mode, use fixtures instead of network requests
        if self.test_mode {
            debug!("Test mode enabled, using fixture database");
            return self.load_test_database().await;
        }

        // First try to load from cache if it exists and we don't need to force refresh
        if !force_refresh {
            debug!("Attempting to load from cache first...");
            match self.load_database().await {
                Ok(database) => {
                    debug!("Successfully loaded from cache, checking TTL...");
                    // Check if we need to refresh based on TTL
                    let should_refresh = self.cache.should_refresh(24).unwrap_or(true);
                    debug!("TTL check result: should_refresh={}", should_refresh);
                    if !should_refresh {
                        debug!("Cache is fresh, returning cached database");
                        return Ok(database);
                    }
                    debug!("Cache is stale, will refresh");
                }
                Err(e) => {
                    debug!("Failed to load from cache: {}", e);
                }
            }
        } else {
            debug!("Force refresh requested, skipping cache load");
        }

        // If we reach here, we need to refresh the database
        info!("Refreshing vulnerability database...");
        match self.refresh_database().await {
            Ok(()) => {
                debug!("Successfully refreshed database");
            }
            Err(e) => {
                warn!("Failed to refresh database: {}", e);
                return Err(e);
            }
        }

        // Load database from cache after refresh
        debug!("Loading database from cache after refresh...");
        self.load_database().await
    }

    /// Force refresh the vulnerability database from remote sources
    pub async fn refresh_database(&self) -> Result<()> {
        info!("Downloading PyPA advisory database...");

        // Download the advisory database ZIP
        let zip_data = self.client.download_advisory_database().await?;

        info!("Extracting and parsing advisories...");

        // Extract and parse advisories from ZIP
        let advisories = self.extract_advisories_from_zip(&zip_data).await?;

        info!("Parsed {} advisories", advisories.len());

        // Convert OSV advisories to our internal format
        let (vulnerabilities, package_mapping) = Self::convert_osv_advisories(advisories)?;

        info!("Converted {} vulnerabilities", vulnerabilities.len());

        // Build the database with index using the package mapping
        let database = Self::build_database_with_mapping(vulnerabilities, &package_mapping);

        // Save to cache
        self.save_database(&database).await?;

        info!("Database refresh complete");

        Ok(())
    }

    /// Extract `PyPA` advisories from the downloaded ZIP file
    async fn extract_advisories_from_zip(&self, zip_data: &[u8]) -> Result<Vec<OsvAdvisory>> {
        // Write ZIP data to temporary file for processing
        let temp_dir = tempfile::tempdir()?;
        let zip_path = temp_dir.path().join("advisory-db.zip");

        fs_err::tokio::write(&zip_path, zip_data).await?;

        // Open and read the ZIP file
        let zip_reader = ZipFileReader::new(&zip_path).await?;
        let mut advisories = Vec::new();

        // Process each entry in the ZIP
        debug!(
            "ZIP file contains {} entries",
            zip_reader.file().entries().len()
        );
        let mut yaml_files_count = 0;
        let mut vulns_dir_count = 0;
        let mut parsed_count = 0;
        let mut error_count = 0;
        let mut sample_files = Vec::new();
        let mut sample_errors = Vec::new();

        for i in 0..zip_reader.file().entries().len() {
            let entry = zip_reader.file().entries().get(i).unwrap();
            let filename = entry.filename().as_str().unwrap();

            // Log first 10 filenames to understand structure
            if sample_files.len() < 10 {
                sample_files.push(filename.to_string());
            }

            if std::path::Path::new(filename)
                .extension()
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml")
                })
            {
                yaml_files_count += 1;
            }

            // PyPA structure: advisory-database-main/vulns/PACKAGE_NAME/ADVISORY_ID.yaml
            let is_vulnerability_file =
                std::path::Path::new(filename)
                    .extension()
                    .is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml")
                    })
                    && (filename.starts_with("advisory-database-main/vulns/")
                        || filename.contains("/vulns/"));

            if is_vulnerability_file {
                vulns_dir_count += 1;
                if vulns_dir_count <= 5 {
                    // Log first 5 for debugging
                    debug!("Processing vulnerability file: {}", filename);
                }

                let mut entry_reader = zip_reader.reader_with_entry(i).await?;
                let mut content_bytes = Vec::new();
                entry_reader.read_to_end(&mut content_bytes).await?;
                let content = String::from_utf8(content_bytes).map_err(|_| {
                    AuditError::StringParse(
                        std::str::from_utf8(&[]).unwrap_err(), // Create a valid Utf8Error for the type
                    )
                })?;

                // Parse as PyPA YAML format
                match PypaParser::parse_advisory(&content) {
                    Ok(pypa_advisory) => {
                        // Only include PyPI advisories that have affected packages
                        let package_names = PypaParser::extract_package_names(&pypa_advisory);
                        if !package_names.is_empty() {
                            // Convert PyPA format to OSV format for compatibility
                            match PypaParser::to_osv(&pypa_advisory) {
                                Ok(osv_advisory) => {
                                    advisories.push(osv_advisory);
                                    parsed_count += 1;
                                }
                                Err(e) => {
                                    error_count += 1;
                                    if sample_errors.len() < 5 {
                                        sample_errors
                                            .push(format!("{filename}: conversion error: {e}"));
                                    }
                                    warn!(
                                        "Failed to convert PyPA advisory from {}: {}",
                                        filename, e
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error_count += 1;
                        if sample_errors.len() < 5 {
                            sample_errors.push(format!("{filename}: parse error: {e}"));
                        }
                        // Only warn for the first few errors to avoid spam
                        if error_count <= 10 {
                            warn!("Failed to parse PyPA advisory from {}: {}", filename, e);
                        }
                    }
                }
            }
        }

        debug!("Sample ZIP entries: {:?}", sample_files);
        if !sample_errors.is_empty() {
            debug!("Sample parsing errors: {:?}", sample_errors);
        }

        info!(
            "ZIP extraction complete: {} total YAML files, {} files in vulns/, {} PyPI advisories parsed successfully, {} errors",
            yaml_files_count, vulns_dir_count, parsed_count, error_count
        );

        if parsed_count == 0 {
            return Err(AuditError::EmptyDatabase(format!(
                "Processed {vulns_dir_count} files but none were successfully parsed"
            )));
        }

        Ok(advisories)
    }

    /// Convert OSV advisories to our internal vulnerability format
    /// Returns both vulnerabilities and a mapping from vulnerability index to affected package names
    fn convert_osv_advisories(
        osv_advisories: Vec<OsvAdvisory>,
    ) -> Result<VulnerabilityConversionResult> {
        let mut vulnerabilities = Vec::new();
        let mut package_mapping = HashMap::new();
        let mut package_mapping_failures = 0;

        for osv in osv_advisories {
            // Extract package names before conversion
            let affected_packages = osv
                .pypi_packages()
                .iter()
                .filter_map(
                    |affected| match PackageName::from_str(&affected.package.name) {
                        Ok(name) => Some(name),
                        Err(e) => {
                            debug!(
                                "Invalid package name '{}' in advisory {}: {}",
                                affected.package.name, osv.id, e
                            );
                            None
                        }
                    },
                )
                .collect::<Vec<_>>();

            // Only proceed if we have valid package names
            if affected_packages.is_empty() {
                debug!(
                    "Skipping advisory {} - no valid PyPI packages found",
                    osv.id
                );
                package_mapping_failures += 1;
                continue;
            }

            let vulnerability = Self::convert_osv_advisory(&osv);
            let vuln_index = vulnerabilities.len();
            vulnerabilities.push(vulnerability);

            // Store the package mapping - we know it's not empty from the check above
            package_mapping.insert(vuln_index, affected_packages);
        }

        info!(
            "Advisory conversion complete: {} vulnerabilities processed, {} package mapping failures",
            vulnerabilities.len(),
            package_mapping_failures
        );

        // Validate that every vulnerability has a package mapping
        for (i, vulnerability) in vulnerabilities.iter().enumerate() {
            if !package_mapping.contains_key(&i) {
                return Err(AuditError::DatabaseIntegrity(format!(
                    "Vulnerability {} at index {i} has no package mapping",
                    vulnerability.id
                )));
            }
        }

        if vulnerabilities.is_empty() {
            return Err(AuditError::EmptyDatabase(
                "No valid vulnerabilities were processed - all advisories had invalid package names or conversion errors".to_string()
            ));
        }

        debug!(
            "Package mapping validation successful: {} vulnerabilities, {} mappings",
            vulnerabilities.len(),
            package_mapping.len()
        );

        Ok((vulnerabilities, package_mapping))
    }

    /// Convert a single OSV advisory to our internal format
    fn convert_osv_advisory(osv: &OsvAdvisory) -> Vulnerability {
        // Determine severity from CVSS score or other indicators
        let severity = Self::determine_severity(osv);

        // Extract affected version ranges
        let affected_versions = Self::extract_version_ranges(osv);

        // Extract fixed versions
        let fixed_versions = Self::extract_fixed_versions(osv);

        // Extract reference URLs
        let references = osv.references.iter().map(|r| r.url.clone()).collect();

        // Parse timestamps
        let published = osv
            .published
            .as_ref()
            .and_then(|s| Timestamp::from_str(s).ok());

        let modified = osv
            .modified
            .as_ref()
            .and_then(|s| Timestamp::from_str(s).ok());

        Vulnerability {
            id: osv.id.clone(),
            summary: osv.summary.clone(),
            description: osv.details.clone(),
            severity,
            affected_versions,
            fixed_versions,
            references,
            cvss_score: osv.max_cvss_score(),
            published,
            modified,
        }
    }

    /// Determine severity level from OSV advisory
    fn determine_severity(osv: &OsvAdvisory) -> Severity {
        // Check for CVSS score first
        if let Some(cvss_score) = osv.max_cvss_score() {
            return match cvss_score {
                score if score >= 9.0 => Severity::Critical,
                score if score >= 7.0 => Severity::High,
                score if score >= 4.0 => Severity::Medium,
                _ => Severity::Low,
            };
        }

        // Fallback to keyword-based severity detection
        let text = format!("{} {}", osv.summary, osv.details.as_deref().unwrap_or(""));
        let text_lower = text.to_lowercase();

        if text_lower.contains("critical")
            || text_lower.contains("rce")
            || text_lower.contains("remote code execution")
        {
            Severity::Critical
        } else if text_lower.contains("high")
            || text_lower.contains("sql injection")
            || text_lower.contains("xss")
        {
            Severity::High
        } else if text_lower.contains("medium")
            || text_lower.contains("csrf")
            || text_lower.contains("privilege escalation")
        {
            Severity::Medium
        } else {
            Severity::Low
        }
    }

    /// Extract version ranges from OSV advisory
    fn extract_version_ranges(osv: &OsvAdvisory) -> Vec<VersionRange> {
        let mut ranges = Vec::new();

        for affected in osv.pypi_packages() {
            // Add explicit versions as individual ranges
            if let Some(versions) = &affected.versions {
                for version_str in versions {
                    if let Ok(version) = Version::from_str(version_str) {
                        ranges.push(VersionRange {
                            min: Some(version.clone()),
                            max: Some(version.clone()),
                            constraint: format!("=={version}"),
                        });
                    }
                }
            }

            // Process version ranges
            for range in &affected.ranges {
                if range.range_type == "ECOSYSTEM" {
                    let version_range = Self::parse_osv_range(range);
                    ranges.push(version_range);
                }
            }
        }

        ranges
    }

    /// Parse an OSV range into our internal format
    fn parse_osv_range(range: &crate::osv::OsvRange) -> VersionRange {
        let mut min_version: Option<Version> = None;
        let mut max_version: Option<Version> = None;

        for event in &range.events {
            if let Some(introduced) = &event.introduced {
                if introduced == "0" {
                    min_version = Some(Version::new([0, 0, 0]));
                } else if let Ok(version) = Version::from_str(introduced) {
                    min_version = Some(version);
                }
            }

            if let Some(fixed) = &event.fixed {
                if let Ok(version) = Version::from_str(fixed) {
                    max_version = Some(version);
                }
            }
        }

        // Build constraint string
        let constraint = match (&min_version, &max_version) {
            (Some(min), Some(max)) => format!(">={min},<{max}"),
            (Some(min), None) => format!(">={min}"),
            (None, Some(max)) => format!("<{max}"),
            (None, None) => "*".to_string(),
        };

        VersionRange {
            min: min_version,
            max: max_version,
            constraint,
        }
    }

    /// Extract fixed versions from OSV advisory
    fn extract_fixed_versions(osv: &OsvAdvisory) -> Vec<Version> {
        let mut fixed_versions = Vec::new();

        for affected in osv.pypi_packages() {
            for range in &affected.ranges {
                for event in &range.events {
                    if let Some(fixed) = &event.fixed {
                        if let Ok(version) = Version::from_str(fixed) {
                            fixed_versions.push(version);
                        }
                    }
                }
            }
        }

        // Deduplicate and sort fixed versions
        fixed_versions.sort();
        fixed_versions.dedup();

        fixed_versions
    }

    /// Build the vulnerability database with index using package mapping
    fn build_database_with_mapping(
        vulnerabilities: Vec<Vulnerability>,
        package_mapping: &HashMap<usize, Vec<PackageName>>,
    ) -> VulnerabilityDatabase {
        let mut package_index: HashMap<PackageName, Vec<usize>> = HashMap::new();
        let mut vulnerabilities_without_packages = Vec::new();

        // Build index of package names to vulnerability indices using the preserved mapping
        for (vuln_index, vulnerability) in vulnerabilities.iter().enumerate() {
            if let Some(packages) = package_mapping.get(&vuln_index) {
                // Use the preserved package mapping from PyPA data - this should be the only source
                if packages.is_empty() {
                    vulnerabilities_without_packages.push((vuln_index, &vulnerability.id));
                } else {
                    for package_name in packages {
                        package_index
                            .entry(package_name.clone())
                            .or_default()
                            .push(vuln_index);
                    }
                }
            } else {
                // This should not happen with properly functioning extraction pipeline
                vulnerabilities_without_packages.push((vuln_index, &vulnerability.id));
                warn!(
                    "Vulnerability {} has no package mapping - this indicates a bug in the extraction pipeline",
                    vulnerability.id
                );
            }
        }

        // Log any vulnerabilities that couldn't be mapped to packages
        if !vulnerabilities_without_packages.is_empty() {
            warn!(
                "Found {} vulnerabilities without package mappings: {:?}",
                vulnerabilities_without_packages.len(),
                vulnerabilities_without_packages
                    .iter()
                    .map(|(_, id)| id)
                    .collect::<Vec<_>>()
            );
        }

        VulnerabilityDatabase {
            advisories: vulnerabilities,
            package_index,
        }
    }

    /// Build the vulnerability database with index (fallback method)
    #[allow(dead_code)]
    fn build_database(vulnerabilities: Vec<Vulnerability>) -> VulnerabilityDatabase {
        Self::build_database_with_mapping(vulnerabilities, &HashMap::new())
    }

    /// Extract package names that this vulnerability affects
    ///
    /// DEPRECATED: This method uses unreliable heuristics and should not be used.
    /// Package mappings should be preserved during the OSV conversion pipeline instead.
    /// This method is kept only for backward compatibility and emergency fallback scenarios.
    #[deprecated(
        note = "Use preserved package mappings from PyPA data instead of heuristic extraction"
    )]
    #[allow(deprecated)]
    fn extract_package_names_from_vulnerability(vulnerability: &Vulnerability) -> Vec<PackageName> {
        warn!(
            "Using deprecated heuristic package extraction for vulnerability {} - this indicates a bug in the data pipeline",
            vulnerability.id
        );

        let mut package_names = Vec::new();

        // Try to extract from the vulnerability ID if it follows certain patterns
        if let Some(package_name) = Self::guess_package_name_from_id(&vulnerability.id) {
            package_names.push(package_name);
        }

        // For PYSEC IDs, try extracting package name from the description
        if vulnerability.id.starts_with("PYSEC-") {
            if let Some(package_name) = Self::extract_package_from_description(vulnerability) {
                if !package_names.contains(&package_name) {
                    package_names.push(package_name);
                }
            }
        }

        // Log failure - this should be investigated
        if package_names.is_empty() {
            warn!(
                "Heuristic package extraction failed for vulnerability {} - this vulnerability will not be indexed",
                vulnerability.id
            );
        }

        package_names
    }

    /// Attempt to guess package name from vulnerability ID
    ///
    /// DEPRECATED: This heuristic is unreliable and should not be used in the main pipeline.
    #[deprecated(note = "Use preserved package mappings from PyPA data instead")]
    fn guess_package_name_from_id(id: &str) -> Option<PackageName> {
        // This is a very basic heuristic that often fails
        if id.starts_with("PYSEC-") {
            // PyPI Security Advisory format sometimes includes package names
            // Example: PYSEC-2023-123-django (but many don't follow this pattern)
            let parts: Vec<&str> = id.split('-').collect();
            if parts.len() >= 4 {
                if let Ok(package_name) = PackageName::from_str(parts[3]) {
                    return Some(package_name);
                }
            }
        }

        None
    }

    /// Attempt to extract package name from vulnerability description
    ///
    /// DEPRECATED: This heuristic is extremely unreliable and produces many false positives/negatives.
    #[deprecated(note = "Use preserved package mappings from PyPA data instead")]
    fn extract_package_from_description(vulnerability: &Vulnerability) -> Option<PackageName> {
        // This is a very unreliable heuristic that often fails or gives wrong results
        let text = format!(
            "{} {}",
            vulnerability.summary,
            vulnerability.description.as_deref().unwrap_or("")
        )
        .to_lowercase();

        // Look for common package name patterns - this is a hardcoded list that misses many packages
        let common_packages = [
            "django",
            "flask",
            "requests",
            "numpy",
            "pandas",
            "pillow",
            "cryptography",
            "pyyaml",
            "jinja2",
            "sqlalchemy",
            "celery",
            "tornado",
            "pyramid",
            "bottle",
            "cherrypy",
            "twisted",
            "zope",
            "plone",
            "wagtail",
            "fastapi",
            "starlette",
            "aiohttp",
            "httpx",
            "urllib3",
            "paramiko",
            "pycrypto",
            "lxml",
            "beautifulsoup4",
            "scrapy",
            "selenium",
            "matplotlib",
            "scipy",
            "scikit-learn",
            "tensorflow",
            "pytorch",
            "jupyter",
            "ipython",
            "notebook",
            "gunicorn",
            "uwsgi",
            "mod-wsgi",
        ];

        for package in &common_packages {
            if text.contains(package) {
                if let Ok(package_name) = PackageName::from_str(package) {
                    return Some(package_name);
                }
            }
        }

        None
    }

    /// Save database to cache
    async fn save_database(&self, database: &VulnerabilityDatabase) -> Result<()> {
        debug!("Saving vulnerability database to cache...");

        // Save the main database
        let db_entry = self.cache.database_entry();
        fs_err::create_dir_all(db_entry.dir())?;

        let db_content = serde_json::to_string(&database.advisories)?;
        fs_err::tokio::write(db_entry.path(), db_content).await?;

        // Save the package index
        let index_entry = self.cache.index_entry();
        // Ensure cache directory exists for index file too
        fs_err::create_dir_all(index_entry.dir())?;
        let index_content = serde_json::to_string(&database.package_index)?;
        fs_err::tokio::write(index_entry.path(), index_content).await?;

        // Save metadata (save_metadata already handles directory creation)
        let metadata = DatabaseMetadata {
            last_updated: Timestamp::now(),
            version: "1.0".to_string(),
            advisory_count: database.advisories.len(),
        };

        self.cache.save_metadata(&metadata)?;

        debug!("Database saved to cache");

        Ok(())
    }

    /// Load database from cache
    async fn load_database(&self) -> Result<VulnerabilityDatabase> {
        debug!("[load_database] Loading vulnerability database from cache...");

        // Load advisories
        let db_entry = self.cache.database_entry();

        // Check if cache file exists before trying to read it
        if !db_entry.path().exists() {
            return Err(AuditError::CacheNotFound(
                "Vulnerability database not found in cache".to_string(),
            ));
        }

        let db_content = fs_err::tokio::read_to_string(db_entry.path()).await?;
        let advisories: Vec<Vulnerability> = serde_json::from_str(&db_content)?;

        // Load package index
        let index_entry = self.cache.index_entry();
        let package_index = if index_entry.path().exists() {
            let index_content = fs_err::tokio::read_to_string(index_entry.path()).await?;
            serde_json::from_str(&index_content)?
        } else {
            // Rebuild index if missing
            debug!("Package index missing, rebuilding...");
            Self::build_package_index(&advisories)
        };

        let database = VulnerabilityDatabase {
            advisories,
            package_index,
        };

        debug!("Loaded {} advisories from cache", database.advisories.len());

        Ok(database)
    }

    /// Rebuild package index from advisories
    #[allow(deprecated)]
    fn build_package_index(advisories: &[Vulnerability]) -> HashMap<PackageName, Vec<usize>> {
        let mut package_index: HashMap<PackageName, Vec<usize>> = HashMap::new();

        for (i, vulnerability) in advisories.iter().enumerate() {
            let package_names = Self::extract_package_names_from_vulnerability(vulnerability);

            for package_name in package_names {
                package_index.entry(package_name).or_default().push(i);
            }
        }

        package_index
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<Option<DatabaseMetadata>> {
        self.cache.load_metadata().map_err(AuditError::from)
    }

    /// Load test database from fixtures (used in test mode)
    async fn load_test_database(&self) -> Result<VulnerabilityDatabase> {
        debug!("Loading test database from fixtures...");

        // Find the fixtures directory relative to the current crate
        let fixtures_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");

        if !fixtures_dir.exists() {
            return Err(AuditError::CacheNotFound(format!(
                "Test fixtures directory not found at: {}",
                fixtures_dir.display()
            )));
        }

        // Allow tests to specify which fixture set to use
        let fixture_set =
            std::env::var("UV_AUDIT_TEST_FIXTURE").unwrap_or_else(|_| "test".to_string());

        // Load test database file
        let db_filename = format!("{fixture_set}_database.json");
        let db_path = fixtures_dir.join(&db_filename);
        if !db_path.exists() {
            return Err(AuditError::CacheNotFound(format!(
                "Test database fixture not found at: {}",
                db_path.display()
            )));
        }

        let db_content = fs_err::tokio::read_to_string(&db_path).await?;
        let advisories: Vec<Vulnerability> = serde_json::from_str(&db_content)?;

        // Load test index file
        let index_filename = format!("{fixture_set}_index.json");
        let index_path = fixtures_dir.join(&index_filename);
        let package_index = if index_path.exists() {
            let index_content = fs_err::tokio::read_to_string(&index_path).await?;
            serde_json::from_str(&index_content)?
        } else {
            // Build index from advisories if not available
            debug!("Test index not found, building from advisories...");
            Self::build_package_index(&advisories)
        };

        let database = VulnerabilityDatabase {
            advisories,
            package_index,
        };

        debug!(
            "Loaded {} test advisories from fixtures (fixture_set: {})",
            database.advisories.len(),
            fixture_set
        );

        Ok(database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osv::{OsvAffected, OsvEvent, OsvPackage, OsvRange};
    use tempfile::TempDir;
    use uv_cache::Cache;

    #[tokio::test]
    async fn test_database_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);

        let _manager = DatabaseManager::new(audit_cache);
    }

    #[test]
    fn test_severity_determination() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);
        let _manager = DatabaseManager::new(audit_cache);

        let mut osv = OsvAdvisory {
            id: "test".to_string(),
            summary: "Critical remote code execution".to_string(),
            details: None,
            affected: vec![],
            references: vec![],
            severity: vec![],
            published: None,
            modified: None,
            database_specific: None,
        };

        let severity = DatabaseManager::determine_severity(&osv);
        assert_eq!(severity, Severity::Critical);

        osv.summary = "Medium severity vulnerability".to_string();
        let severity = DatabaseManager::determine_severity(&osv);
        assert_eq!(severity, Severity::Medium);
    }

    #[test]
    fn test_package_mapping_preservation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);
        let _manager = DatabaseManager::new(audit_cache);

        // Create test OSV advisories with explicit package information
        let osv_advisories = vec![
            OsvAdvisory {
                id: "PYSEC-2025-1".to_string(),
                summary: "Test vulnerability in Django".to_string(),
                details: Some("A test vulnerability affecting Django".to_string()),
                affected: vec![OsvAffected {
                    package: OsvPackage {
                        ecosystem: "PyPI".to_string(),
                        name: "django".to_string(),
                        purl: Some("pkg:pypi/django".to_string()),
                    },
                    ranges: vec![OsvRange {
                        range_type: "ECOSYSTEM".to_string(),
                        repo: None,
                        events: vec![OsvEvent {
                            introduced: Some("0".to_string()),
                            fixed: Some("5.0.0".to_string()),
                            last_affected: None,
                            limit: None,
                        }],
                        database_specific: None,
                    }],
                    versions: None,
                    database_specific: None,
                    ecosystem_specific: None,
                }],
                references: vec![],
                severity: vec![],
                published: Some("2025-01-01T00:00:00Z".to_string()),
                modified: None,
                database_specific: None,
            },
            OsvAdvisory {
                id: "PYSEC-2025-2".to_string(),
                summary: "Test vulnerability in Flask and Jinja2".to_string(),
                details: Some("A test vulnerability affecting multiple packages".to_string()),
                affected: vec![
                    OsvAffected {
                        package: OsvPackage {
                            ecosystem: "PyPI".to_string(),
                            name: "flask".to_string(),
                            purl: Some("pkg:pypi/flask".to_string()),
                        },
                        ranges: vec![],
                        versions: Some(vec!["2.0.0".to_string()]),
                        database_specific: None,
                        ecosystem_specific: None,
                    },
                    OsvAffected {
                        package: OsvPackage {
                            ecosystem: "PyPI".to_string(),
                            name: "jinja2".to_string(),
                            purl: Some("pkg:pypi/jinja2".to_string()),
                        },
                        ranges: vec![],
                        versions: Some(vec!["3.1.0".to_string()]),
                        database_specific: None,
                        ecosystem_specific: None,
                    },
                ],
                references: vec![],
                severity: vec![],
                published: Some("2025-01-01T00:00:00Z".to_string()),
                modified: None,
                database_specific: None,
            },
        ];

        // Test conversion process
        let result = DatabaseManager::convert_osv_advisories(osv_advisories).unwrap();
        let (vulnerabilities, package_mapping) = result;

        // Verify we have the expected number of vulnerabilities
        assert_eq!(vulnerabilities.len(), 2);

        // Verify package mapping is preserved correctly
        assert_eq!(package_mapping.len(), 2);

        // Check first vulnerability (Django)
        let vuln_0_packages = &package_mapping[&0];
        assert_eq!(vuln_0_packages.len(), 1);
        assert_eq!(vuln_0_packages[0].to_string(), "django");

        // Check second vulnerability (Flask + Jinja2)
        let vuln_1_packages = &package_mapping[&1];
        assert_eq!(vuln_1_packages.len(), 2);
        let package_names: Vec<String> = vuln_1_packages.iter().map(ToString::to_string).collect();
        assert!(package_names.contains(&"flask".to_string()));
        assert!(package_names.contains(&"jinja2".to_string()));

        // Test database building with mapping
        let database =
            DatabaseManager::build_database_with_mapping(vulnerabilities, &package_mapping);

        // Verify package index is built correctly
        assert!(
            database
                .package_index
                .contains_key(&PackageName::from_str("django").unwrap())
        );
        assert!(
            database
                .package_index
                .contains_key(&PackageName::from_str("flask").unwrap())
        );
        assert!(
            database
                .package_index
                .contains_key(&PackageName::from_str("jinja2").unwrap())
        );

        // Verify Django points to first vulnerability
        let django_vulns = &database.package_index[&PackageName::from_str("django").unwrap()];
        assert_eq!(django_vulns.len(), 1);
        assert_eq!(django_vulns[0], 0);

        // Verify Flask and Jinja2 point to second vulnerability
        let flask_vulns = &database.package_index[&PackageName::from_str("flask").unwrap()];
        assert_eq!(flask_vulns.len(), 1);
        assert_eq!(flask_vulns[0], 1);

        let jinja2_vulns = &database.package_index[&PackageName::from_str("jinja2").unwrap()];
        assert_eq!(jinja2_vulns.len(), 1);
        assert_eq!(jinja2_vulns[0], 1);
    }

    #[test]
    fn test_conversion_validation_rejects_empty_packages() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);
        let _manager = DatabaseManager::new(audit_cache);

        // Create OSV advisory with no PyPI packages
        let osv_advisories = vec![OsvAdvisory {
            id: "TEST-NO-PACKAGES".to_string(),
            summary: "Vulnerability with no PyPI packages".to_string(),
            details: None,
            affected: vec![], // No affected packages
            references: vec![],
            severity: vec![],
            published: None,
            modified: None,
            database_specific: None,
        }];

        // Conversion should result in empty database error
        let result = DatabaseManager::convert_osv_advisories(osv_advisories);
        match result {
            Err(AuditError::EmptyDatabase(_)) => {
                // Expected
            }
            _ => panic!("Expected EmptyDatabase error when no valid packages found"),
        }
    }

    #[test]
    fn test_conversion_validation_rejects_invalid_package_names() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);
        let _manager = DatabaseManager::new(audit_cache);

        // Create OSV advisory with invalid package name
        let osv_advisories = vec![OsvAdvisory {
            id: "TEST-INVALID-PACKAGE".to_string(),
            summary: "Vulnerability with invalid package name".to_string(),
            details: None,
            affected: vec![OsvAffected {
                package: OsvPackage {
                    ecosystem: "PyPI".to_string(),
                    name: "invalid-package-name-with-@#$%^&*()".to_string(), // Invalid characters
                    purl: None,
                },
                ranges: vec![],
                versions: None,
                database_specific: None,
                ecosystem_specific: None,
            }],
            references: vec![],
            severity: vec![],
            published: None,
            modified: None,
            database_specific: None,
        }];

        // Conversion should result in empty database error since no valid packages
        let result = DatabaseManager::convert_osv_advisories(osv_advisories);
        match result {
            Err(AuditError::EmptyDatabase(_)) => {
                // Expected - invalid package names are filtered out
            }
            _ => panic!("Expected EmptyDatabase error when no valid package names found"),
        }
    }

    #[test]
    fn test_database_integrity_validation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        let audit_cache = AuditCache::new(cache);
        let _manager = DatabaseManager::new(audit_cache);

        // Create vulnerability without corresponding package mapping (simulating a bug)
        let vulnerabilities = vec![Vulnerability {
            id: "TEST-NO-MAPPING".to_string(),
            summary: "Test vulnerability".to_string(),
            description: None,
            severity: Severity::Medium,
            affected_versions: vec![],
            fixed_versions: vec![],
            references: vec![],
            cvss_score: None,
            published: None,
            modified: None,
        }];

        let package_mapping = HashMap::new(); // Empty mapping

        // Database building should succeed but log warnings about unmapped vulnerabilities
        let database =
            DatabaseManager::build_database_with_mapping(vulnerabilities, &package_mapping);

        // Verify that the vulnerability exists but has no package index entries
        assert_eq!(database.advisories.len(), 1);
        assert_eq!(database.package_index.len(), 0); // No packages mapped

        // This tests that we don't crash but handle missing mappings gracefully
    }
}
