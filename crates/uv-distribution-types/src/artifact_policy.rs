use std::fmt::{Display, Formatter};
use std::sync::Arc;

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::{MacosDeploymentTarget, MacosPlatformTags, PlatformError};

use crate::{MinimumLibcVersion, PinnedHashSource};

/// Required platform baselines and explicit libc exclusions for universal resolution.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ArtifactPolicy {
    pub(crate) libc: Option<MinimumLibcVersion>,
    pub(crate) macos: Option<Arc<MacosPlatformTags>>,
}

impl ArtifactPolicy {
    /// Hash all retained files when platform baselines or libc exclusions are configured.
    pub(crate) fn hash_source(&self) -> PinnedHashSource {
        if self.libc.is_some() || self.macos.is_some() {
            PinnedHashSource::Artifacts
        } else {
            PinnedHashSource::Package
        }
    }

    /// Prepare platform tags once, instead of regenerating them for each candidate wheel.
    pub fn new(
        libc: Option<MinimumLibcVersion>,
        macos: Option<MacosDeploymentTarget>,
    ) -> Result<Self, PlatformError> {
        Ok(Self {
            libc,
            macos: macos.map(MacosPlatformTags::new).transpose()?.map(Arc::new),
        })
    }

    /// Retain newer wheels while respecting explicit libc exclusions.
    pub fn allows_wheel(&self, filename: &WheelFilename) -> bool {
        self.libc.is_none_or(|libc| libc.allows_wheel(filename))
    }
}

impl Display for ArtifactPolicy {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(libc) = self.libc {
            write!(formatter, "{libc}")?;
            if self.macos.is_some() {
                formatter.write_str(" and ")?;
            }
        }
        if let Some(macos) = &self.macos {
            write!(formatter, "macOS {}", macos.deployment_target())?;
        }
        Ok(())
    }
}
