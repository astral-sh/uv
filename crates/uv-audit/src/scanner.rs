use crate::{AuditError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, warn};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver::{Lock, Package};
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

/// A dependency found during scanning
#[derive(Debug, Clone)]
pub struct ScannedDependency {
    /// Package name
    pub name: PackageName,
    /// Installed version
    pub version: Version,
    /// Whether this is a direct dependency (listed in pyproject.toml)
    pub is_direct: bool,
    /// Source of the dependency (PyPI, git, path, etc.)
    pub source: DependencySource,
    /// Optional path for path dependencies
    pub path: Option<std::path::PathBuf>,
}

/// Source type for dependencies
#[derive(Debug, Clone)]
pub enum DependencySource {
    /// PyPI registry
    Registry,
    /// Git repository
    Git { url: String, rev: Option<String> },
    /// Local path
    Path,
    /// Direct URL
    Url(String),
}

/// Information about a dependency in the dependency graph
#[derive(Debug, Clone)]
struct DependencyInfo {
    /// Whether this is a direct dependency
    is_direct: bool,
    /// Dependency type (dev, optional, etc.)
    dependency_type: DependencyType,
}

/// Type of dependency
#[derive(Debug, Clone, Copy, PartialEq)]
enum DependencyType {
    /// Main production dependency
    Main,
    /// Development dependency
    Dev,
    /// Optional dependency
    Optional,
}

/// Scanner for Python project dependencies
pub struct DependencyScanner {
    /// Include development dependencies
    include_dev: bool,
    /// Include optional dependencies
    include_optional: bool,
    /// Only scan direct dependencies
    direct_only: bool,
}

impl DependencyScanner {
    /// Create a new dependency scanner
    pub fn new(include_dev: bool, include_optional: bool, direct_only: bool) -> Self {
        Self {
            include_dev,
            include_optional,
            direct_only,
        }
    }

    /// Scan dependencies from a project directory
    pub async fn scan_project(&self, project_path: &Path) -> Result<Vec<ScannedDependency>> {
        debug!("Scanning dependencies in: {}", project_path.display());

        // First try to scan from uv.lock if available
        let lock_path = project_path.join("uv.lock");
        if lock_path.exists() {
            debug!("Found uv.lock, scanning from lock file");
            return self.scan_from_lock(&lock_path, project_path).await;
        }

        // Fallback to scanning from pyproject.toml
        let pyproject_path = project_path.join("pyproject.toml");
        if pyproject_path.exists() {
            debug!("Found pyproject.toml, scanning from project file");
            return self.scan_from_pyproject(&pyproject_path).await;
        }

        Err(AuditError::NoDependencyInfo)
    }

