use std::fmt::{Display, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use uv_distribution_filename::WheelFilename;
use uv_pep508::MarkerTree;
use uv_platform_tags::{LibcVersion, PlatformTag};

use crate::Environments;
use crate::prioritized_distribution::{implied_platform_markers, implied_python_markers};

/// The selected Linux libc families and their oldest supported releases.
///
/// At least one family must be specified. The setting determines whether these releases constrain
/// eligible artifacts or require compatible artifacts to exist.
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

impl MinimumLibcVersion {
    /// Reject omitted libc families and releases newer than the selected family baseline.
    ///
    /// Native Linux tags declare no libc version, so accepting them does not establish a glibc
    /// compatibility guarantee. Non-Linux tags are unconstrained.
    fn allows_platform(self, platform: &PlatformTag) -> bool {
        match platform {
            PlatformTag::Manylinux { major, minor, .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(*major, *minor) <= version),
            PlatformTag::Manylinux1 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 5) <= version),
            PlatformTag::Manylinux2010 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 12) <= version),
            PlatformTag::Manylinux2014 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 17) <= version),
            PlatformTag::Musllinux { major, minor, .. } => self
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

/// Artifact restrictions and coverage requirements for a universal resolution.
///
/// Supported environments restrict eligible wheels. Required environments only constrain coverage.
#[derive(Debug, Clone)]
pub struct ArtifactPolicy {
    supported: Arc<[LibcEnvironment]>,
    required: Arc<[LibcEnvironment]>,
    unconstrained: MarkerTree,
}

#[derive(Debug, PartialEq, Eq)]
struct LibcEnvironment {
    marker: MarkerTree,
    libc: MinimumLibcVersion,
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self {
            supported: Arc::default(),
            required: Arc::default(),
            unconstrained: MarkerTree::TRUE,
        }
    }
}

impl ArtifactPolicy {
    pub fn new(supported: &Environments, required: &Environments) -> Self {
        let constrained = |environments: &Environments| {
            environments
                .iter()
                .filter_map(|environment| {
                    environment.libc.map(|libc| LibcEnvironment {
                        marker: environment.marker,
                        libc,
                    })
                })
                .collect::<Arc<[_]>>()
        };
        let supported = constrained(supported);
        let required = constrained(required);
        let constrained = supported
            .iter()
            .fold(MarkerTree::FALSE, |marker, environment| {
                marker.or(environment.marker)
            });
        Self {
            supported,
            required,
            unconstrained: constrained.negate(),
        }
    }

    /// Both supported and required environments need compatible artifacts, even when their scopes
    /// overlap and specify different libc baselines.
    fn coverage_environments(&self) -> impl Iterator<Item = &LibcEnvironment> {
        self.supported.iter().chain(self.required.iter())
    }

    /// Retain a whole wheel if any tag is useful in any allowed region. Unknown platform tags
    /// remain eligible when their environment cannot be inferred; they contribute no coverage.
    pub fn check_wheel(&self, filename: &WheelFilename) -> Result<(), ArtifactPolicyError> {
        if self.supported.is_empty() {
            return Ok(());
        }
        let python = implied_python_markers(filename);
        for tag in filename.platform_tags() {
            let platform = implied_platform_markers([tag]);
            if platform.is_false() {
                return Ok(());
            }
            let marker = platform.and(python);
            if !marker.is_disjoint(self.unconstrained)
                || self.supported.iter().any(|environment| {
                    environment.libc.allows_platform(tag) && !marker.is_disjoint(environment.marker)
                })
            {
                return Ok(());
            }
        }
        Err(ArtifactPolicyError {
            environments: Arc::clone(&self.supported),
        })
    }

    /// Return coverage of a single wheel under each applicable scope. Registry callers must
    /// aggregate separate wheel files before intersecting the libc families within each scope.
    pub fn wheel_coverage(&self, filename: &WheelFilename) -> MarkerTree {
        let mut coverage = ArtifactCoverage::new(self);
        coverage.insert_wheel(self, filename);
        coverage.markers(self)
    }
}

/// Per-scope, per-family unions of compatible artifacts. The cache must be used with the same
/// policy that created it; separate wheel files can jointly satisfy a two-family scope.
#[derive(Debug, Clone)]
pub(crate) struct ArtifactCoverage {
    ordinary: MarkerTree,
    environments: Vec<LibcCoverage>,
}

#[derive(Debug, Clone, Copy)]
struct LibcCoverage {
    glibc: MarkerTree,
    musl: MarkerTree,
}

impl ArtifactCoverage {
    pub(crate) fn new(policy: &ArtifactPolicy) -> Self {
        Self {
            ordinary: MarkerTree::FALSE,
            environments: vec![
                LibcCoverage {
                    glibc: MarkerTree::FALSE,
                    musl: MarkerTree::FALSE
                };
                policy.supported.len() + policy.required.len()
            ],
        }
    }

    /// A usable source distribution covers all regions without requiring a wheel for each libc.
    pub(crate) fn insert_source(&mut self) {
        self.ordinary = MarkerTree::TRUE;
        for coverage in &mut self.environments {
            coverage.glibc = MarkerTree::TRUE;
            coverage.musl = MarkerTree::TRUE;
        }
    }

