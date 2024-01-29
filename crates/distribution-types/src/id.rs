use std::fmt::{Display, Formatter};

/// A unique identifier for a package (e.g., `black==23.10.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageId(String);

impl PackageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A unique identifier for a distribution (e.g., `black-23.10.0-py3-none-any.whl`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DistributionId(String);

impl DistributionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl DistributionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A unique identifier for a resource, like a URL or a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&PackageId> for PackageId {
    /// Required for `WaitMap::wait`.
    fn from(value: &PackageId) -> Self {
        value.clone()
    }
}

impl From<&DistributionId> for DistributionId {
    /// Required for `WaitMap::wait`.
    fn from(value: &DistributionId) -> Self {
        value.clone()
    }
}

impl From<&ResourceId> for ResourceId {
    /// Required for `WaitMap::wait`.
    fn from(value: &ResourceId) -> Self {
        value.clone()
    }
}
