use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use uv_distribution_filename::WheelFilename;
use uv_pep508::MarkerTree;
use uv_platform_tags::{LibcVersion, PlatformTag};

use crate::prioritized_distribution::{implied_platform_markers, implied_python_markers};

/// The selected Linux libc families and their oldest supported releases.
///
/// At least one family must be specified. An omitted family is excluded, not unconstrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(extend("minProperties" = 1)))]
pub struct MinimumLibcVersion {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    glibc: Option<LibcVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    musl: Option<LibcVersion>,
}

impl<'de> Deserialize<'de> for MinimumLibcVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            glibc: Option<LibcVersion>,
            musl: Option<LibcVersion>,
        }

        let Wire { glibc, musl } = Wire::deserialize(deserializer)?;
        if glibc.is_none() && musl.is_none() {
            return Err(serde::de::Error::custom(
                "at least one of `glibc` or `musl` must be specified",
            ));
        }
        Ok(Self { glibc, musl })
    }
}

impl Display for MinimumLibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match (self.glibc, self.musl) {
            (Some(glibc), Some(musl)) => write!(formatter, "glibc {glibc} or musl {musl}"),
            (Some(glibc), None) => write!(formatter, "glibc {glibc}"),
            (None, Some(musl)) => write!(formatter, "musl {musl}"),
            (None, None) => formatter.write_str("the selected libc versions"),
        }
    }
}

/// Constraints on the artifacts included in a universal resolution.
///
/// The same policy determines wheel eligibility and the environments that an eligible wheel
/// covers. These differ for wheels with multiple platform tags: one allowed tag is enough to
/// retain the artifact, but disallowed tags must not contribute environment coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct ArtifactPolicy {
    minimum_libc_version: Option<MinimumLibcVersion>,
}

impl ArtifactPolicy {
    pub fn new(minimum_libc_version: MinimumLibcVersion) -> Self {
        Self {
            minimum_libc_version: Some(minimum_libc_version),
        }
    }

    pub fn is_empty(self) -> bool {
        self.minimum_libc_version.is_none()
    }

    /// Check artifact eligibility, retaining the entire wheel if any platform tag is allowed.
    ///
    /// This does not establish coverage of a required environment; use [`Self::wheel_coverage`]
    /// for that. An empty policy accepts every wheel.
    pub fn check_wheel(self, filename: &WheelFilename) -> Result<(), ArtifactPolicyError> {
        if let Some(version) = self.minimum_libc_version
            && !filename
                .platform_tags()
                .iter()
                .any(|tag| self.allows_platform(tag))
        {
            return Err(ArtifactPolicyError::LibcVersion(version));
        }
        Ok(())
    }

    /// Return the environments this single wheel covers for every selected libc family.
    ///
    /// Registry versions can use separate wheels for each family; their callers must accumulate
    /// per-family coverage across all compatible wheels before intersecting the families.
    /// Tags without a known marker mapping contribute no coverage, even when eligible.
    pub fn wheel_coverage(self, filename: &WheelFilename) -> MarkerTree {
        let mut coverage = ArtifactCoverage::EMPTY;
        coverage.insert_wheel(self, filename);
        coverage.markers(self)
    }

    /// Reject omitted libc families and releases newer than the selected family baseline.
    ///
    /// Native Linux tags declare no libc version, so accepting them does not establish a glibc
    /// compatibility guarantee. Non-Linux tags are unconstrained.
    fn allows_platform(self, platform: &PlatformTag) -> bool {
        let Some(version) = self.minimum_libc_version else {
            return true;
        };
        match platform {
            PlatformTag::Manylinux { major, minor, .. } => version
                .glibc
                .is_some_and(|version| LibcVersion::new(*major, *minor) <= version),
            PlatformTag::Manylinux1 { .. } => version
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 5) <= version),
            PlatformTag::Manylinux2010 { .. } => version
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 12) <= version),
            PlatformTag::Manylinux2014 { .. } => version
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 17) <= version),
            PlatformTag::Musllinux { major, minor, .. } => version
                .musl
                .is_some_and(|version| LibcVersion::new(*major, *minor) <= version),
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