    /// Union each wheel's complete platform-and-Python coverage before combining libc families.
    pub(crate) fn insert_wheel(&mut self, policy: &ArtifactPolicy, filename: &WheelFilename) {
        let python = implied_python_markers(filename);
        let ordinary = implied_platform_markers(filename.platform_tags()).and(python);
        self.ordinary = self.ordinary.or(ordinary);
        for (coverage, environment) in self
            .environments
            .iter_mut()
            .zip(policy.coverage_environments())
        {
            let minimum = environment.libc;
            let glibc = implied_platform_markers(filename.platform_tags().iter().filter(|tag| {
                minimum.allows_platform(tag) && !matches!(tag, PlatformTag::Musllinux { .. })
            }))
            .and(python)
            .and(environment.marker);
            let musl = implied_platform_markers(filename.platform_tags().iter().filter(|tag| {
                minimum.allows_platform(tag)
                    && !matches!(
                        tag,
                        PlatformTag::Manylinux { .. }
                            | PlatformTag::Manylinux1 { .. }
                            | PlatformTag::Manylinux2010 { .. }
                            | PlatformTag::Manylinux2014 { .. }
                    )
            }))
            .and(python)
            .and(environment.marker);
            coverage.glibc = coverage.glibc.or(glibc);
            coverage.musl = coverage.musl.or(musl);
        }
    }

    /// Require each selected libc within its scope, retaining ordinary coverage elsewhere.
    pub(crate) fn markers(&self, policy: &ArtifactPolicy) -> MarkerTree {
        self.environments
            .iter()
            .zip(policy.coverage_environments())
            .fold(self.ordinary, |markers, (coverage, environment)| {
                let covered = match (environment.libc.glibc, environment.libc.musl) {
                    (Some(_), Some(_)) => coverage.glibc.and(coverage.musl),
                    (Some(_), None) => coverage.glibc,
                    (None, Some(_)) => coverage.musl,
                    (None, None) => coverage.glibc.or(coverage.musl),
                };
                markers.and(environment.marker.negate().or(covered))
            })
    }
}

/// A wheel excluded by the universal resolution's artifact policy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ArtifactPolicyError {
    environments: Arc<[LibcEnvironment]>,
}

impl Display for ArtifactPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("no wheels compatible with ")?;
        for (index, environment) in self.environments.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            let marker = environment
                .marker
                .try_to_string()
                .unwrap_or_else(|| "true".to_owned());
            write!(formatter, "{} for `{marker}`", environment.libc)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_pep508::MarkerTree;
    use uv_platform_tags::LibcVersion;

    use super::{ArtifactCoverage, ArtifactPolicy, MinimumLibcVersion};
    use crate::{Environment, Environments, implied_markers};

    fn global_policy(libc: MinimumLibcVersion) -> ArtifactPolicy {
        ArtifactPolicy::new(
            &Environments::from_environments(vec![Environment {
                marker: MarkerTree::TRUE,
                libc: Some(libc),
            }]),
            &Environments::default(),
        )
    }

    #[test]
    fn glibc_artifact_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let policy = global_policy(MinimumLibcVersion {
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
                global_policy(MinimumLibcVersion {
                    glibc: Some(LibcVersion::new(2, minor)),
                    musl: None,
                })
                .wheel_coverage(&filename),
                implied_markers(&filename),
            );
            assert_eq!(
                global_policy(MinimumLibcVersion {
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
        let policy = global_policy(MinimumLibcVersion {
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
        let policy = global_policy(MinimumLibcVersion {
            glibc: Some(LibcVersion::new(2, 31)),
            musl: Some(LibcVersion::new(1, 2)),
        });
        let manylinux = WheelFilename::from_str("example-1.0-py3-none-manylinux_2_17_x86_64.whl")?;
        let musllinux = WheelFilename::from_str("example-1.0-py3-none-musllinux_1_2_x86_64.whl")?;
        let mut coverage = ArtifactCoverage::new(&policy);
        for filename in [&manylinux, &musllinux] {
            assert!(policy.check_wheel(filename).is_ok());
            assert_eq!(policy.wheel_coverage(filename), MarkerTree::FALSE);
            coverage.insert_wheel(&policy, filename);
        }
        assert_eq!(coverage.markers(&policy), implied_markers(&manylinux));

        let mixed = WheelFilename::from_str(
            "example-1.0-py3-none-manylinux_2_17_x86_64.musllinux_1_2_x86_64.whl",
        )?;
        assert_eq!(policy.wheel_coverage(&mixed), coverage.markers(&policy));

        let mut split_python = ArtifactCoverage::new(&policy);
        for filename in [
            "example-1.0-cp312-cp312-manylinux_2_17_x86_64.whl",
            "example-1.0-cp313-cp313-musllinux_1_2_x86_64.whl",
        ] {
            split_python.insert_wheel(&policy, &WheelFilename::from_str(filename)?);
        }
        assert_eq!(split_python.markers(&policy), MarkerTree::FALSE);

        let mut python_union = ArtifactCoverage::new(&policy);
        for filename in [
            "example-1.0-cp312-cp312-manylinux_2_17_x86_64.whl",
            "example-1.0-cp313-cp313-manylinux_2_17_x86_64.whl",
            "example-1.0-cp312-cp312-musllinux_1_2_x86_64.whl",
        ] {
            python_union.insert_wheel(&policy, &WheelFilename::from_str(filename)?);
        }
        let expected =
            WheelFilename::from_str("example-1.0-cp312-cp312-manylinux_2_17_x86_64.whl")?;
        assert_eq!(python_union.markers(&policy), implied_markers(&expected));
        let newer = WheelFilename::from_str("example-1.0-cp313-cp313-musllinux_1_2_x86_64.whl")?;
        python_union.insert_wheel(&policy, &newer);
        assert_eq!(
            python_union.markers(&policy),
            implied_markers(&expected).or(implied_markers(&newer)),
        );
        coverage.insert_source();
        assert_eq!(coverage.markers(&policy), MarkerTree::TRUE);
        Ok(())
    }
}
