use std::fmt;
use std::ops::Deref;

use uv_normalize::PackageName;
use uv_pep440::Version;

use super::{LockError, LockErrorKind, RegistrySource, Source};

/// The permissive identity representation read from the lockfile.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PackageIdWire {
    pub(super) name: PackageName,
    pub(super) version: Option<Version>,
    pub(super) source: Source,
}

/// A validated lockfile identity. Registry packages always have a version.
///
/// The inner data can be inspected but cannot be mutated without constructing and validating a
/// new identity. Deserialization is restricted to [`PackageIdWire`].
#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) struct PackageId(PackageIdWire);

impl TryFrom<PackageIdWire> for PackageId {
    type Error = LockError;

    fn try_from(wire: PackageIdWire) -> Result<Self, Self::Error> {
        if matches!(wire.source, Source::Registry(_)) && wire.version.is_none() {
            return Err(LockErrorKind::MissingPackageVersion { name: wire.name }.into());
        }
        Ok(Self(wire))
    }
}

impl PackageId {
    pub(super) fn into_wire(self) -> PackageIdWire {
        self.0
    }

    /// Return a registry identity with its required version already available.
    pub(super) fn registry(&self) -> Option<RegistryPackageId<'_>> {
        match (&self.source, &self.version) {
            (Source::Registry(source), Some(version)) => Some(RegistryPackageId {
                name: &self.name,
                version,
                source,
            }),
            _ => None,
        }
    }
}

/// A registry package's identity, including the version required by registry artifacts.
pub(super) struct RegistryPackageId<'a> {
    pub(super) name: &'a PackageName,
    pub(super) version: &'a Version,
    pub(super) source: &'a RegistrySource,
}

// Read-only access preserves the invariant checked by `TryFrom<PackageIdWire>`.
impl Deref for PackageId {
    type Target = PackageIdWire;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PackageId")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("source", &self.source)
            .finish()
    }
}
