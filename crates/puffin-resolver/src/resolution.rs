use std::collections::BTreeMap;
use std::io;
use std::io::Write;

use pep440_rs::Version;
use puffin_client::File;
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;

#[derive(Debug, Default)]
pub struct Resolution(BTreeMap<PackageName, PinnedPackage>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub(crate) fn new(packages: BTreeMap<PackageName, PinnedPackage>) -> Self {
        Self(packages)
    }

    /// Iterate over the pinned packages in this resolution.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &PinnedPackage)> {
        self.0.iter()
    }

    /// Iterate over the wheels in this resolution.
    pub fn into_files(self) -> impl Iterator<Item = File> {
        self.0.into_values().map(|package| package.file)
    }

    /// Return the number of pinned packages in this resolution.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Write the resolution in the `{name}=={version}` format of requirements.txt that pip uses.
    pub fn write_requirement_format(&self, writer: &mut impl Write) -> io::Result<()> {
        for (name, package) in self.iter() {
            writeln!(writer, "{}=={}", name, package.version())?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for (name, package) in self.iter() {
            if !first {
                writeln!(f)?;
            }
            first = false;
            write!(f, "{}=={}", name, package.version())?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PinnedPackage {
    pub metadata: Metadata21,
    pub file: File,
}

impl PinnedPackage {
    pub fn version(&self) -> &Version {
        &self.metadata.version
    }
}