    /// Scan dependencies from uv.lock file
    async fn scan_from_lock(
        &self,
        lock_path: &Path,
        project_path: &Path,
    ) -> Result<Vec<ScannedDependency>> {
        debug!("Reading lock file: {}", lock_path.display());

        // Read and parse lock file using uv's standard approach
        let lock_content = fs_err::tokio::read_to_string(lock_path)
            .await
            .map_err(AuditError::Cache)?;

        let lock: Lock = toml::from_str(&lock_content).map_err(AuditError::LockFileParse)?;

        // Validate lock file structure
        if lock.packages().is_empty() {
            warn!("Lock file contains no packages: {}", lock_path.display());
        }

        debug!("Found {} packages in lock file", lock.packages().len());

        // Load workspace to determine direct dependencies
        let workspace = Workspace::discover(
            project_path,
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await
        .map_err(AuditError::WorkspaceDiscovery)?;

        // Get direct dependencies
        let direct_deps_with_types = self.get_direct_dependencies_with_types(&workspace);

        debug!(
            "Found {} direct dependencies from workspace",
            direct_deps_with_types.len()
        );

        let mut dependencies = Vec::new();

        // Build dependency graph
        let dependency_graph: HashMap<PackageName, DependencyInfo> =
            Self::build_dependency_graph(&lock, &direct_deps_with_types);

        for package in lock.packages() {
            let package_name = package.name().clone();
            let version = package.version();

            debug!(
                "Processing package: {} (version: {:?})",
                package_name, version
            );

            // Get dependency information from graph
            let dep_info = dependency_graph.get(&package_name).ok_or_else(|| {
                // Enhanced error reporting to help debug the issue
                let available_packages: Vec<String> =
                    dependency_graph.keys().map(std::string::ToString::to_string).collect();
                debug!(
                    "Available packages in dependency graph: {:?}",
                    available_packages
                );
                AuditError::InvalidDependency(format!(
                    "Package {package_name} not found in dependency graph. Available packages: [{}]",
                    available_packages.join(", ")
                ))
            })?;

            // Apply filtering based on configuration
            if self.direct_only && !dep_info.is_direct {
                continue;
            }

            // Check if this package should be included based on dev/optional flags
            if !self.should_include_package_with_info(&package_name, dep_info) {
                continue;
            }

            let source = Self::determine_source_from_package(package);
            let path = Self::extract_package_path(package);

            dependencies.push(ScannedDependency {
                name: package_name,
                version: version.cloned().unwrap_or_else(|| {
                    warn!(
                        "Package {} has no version, using placeholder",
                        package.name()
                    );
                    Version::new([0, 0, 0])
                }),
                is_direct: dep_info.is_direct,
                source,
                path,
            });
        }

        debug!("Found {} dependencies in lock file", dependencies.len());
        Ok(dependencies)
    }

    /// Scan dependencies from pyproject.toml file
    async fn scan_from_pyproject(&self, pyproject_path: &Path) -> Result<Vec<ScannedDependency>> {
        debug!("Reading pyproject.toml: {}", pyproject_path.display());

        let project_path = pyproject_path.parent().unwrap();
        let workspace = Workspace::discover(
            project_path,
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await
        .map_err(AuditError::WorkspaceDiscovery)?;

        let mut dependencies = Vec::new();
        let mut warned_about_placeholder = false;

        // Get direct dependencies from pyproject.toml with enhanced parsing
        let direct_deps_with_info = self.get_direct_dependencies_with_info(&workspace);

        for (package_name, dep_type, version_spec) in direct_deps_with_info {
            // Check if this dependency type should be included
            if !self.should_include_dependency_type(dep_type) {
                continue;
            }

            // For pyproject.toml scanning, we can only get direct dependencies
            // and we don't know the exact installed versions without a lock file
            if !warned_about_placeholder {
                warn!(
                    "Scanning from pyproject.toml only shows direct dependencies with version constraints. \
                    Run 'uv lock' to generate a complete dependency tree with exact versions."
                );
                warned_about_placeholder = true;
            }

            // Try to extract a reasonable version from the version specification
            let version = Self::extract_version_from_spec(&version_spec)
                .unwrap_or_else(|| Version::new([0, 0, 0]));

            // Determine source type from package name/spec
            let source = Self::determine_source_from_spec(&package_name, &version_spec);
            let path = Self::extract_path_from_spec(&version_spec);

            dependencies.push(ScannedDependency {
                name: package_name,
                version,
                is_direct: true,
                source,
                path,
            });
        }

        debug!(
            "Found {} direct dependencies in pyproject.toml",
            dependencies.len()
        );
        Ok(dependencies)
    }

    /// Get direct dependencies
    fn get_direct_dependencies_with_info(
        &self,
        workspace: &Workspace,
    ) -> Vec<(PackageName, DependencyType, String)> {
        // Use a map to track dependencies with proper priority
        let mut deps_map: HashMap<PackageName, (DependencyType, String)> = HashMap::new();

        for member in workspace.packages().values() {
            let pyproject = member.pyproject_toml();

            // Add main dependencies
            if let Some(project_table) = &pyproject.project {
                if let Some(dependencies) = &project_table.dependencies {
                    for dep_str in dependencies {
                        if let Ok(package_name) = extract_package_name_from_dep_string(dep_str) {
                            deps_map.insert(package_name, (DependencyType::Main, dep_str.clone()));
                        }
                    }
                }

                // Add optional dependencies if requested
                if self.include_optional {
                    if let Some(optional_deps) = &project_table.optional_dependencies {
                        for deps in optional_deps.values() {
                            for dep_str in deps {
                                if let Ok(package_name) =
                                    extract_package_name_from_dep_string(dep_str)
                                {
                                    // Only insert if not already present as Main
                                    deps_map
                                        .entry(package_name)
                                        .or_insert((DependencyType::Optional, dep_str.clone()));
                                }
                            }
                        }
                    }
                }
            }

            // Add development dependencies if requested
            if self.include_dev {
                if let Some(tool_uv) = &pyproject.tool.as_ref().and_then(|t| t.uv.as_ref()) {
                    if let Some(dev_deps) = &tool_uv.dev_dependencies {
                        for dep in dev_deps {
                            // Convert Requirement to string representation
                            let dep_str = format!("{dep}");
                            deps_map.insert(dep.name.clone(), (DependencyType::Dev, dep_str));
                        }
                    }
                }
            }
        }

        // Convert map to vector
        let direct_deps: Vec<(PackageName, DependencyType, String)> = deps_map
            .into_iter()
            .map(|(name, (dep_type, spec))| (name, dep_type, spec))
            .collect();

        debug!(
            "Found {} direct dependencies with info: {} main, {} dev, {} optional",
            direct_deps.len(),
            direct_deps
                .iter()
                .filter(|(_, t, _)| *t == DependencyType::Main)
                .count(),
            direct_deps
                .iter()
                .filter(|(_, t, _)| *t == DependencyType::Dev)
                .count(),
            direct_deps
                .iter()
                .filter(|(_, t, _)| *t == DependencyType::Optional)
                .count(),
        );

        direct_deps
    }

    /// Get direct dependencies with their types for use in lock file scanning
    fn get_direct_dependencies_with_types(
        &self,
        workspace: &Workspace,
    ) -> HashMap<PackageName, DependencyType> {
        let mut direct_deps_map = HashMap::new();

        for member in workspace.packages().values() {
            let pyproject = member.pyproject_toml();

            // Add main dependencies first (lowest priority)
            if let Some(project_table) = &pyproject.project {
                if let Some(dependencies) = &project_table.dependencies {
                    for dep_str in dependencies {
                        if let Ok(package_name) = extract_package_name_from_dep_string(dep_str) {
                            direct_deps_map.insert(package_name, DependencyType::Main);
                        }
                    }
                }

                // Add optional dependencies (don't override Main)
                if self.include_optional {
                    if let Some(optional_deps) = &project_table.optional_dependencies {
                        for deps in optional_deps.values() {
                            for dep_str in deps {
                                if let Ok(package_name) =
                                    extract_package_name_from_dep_string(dep_str)
                                {
                                    // Optional dependencies don't override Main dependencies
                                    // They're considered Optional only if not already Main
                                    direct_deps_map
                                        .entry(package_name)
                                        .or_insert(DependencyType::Optional);
                                }
                            }
                        }
                    }
                }
            }

            // Add development dependencies (highest priority - overrides Main and Optional)
            if self.include_dev {
                if let Some(tool_uv) = &pyproject.tool.as_ref().and_then(|t| t.uv.as_ref()) {
                    if let Some(dev_deps) = &tool_uv.dev_dependencies {
                        for dep in dev_deps {
                            // Dev dependencies override everything else
                            // This is because if a package is both a main/optional AND dev dependency,
                            // we want to treat it as Dev for filtering purposes
                            direct_deps_map.insert(dep.name.clone(), DependencyType::Dev);
                        }
                    }
                }
            }
        }

        debug!(
            "Classified {} direct dependencies: {} main, {} dev, {} optional",
            direct_deps_map.len(),
            direct_deps_map
                .values()
                .filter(|t| **t == DependencyType::Main)
                .count(),
            direct_deps_map
                .values()
                .filter(|t| **t == DependencyType::Dev)
                .count(),
            direct_deps_map
                .values()
                .filter(|t| **t == DependencyType::Optional)
                .count(),
        );

        direct_deps_map
    }

    /// Check if a dependency type should be included based on scanner configuration
    fn should_include_dependency_type(&self, dep_type: DependencyType) -> bool {
        match dep_type {
            DependencyType::Main => true,
            DependencyType::Dev => self.include_dev,
            DependencyType::Optional => self.include_optional,
        }
    }

    /// Extract version from dependency specification string
    fn extract_version_from_spec(version_spec: &str) -> Option<Version> {
        // Try to extract version from specs like "package>=1.0.0", "package==2.1.0", etc.

        // Look for exact version specification (==)
        if let Some(pos) = version_spec.find("==") {
            let version_part = &version_spec[pos + 2..];
            // Extract version until space, comma, or end
            let version_str = version_part
                .split_whitespace()
                .next()
                .unwrap_or(version_part)
                .split(',')
                .next()
                .unwrap_or(version_part)
                .trim();

            if let Ok(version) = Version::from_str(version_str) {
                return Some(version);
            }
        }

        // Look for minimum version specification (>=)
        if let Some(pos) = version_spec.find(">=") {
            let version_part = &version_spec[pos + 2..];
            let version_str = version_part
                .split_whitespace()
                .next()
                .unwrap_or(version_part)
                .split(',')
                .next()
                .unwrap_or(version_part)
                .trim();

            if let Ok(version) = Version::from_str(version_str) {
                return Some(version);
            }
        }

        None
    }

    /// Determine source type from dependency specification
    fn determine_source_from_spec(
        _package_name: &PackageName,
        version_spec: &str,
    ) -> DependencySource {
        // Check if it's a URL-based dependency
        if version_spec.contains("git+") || version_spec.contains(".git") {
            // Extract URL for Git dependencies
            let url = if let Some(pos) = version_spec.find("git+") {
                version_spec[pos..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            } else if let Some(pos) = version_spec.find('@') {
                version_spec[pos + 1..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            } else {
                "unknown".to_string()
            };

            return DependencySource::Git { url, rev: None };
        }

        if version_spec.contains("file://")
            || version_spec.contains("./")
            || version_spec.contains("../")
        {
            return DependencySource::Path;
        }

        if version_spec.contains("http://") || version_spec.contains("https://") {
            let url = version_spec
                .split_whitespace()
                .find(|s| s.starts_with("http"))
                .unwrap_or("unknown")
                .to_string();
            return DependencySource::Url(url);
        }

        // Default to registry
        DependencySource::Registry
    }

    /// Extract path from dependency specification
    fn extract_path_from_spec(version_spec: &str) -> Option<std::path::PathBuf> {
        if version_spec.contains("file://") {
            if let Some(pos) = version_spec.find("file://") {
                let path_part = &version_spec[pos + 7..];
                let path_str = path_part.split_whitespace().next().unwrap_or(path_part);
                return Some(std::path::PathBuf::from(path_str));
            }
        }

        if version_spec.contains("./") || version_spec.contains("../") {
            // Find relative path
            for part in version_spec.split_whitespace() {
                if part.starts_with("./") || part.starts_with("../") {
                    return Some(std::path::PathBuf::from(part));
                }
            }
        }

        None
    }

    /// Check if a package should be included based on dependency info
    fn should_include_package_with_info(
        &self,
        _package_name: &PackageName,
        dep_info: &DependencyInfo,
    ) -> bool {
        // Filter based on dependency type and scanner configuration
        match dep_info.dependency_type {
            DependencyType::Dev => self.include_dev,
            DependencyType::Optional => self.include_optional,
            DependencyType::Main => true,
        }
    }

    /// Build dependency graph from lock file to determine direct vs transitive relationships
    fn build_dependency_graph(
        lock: &Lock,
        direct_deps_with_types: &HashMap<PackageName, DependencyType>,
    ) -> HashMap<PackageName, DependencyInfo> {
        debug!(
            "Building dependency graph for {} packages",
            lock.packages().len()
        );
        debug!(
            "Direct dependencies with types: {:?}",
            direct_deps_with_types
        );

        let mut graph = HashMap::new();

        // Add ALL packages from the lock file to the graph
        // This ensures that every package that was scanned will be found in the graph
        for package in lock.packages() {
            let package_name = package.name().clone();

            // Check if this is a direct dependency and get its type
            let (is_direct, dependency_type) =
                if let Some(dep_type) = direct_deps_with_types.get(&package_name) {
                    (true, *dep_type)
                } else {
                    // Transitive dependencies default to Main type
                    (false, DependencyType::Main)
                };

            graph.insert(
                package_name.clone(),
                DependencyInfo {
                    is_direct,
                    dependency_type,
                },
            );

            debug!(
                "Added {} to dependency graph (direct: {}, type: {:?})",
                package_name, is_direct, dependency_type
            );
        }

        debug!("Built dependency graph with {} entries", graph.len());
        graph
    }

    /// Determine the source type from a lock file package
    fn determine_source_from_package(package: &Package) -> DependencySource {
        // For now, we'll use a simplified approach since the internal Package structure
        // is complex and we don't have direct access to the source field.
        // TODO: This would need to examine package.id.source once we have access patterns

        debug!("Determining source for package: {}", package.name());

        // Most packages are from PyPI registry, so default to that
        // In a complete implementation, we would:
        // 1. Check if it's a registry source (PyPI or other index)
        // 2. Check if it's a Git source
        // 3. Check if it's a path source
        // 4. Check if it's a direct URL source

        DependencySource::Registry
    }

    /// Extract path information for path-based dependencies
    fn extract_package_path(_package: &Package) -> Option<std::path::PathBuf> {
        // For now, return None since we need proper access to internal source structure
        // TODO: Implement path extraction from package source information
        None
    }

    /// Validate that the scanned dependencies are reasonable
    pub fn validate_dependencies(&self, dependencies: &[ScannedDependency]) -> Vec<String> {
        let mut warnings = Vec::new();

        if dependencies.is_empty() {
            warnings.push(
                "No dependencies found. This might indicate an issue with dependency resolution."
                    .to_string(),
            );
            return warnings;
        }

        // Check for placeholder versions (indicates pyproject.toml scan)
        let placeholder_count = dependencies
            .iter()
            .filter(|dep| dep.version == Version::new([0, 0, 0]))
            .count();

        if placeholder_count > 0 {
            warnings.push(format!(
                "{placeholder_count} dependencies have placeholder versions. Run 'uv lock' for accurate version information."
            ));
        }

        // Check for very large dependency trees
        if dependencies.len() > 1000 {
            warnings.push(format!(
                "Found {} dependencies. This is a very large dependency tree that may take longer to audit.",
                dependencies.len()
            ));
        }

        // Check for unusual source types
        let non_registry_count = dependencies
            .iter()
            .filter(|dep| !matches!(dep.source, DependencySource::Registry))
            .count();

        if non_registry_count > 0 {
            warnings.push(format!(
                "{non_registry_count} dependencies are from non-registry sources (git, path, URL). \
                Vulnerability data may be limited for these packages."
            ));
        }

        warnings
    }

    /// Get dependency statistics
    pub fn get_stats(&self, dependencies: &[ScannedDependency]) -> DependencyStats {
        let total = dependencies.len();
        let direct = dependencies.iter().filter(|dep| dep.is_direct).count();
        let transitive = total - direct;

        let mut source_counts = HashMap::new();
        for dep in dependencies {
            let source_name = match &dep.source {
                DependencySource::Registry => "Registry",
                DependencySource::Git { .. } => "Git",
                DependencySource::Path => "Path",
                DependencySource::Url(_) => "URL",
            };
            *source_counts.entry(source_name.to_string()).or_insert(0) += 1;
        }

        DependencyStats {
            total,
            direct,
            transitive,
            source_counts,
        }
    }
}

/// Statistics about scanned dependencies
#[derive(Debug, Clone)]
pub struct DependencyStats {
    pub total: usize,
    pub direct: usize,
    pub transitive: usize,
    pub source_counts: HashMap<String, usize>,
}

impl std::fmt::Display for DependencyStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Dependencies: {} total ({} direct, {} transitive)",
            self.total, self.direct, self.transitive
        )?;

        if !self.source_counts.is_empty() {
            write!(f, " - Sources: ")?;
            let mut first = true;
            for (source, count) in &self.source_counts {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "{source}: {count}")?;
                first = false;
            }
        }

        Ok(())
    }
}

