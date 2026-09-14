use uv_distribution_filename::WheelFilename;
use uv_pep508::MarkerTree;
use uv_platform_tags::{GlibcVersion, PlatformTag};

use crate::prioritized_distribution::{implied_platform_markers, implied_python_markers};

/// Constraints on the artifacts included in a universal resolution.
///
/// The same policy determines wheel eligibility and the environments that an eligible wheel
/// covers. These differ for wheels with multiple platform tags: one allowed tag is enough to
/// retain the artifact, but disallowed tags must not contribute environment coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct ArtifactPolicy {
    minimum_glibc_version: Option<GlibcVersion>,
}

impl ArtifactPolicy {
    /// Create a policy for the oldest supported glibc release.
    pub fn new(minimum_glibc_version: Option<GlibcVersion>) -> Self {
        Self {
            minimum_glibc_version,
        }
    }

    /// Return whether the policy leaves all artifacts eligible.
    pub fn is_empty(self) -> bool {
        self.minimum_glibc_version.is_none()
    }

    /// Check whether any of the wheel's platform tags satisfy the policy.
    pub fn check_wheel(self, filename: &WheelFilename) -> Result<(), ArtifactPolicyError> {
        if let Some(version) = self.minimum_glibc_version
            && !filename
                .platform_tags()
                .iter()
                .any(|tag| self.allows_platform(tag))
        {
            return Err(ArtifactPolicyError::GlibcVersion(version));
        }
        Ok(())
    }

    /// Return the environments covered by the wheel's allowed platform tags.
    pub fn wheel_coverage(self, filename: &WheelFilename) -> MarkerTree {
        implied_platform_markers(
            filename
                .platform_tags()
                .iter()
                .filter(|tag| self.allows_platform(tag)),
        )
        .and(implied_python_markers(filename))
    }

    fn allows_platform(self, platform: &PlatformTag) -> bool {
        let Some(version) = self.minimum_glibc_version else {
            return true;
        };
        match platform {
            PlatformTag::Manylinux { major, minor, .. } => {
                GlibcVersion::new(*major, *minor) <= version
            }
            PlatformTag::Manylinux1 { .. } => GlibcVersion::new(2, 5) <= version,
            PlatformTag::Manylinux2010 { .. } => GlibcVersion::new(2, 12) <= version,
            PlatformTag::Manylinux2014 { .. } => GlibcVersion::new(2, 17) <= version,
            PlatformTag::Musllinux { .. } => false,
            // Native Linux tags do not declare a libc requirement.
            PlatformTag::Linux { .. }
            | PlatformTag::Any
            | PlatformTag::Macos { .. }
            | PlatformTag::Win32
            | PlatformTag::WinAmd64
            | PlatformTag::WinArm64
            | PlatformTag::WinIa64
            | PlatformTag::Android { .. }
            | PlatformTag::FreeBsd { .. }
            | PlatformTag::NetBsd { .. }
            | PlatformTag::OpenBsd { .. }
            | PlatformTag::Dragonfly { .. }
            | PlatformTag::Haiku { .. }
            | PlatformTag::Illumos { .. }
            | PlatformTag::Solaris { .. }
            | PlatformTag::Pyodide { .. }
            | PlatformTag::PyEmscripten { .. }
            | PlatformTag::Ios { .. } => true,
        }
    }
}

/// A wheel excluded by the universal resolution's artifact policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactPolicyError {
    #[error("no wheels compatible with glibc {0}")]
    GlibcVersion(GlibcVersion),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_pep508::MarkerTree;
    use uv_platform_tags::GlibcVersion;

    use super::ArtifactPolicy;
    use crate::implied_markers;

    #[test]
    fn glibc_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let minimum_glibc_version = GlibcVersion::new(2, 31);
        for platform in [
            "any",
            "manylinux_2_31_x86_64",
            "manylinux_2_17_aarch64",
            "manylinux1_x86_64",
            "manylinux2010_x86_64",
            "manylinux2014_x86_64",
            "linux_x86_64",
            "win_amd64",
            "macosx_11_0_arm64",
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert_eq!(
                ArtifactPolicy::new(Some(minimum_glibc_version)).wheel_coverage(&filename),
                implied_markers(&filename),
                "{platform}",
            );
        }

        for platform in ["manylinux_2_34_x86_64", "musllinux_1_2_x86_64"] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert_eq!(
                ArtifactPolicy::new(Some(minimum_glibc_version)).wheel_coverage(&filename),
                MarkerTree::FALSE,
                "{platform}",
            );
            assert!(!implied_markers(&filename).is_false());
        }

        let filename = WheelFilename::from_str(
            "example-1.0-py3-none-manylinux_2_17_x86_64.manylinux_2_34_aarch64.win_amd64.whl",
        )?;
        let compatible =
            WheelFilename::from_str("example-1.0-py3-none-manylinux_2_17_x86_64.win_amd64.whl")?;
        assert_eq!(
            ArtifactPolicy::new(Some(minimum_glibc_version)).wheel_coverage(&filename),
            implied_markers(&compatible),
        );
        Ok(())
    }

    #[test]
    fn legacy_glibc_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        for (platform, minor) in [
            ("manylinux1_x86_64", 5),
            ("manylinux2010_x86_64", 12),
            ("manylinux2014_x86_64", 17),
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert_eq!(
                ArtifactPolicy::new(Some(GlibcVersion::new(2, minor))).wheel_coverage(&filename),
                implied_markers(&filename),
            );
            assert_eq!(
                ArtifactPolicy::new(Some(GlibcVersion::new(2, minor - 1)))
                    .wheel_coverage(&filename),
                MarkerTree::FALSE,
            );
        }
        Ok(())
    }

    #[test]
    fn wheel_eligibility() -> Result<(), Box<dyn std::error::Error>> {
        let policy = ArtifactPolicy::new(Some(GlibcVersion::new(2, 31)));
        for platform in [
            "manylinux_2_17_x86_64",
            "manylinux_2_17_x86_64.manylinux_2_34_aarch64",
            "win_amd64",
            "linux_x86_64",
            "freebsd_13_0_x86_64",
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert!(policy.check_wheel(&filename).is_ok(), "{platform}");
        }
        for platform in ["manylinux_2_34_x86_64", "musllinux_1_2_x86_64"] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert!(policy.check_wheel(&filename).is_err(), "{platform}");
            assert!(ArtifactPolicy::default().check_wheel(&filename).is_ok());
        }
        Ok(())
    }
}
