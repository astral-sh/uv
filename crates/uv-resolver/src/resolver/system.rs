use std::str::FromStr;

use pubgrub::Ranges;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_redacted::DisplaySafeUrl;
use uv_torch::TorchBackend;

use crate::pubgrub::{PubGrubDependency, PubGrubPackage, PubGrubPackageInner};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SystemDependency {
    /// The name of the system dependency (e.g., `cuda`).
    name: PackageName,
    /// The version of the system dependency (e.g., `12.4`).
    version: Version,
}

impl SystemDependency {
    /// Extract a [`SystemDependency`] from an index URL.
    ///
    /// For example, given `https://download.pytorch.org/whl/cu124`, returns CUDA 12.4.
    pub(super) fn from_index(index: &DisplaySafeUrl) -> Option<Self> {
        let backend = TorchBackend::from_index(index)?;
        let cuda_version = backend.cuda_version()?;
        Some(Self {
            name: PackageName::from_str("cuda").unwrap(),
            version: cuda_version,
        })
    }
}

impl std::fmt::Display for SystemDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

impl From<SystemDependency> for PubGrubDependency {
    fn from(value: SystemDependency) -> Self {
        PubGrubDependency {
            package: PubGrubPackage::from(PubGrubPackageInner::System(value.name)),
            version: Ranges::singleton(value.version),
            url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_redacted::DisplaySafeUrl;

    use crate::resolver::system::SystemDependency;

    #[test]
    fn pypi() {
        let url = DisplaySafeUrl::parse("https://pypi.org/simple").unwrap();
        assert_eq!(SystemDependency::from_index(&url), None);
    }

    #[test]
    fn pytorch_cuda_12_4() {
        let url = DisplaySafeUrl::parse("https://download.pytorch.org/whl/cu124").unwrap();
        assert_eq!(
            SystemDependency::from_index(&url),
            Some(SystemDependency {
                name: PackageName::from_str("cuda").unwrap(),
                version: Version::new([12, 4]),
            })
        );
    }

    #[test]
    fn pytorch_cpu() {
        let url = DisplaySafeUrl::parse("https://download.pytorch.org/whl/cpu").unwrap();
        assert_eq!(SystemDependency::from_index(&url), None);
    }
}