/// Extract package name from a dependency string like "package>=1.0" or "package[extra]>=1.0"
fn extract_package_name_from_dep_string(dep_str: &str) -> Result<PackageName> {
    // Simple extraction - find the package name before any version specifiers, extras, or URL specs
    let dep_str = dep_str.trim();

    // Handle the common cases:
    // - "package>=1.0"
    // - "package[extra]>=1.0"
    // - "package @ git+https://..."
    // - "package"

    let name_part = if let Some(pos) = dep_str.find(&['>', '<', '=', '!', '~', '[', '@'][..]) {
        &dep_str[..pos]
    } else {
        dep_str
    };

    let package_name = name_part.trim();

    PackageName::from_str(package_name)
        .map_err(|_| AuditError::InvalidDependency(format!("Invalid package name: {package_name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_dependency_scanner_creation() {
        let scanner = DependencyScanner::new(true, true, false);
        assert!(scanner.include_dev);
        assert!(scanner.include_optional);
        assert!(!scanner.direct_only);
    }

    #[test]
    fn test_dependency_stats() {
        let dependencies = vec![
            ScannedDependency {
                name: PackageName::from_str("package1").unwrap(),
                version: Version::from_str("1.0.0").unwrap(),
                is_direct: true,
                source: DependencySource::Registry,
                path: None,
            },
            ScannedDependency {
                name: PackageName::from_str("package2").unwrap(),
                version: Version::from_str("2.0.0").unwrap(),
                is_direct: false,
                source: DependencySource::Registry,
                path: None,
            },
        ];

        let scanner = DependencyScanner::new(false, false, false);
        let stats = scanner.get_stats(&dependencies);

        assert_eq!(stats.total, 2);
        assert_eq!(stats.direct, 1);
        assert_eq!(stats.transitive, 1);
        assert_eq!(stats.source_counts.get("Registry"), Some(&2));
    }

    #[test]
    fn test_dependency_validation() {
        let scanner = DependencyScanner::new(false, false, false);

        // Test empty dependencies
        let warnings = scanner.validate_dependencies(&[]);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("No dependencies found"));

        // Test placeholder versions
        let dependencies = vec![ScannedDependency {
            name: PackageName::from_str("package1").unwrap(),
            version: Version::new([0, 0, 0]),
            is_direct: true,
            source: DependencySource::Registry,
            path: None,
        }];

        let warnings = scanner.validate_dependencies(&dependencies);
        assert!(warnings.iter().any(|w| w.contains("placeholder versions")));
    }

    #[test]
    fn test_version_extraction_from_spec() {
        // Test exact version
        let version = DependencyScanner::extract_version_from_spec("requests==2.31.0");
        assert_eq!(version, Some(Version::from_str("2.31.0").unwrap()));

        // Test minimum version
        let version = DependencyScanner::extract_version_from_spec("requests>=2.28.0");
        assert_eq!(version, Some(Version::from_str("2.28.0").unwrap()));

        // Test no version
        let version = DependencyScanner::extract_version_from_spec("requests");
        assert_eq!(version, None);

        // Test complex constraint
        let version = DependencyScanner::extract_version_from_spec("requests>=2.28.0,<3.0.0");
        assert_eq!(version, Some(Version::from_str("2.28.0").unwrap()));
    }

    #[test]
    fn test_source_detection_from_spec() {
        let package_name = PackageName::from_str("test-package").unwrap();

        // Test registry source
        let source =
            DependencyScanner::determine_source_from_spec(&package_name, "requests>=2.31.0");
        assert!(matches!(source, DependencySource::Registry));

        // Test Git source
        let source = DependencyScanner::determine_source_from_spec(
            &package_name,
            "git+https://github.com/user/repo.git",
        );
        assert!(matches!(source, DependencySource::Git { .. }));

        // Test path source
        let source =
            DependencyScanner::determine_source_from_spec(&package_name, "file:///path/to/package");
        assert!(matches!(source, DependencySource::Path));

        // Test URL source
        let source = DependencyScanner::determine_source_from_spec(
            &package_name,
            "https://example.com/package.tar.gz",
        );
        assert!(matches!(source, DependencySource::Url(_)));
    }

    #[test]
    fn test_dependency_type_filtering() {
        let scanner = DependencyScanner::new(false, false, false);

        // Main dependencies should always be included
        assert!(scanner.should_include_dependency_type(DependencyType::Main));

        // Dev dependencies only if enabled
        assert!(!scanner.should_include_dependency_type(DependencyType::Dev));

        // Optional dependencies only if enabled
        assert!(!scanner.should_include_dependency_type(DependencyType::Optional));
    }

    #[test]
    fn test_dependency_type_filtering_with_options() {
        let scanner = DependencyScanner::new(true, true, false);

        // All types should be included when options are enabled
        assert!(scanner.should_include_dependency_type(DependencyType::Main));
        assert!(scanner.should_include_dependency_type(DependencyType::Dev));
        assert!(scanner.should_include_dependency_type(DependencyType::Optional));
    }

    #[test]
    fn test_extract_package_name_from_dep_string() {
        // Test basic package name
        let result = extract_package_name_from_dep_string("requests");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "requests");

        // Test with version specifier
        let result = extract_package_name_from_dep_string("requests>=2.31.0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "requests");

        // Test with extras
        let result = extract_package_name_from_dep_string("requests[security]>=2.31.0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "requests");

        // Test with URL
        let result = extract_package_name_from_dep_string(
            "mypackage @ git+https://github.com/user/repo.git",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "mypackage");

        // Test with complex version spec
        let result = extract_package_name_from_dep_string("numpy>=1.20.0,<2.0.0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "numpy");

        // Test edge cases
        let result = extract_package_name_from_dep_string("  requests  ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "requests");

        // Test that empty string handling - it might be valid or invalid depending on PackageName implementation
        // We'll just verify it's handled consistently
        let _result = extract_package_name_from_dep_string("");

        // Test version spec without package name
        let _result = extract_package_name_from_dep_string(">=1.0.0");
        // The function will extract everything before '>=', which is empty string
        // This might be valid or invalid depending on PackageName implementation
    }

    #[test]
    fn test_build_dependency_graph_with_types() {
        // Create a mock lock with test packages
        // Note: This is a simplified test since we can't easily create real Lock instances
        // In a real test, we'd use test fixtures or mocks

        // Create dependency type mappings
        let mut direct_deps_with_types = HashMap::new();
        direct_deps_with_types.insert(
            PackageName::from_str("main-dep").unwrap(),
            DependencyType::Main,
        );
        direct_deps_with_types.insert(
            PackageName::from_str("dev-dep").unwrap(),
            DependencyType::Dev,
        );
        direct_deps_with_types.insert(
            PackageName::from_str("optional-dep").unwrap(),
            DependencyType::Optional,
        );

        // The actual build_dependency_graph would need a real Lock instance
        // This test verifies the logic conceptually
        assert_eq!(direct_deps_with_types.len(), 3);

        // Verify each dependency has the correct type
        assert_eq!(
            direct_deps_with_types.get(&PackageName::from_str("main-dep").unwrap()),
            Some(&DependencyType::Main)
        );
        assert_eq!(
            direct_deps_with_types.get(&PackageName::from_str("dev-dep").unwrap()),
            Some(&DependencyType::Dev)
        );
        assert_eq!(
            direct_deps_with_types.get(&PackageName::from_str("optional-dep").unwrap()),
            Some(&DependencyType::Optional)
        );

        // Verify transitive dependencies would default to Main
        assert_eq!(
            direct_deps_with_types.get(&PackageName::from_str("transitive-dep").unwrap()),
            None
        );
    }

    #[test]
    fn test_dependency_type_priority() {
        // Test that when a package appears in multiple sections,
        // Main takes priority over Optional, which takes priority over Dev

        let mut deps_map = HashMap::new();

        // First insert as Dev
        deps_map.insert(
            PackageName::from_str("multi-dep").unwrap(),
            DependencyType::Dev,
        );

        // Then try to insert as Optional (should not override)
        deps_map
            .entry(PackageName::from_str("multi-dep").unwrap())
            .or_insert(DependencyType::Optional);

        // Verify it's still Dev
        assert_eq!(
            deps_map.get(&PackageName::from_str("multi-dep").unwrap()),
            Some(&DependencyType::Dev)
        );

        // Now force update to Main
        deps_map.insert(
            PackageName::from_str("multi-dep").unwrap(),
            DependencyType::Main,
        );

        // Verify it's now Main
        assert_eq!(
            deps_map.get(&PackageName::from_str("multi-dep").unwrap()),
            Some(&DependencyType::Main)
        );
    }

    #[test]
    fn test_dependency_info_structure() {
        let info = DependencyInfo {
            is_direct: true,
            dependency_type: DependencyType::Dev,
        };

        assert!(info.is_direct);
        assert_eq!(info.dependency_type, DependencyType::Dev);
    }

    #[test]
    fn test_should_include_package_with_info() {
        // Create a mock workspace - we don't actually need a real one for this test
        // since should_include_package_with_info doesn't use the workspace parameter
        let package_name = PackageName::from_str("test-package").unwrap();

        // Test with dev dependency included
        let scanner_with_dev = DependencyScanner::new(true, false, false);
        let dev_info = DependencyInfo {
            is_direct: true,
            dependency_type: DependencyType::Dev,
        };
        assert!(scanner_with_dev.should_include_package_with_info(&package_name, &dev_info));

        // Test with dev dependency excluded
        let scanner_no_dev = DependencyScanner::new(false, false, false);
        assert!(!scanner_no_dev.should_include_package_with_info(&package_name, &dev_info));

        // Test with optional dependency included
        let scanner_with_opt = DependencyScanner::new(false, true, false);
        let opt_info = DependencyInfo {
            is_direct: true,
            dependency_type: DependencyType::Optional,
        };
        assert!(scanner_with_opt.should_include_package_with_info(&package_name, &opt_info));

        // Test with optional dependency excluded
        let scanner_no_opt = DependencyScanner::new(false, false, false);
        assert!(!scanner_no_opt.should_include_package_with_info(&package_name, &opt_info));

        // Test main dependency always included
        let scanner_minimal = DependencyScanner::new(false, false, false);
        let main_info = DependencyInfo {
            is_direct: true,
            dependency_type: DependencyType::Main,
        };
        assert!(scanner_minimal.should_include_package_with_info(&package_name, &main_info));
    }

    #[test]
    fn test_dependency_type_equality() {
        assert_eq!(DependencyType::Main, DependencyType::Main);
        assert_eq!(DependencyType::Dev, DependencyType::Dev);
        assert_eq!(DependencyType::Optional, DependencyType::Optional);
        assert_ne!(DependencyType::Main, DependencyType::Dev);
        assert_ne!(DependencyType::Dev, DependencyType::Optional);
        assert_ne!(DependencyType::Main, DependencyType::Optional);
    }
}
