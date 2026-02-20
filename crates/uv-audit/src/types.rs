//! Types for interacting with dependency audits.

use jiff::Timestamp;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_small_str::SmallString;

/// Represents a resolved dependency, with a normalized name and PEP 440 version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency<'a> {
    name: &'a PackageName,
    version: &'a Version,
}

impl<'a> Dependency<'a> {
    /// Create a new dependency with the given name and version.
    pub fn new(name: &'a PackageName, version: &'a Version) -> Self {
        Self { name, version }
    }

    /// Get the package name.
    pub fn name(&self) -> &PackageName {
        self.name
    }

    /// Get the version.
    pub fn version(&self) -> &Version {
        self.version
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

/// Represents a finding on a dependency.
#[derive(Debug)]
pub enum Finding<'a> {
    /// A vulnerability within a dependency.
    Vulnerability {
        /// The dependency that is vulnerable.
        dependency: Dependency<'a>,
        /// The unique identifier for the vulnerability.
        id: VulnerabilityID,
        /// A short, human-readable description of the vulnerability.
        description: String,
        /// Zero or more versions that fix the vulnerability.
        fix_versions: Vec<Version>,
        /// Zero or more aliases for this vulnerability in other databases.
        aliases: Vec<VulnerabilityID>,
        /// The timestamp when this vulnerability was published, if available.
        published: Option<Timestamp>,
        /// The timestamp when this vulnerability was last modified, if available.
        modified: Option<Timestamp>,
    },
    /// An adverse project status, such as an archived or deprecated project.
    ProjectStatus {
        /// The dependency with the adverse status.
        dependency: Dependency<'a>,
        /// The adverse status of the project.
        status: AdverseStatus,
    },
}
