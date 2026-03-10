//! Types for interacting with dependency audits.

use jiff::Timestamp;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_small_str::SmallString;

/// Represents a resolved dependency, with a normalized name and PEP 440 version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    name: PackageName,
    version: Version,
}

impl Dependency {
    /// Create a new dependency with the given name and version.
    pub fn new(name: PackageName, version: Version) -> Self {
        Self { name, version }
    }

    /// Get the package name.
    pub fn name(&self) -> &PackageName {
        &self.name
    }

    /// Get the version.
    pub fn version(&self) -> &Version {
        &self.version
    }
}

/// An opaque identifier for a vulnerability. These are conventionally
/// formatted as `SRC-XXXX-YYYY`, where `SRC` is an identifier for the vulnerability source,
/// `XXXX` is typically a year or other "bucket" identifier, and `YYYY` is a unique identifier
/// within that bucket. For example, `CVE-2026-12345` or `PYSEC-2023-0001`.
///
/// No assumptions should be made about the format of these identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulnerabilityID(SmallString);

impl VulnerabilityID {
    /// Create a new vulnerability ID from a string.
    pub(crate) fn new(id: impl Into<SmallString>) -> Self {
        Self(id.into())
    }

    /// Get the string representation of this vulnerability ID.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// Represents an "adverse" project status, i.e. a status that indicates that
/// a downstream user of the project should review their use of the project
/// and consider removing it.
///
/// These are a subset of the possible project statuses defined in [PEP 792].
///
/// [PEP 792]: https://peps.python.org/pep-0792/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdverseStatus {
    /// The project is archived, meaning it is read-only and no longer maintained.
    Archived,
    /// The project is considered generally unsafe for use, e.g. due to malware.
    Quarantined,
    /// The project is considered obsolete, and may have been superseded by another project.
    Deprecated,
}

/// A vulnerability within a dependency.
#[derive(Debug)]
pub struct Vulnerability {
    /// The dependency that is vulnerable.
    pub dependency: Dependency,
    /// The unique identifier for the vulnerability.
    pub id: VulnerabilityID,
    /// A short, human-readable summary of the vulnerability, if available.
    pub summary: Option<String>,
    /// A full-length description of the vulnerability, if available.
    pub description: Option<String>,
    /// Zero or more versions that fix the vulnerability.
    pub fix_versions: Vec<Version>,
    /// Zero or more aliases for this vulnerability in other databases.
    pub aliases: Vec<VulnerabilityID>,
    /// The timestamp when this vulnerability was published, if available.
    pub published: Option<Timestamp>,
    /// The timestamp when this vulnerability was last modified, if available.
    pub modified: Option<Timestamp>,
}

impl Vulnerability {
    pub fn new(
        dependency: Dependency,
        id: VulnerabilityID,
        summary: Option<String>,
        description: Option<String>,
        fix_versions: Vec<Version>,
        aliases: Vec<VulnerabilityID>,
        published: Option<Timestamp>,
        modified: Option<Timestamp>,
    ) -> Self {
        // Vulnerability summaries often contain excess whitespace, as well as newlines.
        // We normalize these out.
        let summary = summary.map(|summary| summary.trim().replace('\n', ""));

        Self {
            dependency,
            id,
            summary,
            description,
            fix_versions,
            aliases,
            published,
            modified,
        }
    }

    /// Pick the subjectively "best" identifier for this vulnerability.
    /// For our purposes we prefer PYSEC IDs, then GHSA, then CVE, then whatever
    /// primary ID the vulnerability came with.
    pub fn best_id(&self) -> &VulnerabilityID {
        std::iter::once(&self.id)
            .chain(self.aliases.iter())
            .find(|id| {
                id.as_str().starts_with("PYSEC-")
                    || id.as_str().starts_with("GHSA-")
                    || id.as_str().starts_with("CVE-")
            })
            .unwrap_or(&self.id)
    }
}

/// An adverse project status, such as an archived or deprecated project.
#[derive(Debug)]
pub struct ProjectStatus {
    /// The dependency with the adverse status.
    pub dependency: Dependency,
    /// The adverse status of the project.
    pub status: AdverseStatus,
    /// An optional (index-supplied) reason for the adverse status.
    pub reason: Option<String>,
}

/// Represents a finding on a dependency.
#[derive(Debug)]
pub enum Finding {
    Vulnerability(Vulnerability),
    ProjectStatus(ProjectStatus),
}
