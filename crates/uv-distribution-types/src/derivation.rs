use uv_normalize::PackageName;
use uv_pep440::Version;

/// A chain of derivation steps from the root package to the current package, to explain why a
/// package is included in the resolution.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DerivationChain(Vec<DerivationStep>);

impl FromIterator<DerivationStep> for DerivationChain {
    fn from_iter<T: IntoIterator<Item = DerivationStep>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl DerivationChain {
    /// Returns the length of the derivation chain.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the derivation chain is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the steps in the derivation chain.
    pub fn iter(&self) -> std::slice::Iter<DerivationStep> {
        self.0.iter()
    }
}

impl std::fmt::Display for DerivationChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (idx, step) in self.0.iter().enumerate() {
            if idx > 0 {
                write!(f, " -> ")?;
            }
            write!(f, "{}=={}", step.name, step.version)?;
        }
        Ok(())
    }
}

impl<'chain> IntoIterator for &'chain DerivationChain {
    type Item = &'chain DerivationStep;
    type IntoIter = std::slice::Iter<'chain, DerivationStep>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for DerivationChain {
    type Item = DerivationStep;
    type IntoIter = std::vec::IntoIter<DerivationStep>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A step in a derivation chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DerivationStep {
    /// The name of the package.
    name: PackageName,
    /// The version of the package.
    version: Version,
}

impl DerivationStep {
    /// Create a [`DerivationStep`] from a package name and version.
    pub fn new(name: PackageName, version: Version) -> Self {
        Self { name, version }
    }
}

impl std::fmt::Display for DerivationStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=={}", self.name, self.version)
    }
}
