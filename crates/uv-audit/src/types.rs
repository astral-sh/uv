//! Types for interacting with dependency audits.

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_small_str::SmallString;

/// Represents a resolved dependency, with a normalized name and PEP 440 version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency<'a> {
    name: &'a PackageName,
    version: &'a Version,
}

/// An opaque identifier for a vulnerability. These are conventionally
/// formatted as `SRC-XXXX-YYYY`, where `SRC` is an identifier for the vulnerability source,
/// `XXXX` is typically a year or other "bucket" identifier, and `YYYY` is a unique identifier
/// within that bucket. For example, `CVE-2026-12345` or `PYSEC-2023-0001`.
///
/// No assumptions should be made about the format of these identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulnerabilityID(SmallString);

/// Represents an "adverse" project status, i.e. a status that indicates that
/// a downstream user of the project should review their use of the project
/// and consider removing it.
///
/// These are a subset of the possible project statuses defined in [PEP 792].
///
/// [PEP 792]: https://peps.python.org/pep-0792/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdverseStatus {
    Archived,
    Quarantined,
    Deprecated,
}

/// Represents a finding on a dependency.
pub enum Finding<'a> {
    /// A vulnerability within a dependency.
    Vulnerability {
        dependency: Dependency<'a>,
        id: VulnerabilityID,
        description: String,
        fix_versions: Vec<Version>,
        aliases: Vec<VulnerabilityID>,
        published: String,
    },
    /// An adverse project status, such as an archived or deprecated project.
    ProjectStatus {
        dependency: Dependency<'a>,
        status: AdverseStatus,
    },
}