/// Per-family unions of compatible artifact coverage. Intersect only after all wheels are added:
/// one manylinux wheel and one musllinux wheel can jointly satisfy a two-family policy.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactCoverage {
    glibc: MarkerTree,
    musl: MarkerTree,
}

impl ArtifactCoverage {
    pub(crate) const EMPTY: Self = Self {
        glibc: MarkerTree::FALSE,
        musl: MarkerTree::FALSE,
    };
    pub(crate) const UNIVERSAL: Self = Self {
        glibc: MarkerTree::TRUE,
        musl: MarkerTree::TRUE,
    };

    /// Add one compatible wheel without losing which libc family covers each Python/platform fork.
    pub(crate) fn insert_wheel(&mut self, policy: ArtifactPolicy, filename: &WheelFilename) {
        if self.glibc.is_true() && self.musl.is_true() {
            return;
        }
        let python = implied_python_markers(filename);
        if !self.glibc.is_true() {
            self.glibc = self.glibc.or(implied_platform_markers(
                filename.platform_tags().iter().filter(|tag| {
                    policy.allows_platform(tag) && !matches!(tag, PlatformTag::Musllinux { .. })
                }),
            )
            .and(python));
        }
        if !self.musl.is_true() {
            self.musl = self.musl.or(implied_platform_markers(
                filename.platform_tags().iter().filter(|tag| {
                    policy.allows_platform(tag)
                        && !matches!(
                            tag,
                            PlatformTag::Manylinux { .. }
                                | PlatformTag::Manylinux1 { .. }
                                | PlatformTag::Manylinux2010 { .. }
                                | PlatformTag::Manylinux2014 { .. }
                        )
                }),
            )
            .and(python));
        }
    }

    /// Require coverage for each selected family, after unioning the artifacts within each family.
    pub(crate) fn markers(self, policy: ArtifactPolicy) -> MarkerTree {
        match policy.minimum_libc_version {
            Some(MinimumLibcVersion {
                glibc: Some(_),
                musl: Some(_),
            }) => self.glibc.and(self.musl),
            Some(MinimumLibcVersion {
                glibc: Some(_),
                musl: None,
            }) => self.glibc,
            Some(MinimumLibcVersion {
                glibc: None,
                musl: Some(_),
            }) => self.musl,
            Some(MinimumLibcVersion {
                glibc: None,
                musl: None,
            })
            | None => self.glibc.or(self.musl),
        }
    }
}

/// A wheel excluded by the universal resolution's artifact policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactPolicyError {
    #[error("no wheels compatible with {0}")]
    LibcVersion(MinimumLibcVersion),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_pep508::MarkerTree;
    use uv_platform_tags::LibcVersion;

    use super::{ArtifactCoverage, ArtifactPolicy, MinimumLibcVersion};
    use crate::implied_markers;

    #[test]
    fn glibc_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let policy = ArtifactPolicy::new(MinimumLibcVersion {
            glibc: Some(LibcVersion::new(2, 31)),
            musl: None,
        });
        for platform in [
            "any",
            "manylinux_2_31_x86_64",
            "manylinux_2_17_x86_64",
            "manylinux_2_17_aarch64",
            "manylinux1_x86_64",
            "manylinux2010_x86_64",
            "manylinux2014_x86_64",
            "linux_x86_64",
            "win_amd64",
            "macosx_11_0_arm64",
            "freebsd_13_0_x86_64",
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert!(policy.check_wheel(&filename).is_ok(), "{platform}");
            assert_eq!(
                policy.wheel_coverage(&filename),
                implied_markers(&filename),
                "{platform}",
            );
        }

        for platform in [
            "manylinux_2_34_x86_64",
            "musllinux_1_2_x86_64",
            "manylinux_2_34_x86_64.musllinux_1_2_x86_64",
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert!(policy.check_wheel(&filename).is_err(), "{platform}");
            assert!(ArtifactPolicy::default().check_wheel(&filename).is_ok());
            assert_eq!(
                ArtifactPolicy::default().wheel_coverage(&filename),
                implied_markers(&filename),
                "{platform}",
            );
            assert_eq!(
                policy.wheel_coverage(&filename),
                MarkerTree::FALSE,
                "{platform}",
            );
            assert!(!implied_markers(&filename).is_false());
        }

        let filename = WheelFilename::from_str(
            "example-1.0-py3-none-manylinux_2_17_x86_64.manylinux_2_34_aarch64.musllinux_1_2_aarch64.win_amd64.whl",
        )?;
        let compatible =
            WheelFilename::from_str("example-1.0-py3-none-manylinux_2_17_x86_64.win_amd64.whl")?;
        assert!(policy.check_wheel(&filename).is_ok());
        assert_eq!(
            policy.wheel_coverage(&filename),
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
                ArtifactPolicy::new(MinimumLibcVersion {
                    glibc: Some(LibcVersion::new(2, minor)),
                    musl: None,
                })
                .wheel_coverage(&filename),
                implied_markers(&filename),
            );
            assert_eq!(
                ArtifactPolicy::new(MinimumLibcVersion {
                    glibc: Some(LibcVersion::new(2, minor - 1)),
                    musl: None,
                })
                .wheel_coverage(&filename),
                MarkerTree::FALSE,
            );
        }
        Ok(())
    }

    #[test]
    fn musl_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let policy = ArtifactPolicy::new(MinimumLibcVersion {
            glibc: None,
            musl: Some(LibcVersion::new(1, 2)),
        });
        for (platform, eligible) in [
            ("musllinux_1_1_x86_64", true),
            ("musllinux_1_2_x86_64", true),
            ("musllinux_1_3_x86_64", false),
            ("manylinux_2_17_x86_64", false),
            ("manylinux2014_x86_64", false),
            ("linux_x86_64", true),
            ("any", true),
            ("win_amd64", true),
        ] {
            let filename =
                WheelFilename::from_str(&format!("example-1.0-py3-none-{platform}.whl"))?;
            assert_eq!(
                policy.check_wheel(&filename).is_ok(),
                eligible,
                "{platform}"
            );
            assert_eq!(
                policy.wheel_coverage(&filename),
                if eligible {
                    implied_markers(&filename)
                } else {
                    MarkerTree::FALSE
                },
                "{platform}",
            );
        }
        Ok(())
    }

    #[test]
    fn combined_libc_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let policy = ArtifactPolicy::new(MinimumLibcVersion {
            glibc: Some(LibcVersion::new(2, 31)),
            musl: Some(LibcVersion::new(1, 2)),
        });
        let manylinux = WheelFilename::from_str("example-1.0-py3-none-manylinux_2_17_x86_64.whl")?;
        let musllinux = WheelFilename::from_str("example-1.0-py3-none-musllinux_1_2_x86_64.whl")?;
        let mut coverage = ArtifactCoverage::EMPTY;
        for filename in [&manylinux, &musllinux] {
            assert!(policy.check_wheel(filename).is_ok());
            assert_eq!(policy.wheel_coverage(filename), MarkerTree::FALSE);
            coverage.insert_wheel(policy, filename);
        }
        assert_eq!(coverage.markers(policy), implied_markers(&manylinux));

        let mixed = WheelFilename::from_str(
            "example-1.0-py3-none-manylinux_2_17_x86_64.musllinux_1_2_x86_64.whl",
        )?;
        assert_eq!(policy.wheel_coverage(&mixed), coverage.markers(policy));

        let mut split_python = ArtifactCoverage::EMPTY;
        for filename in [
            "example-1.0-cp312-cp312-manylinux_2_17_x86_64.whl",
            "example-1.0-cp313-cp313-musllinux_1_2_x86_64.whl",
        ] {
            split_python.insert_wheel(policy, &WheelFilename::from_str(filename)?);
        }
        assert_eq!(split_python.markers(policy), MarkerTree::FALSE);
        assert_eq!(
            ArtifactCoverage::UNIVERSAL.markers(policy),
            MarkerTree::TRUE
        );
        Ok(())
    }
}
