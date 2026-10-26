use arcstr::ArcStr;
use tracing::debug;

use uv_distribution_filename::WheelFilename;
use uv_pep440::{Version, VersionSpecifier};
use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};
use uv_platform_tags::{BinaryFormat, LanguageTag, PlatformTag};

/// Infer the environments described by a set of platform tags.
pub fn implied_platform_markers<'a>(
    platform_tags: impl IntoIterator<Item = &'a PlatformTag>,
) -> MarkerTree {
    let mut marker = MarkerTree::FALSE;
    for platform_tag in platform_tags {
        match platform_tag {
            PlatformTag::Any => {
                return MarkerTree::TRUE;
            }

            // Windows
            PlatformTag::Win32 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker = tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("x86"),
                }));
                marker = marker.or(tag_marker);
            }
            PlatformTag::WinAmd64 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker = tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("AMD64"),
                }));
                marker = marker.or(tag_marker);
            }
            PlatformTag::WinArm64 => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("win32"),
                });
                tag_marker = tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("ARM64"),
                }));
                marker = marker.or(tag_marker);
            }

            // macOS
            PlatformTag::Macos {
                major,
                minor,
                binary_format,
            } => {
                let Some(release) = macos_darwin_release(*major, *minor) else {
                    continue;
                };
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("darwin"),
                });

                // Extract the architecture from the end of the tag.
                let mut arch_marker = MarkerTree::FALSE;
                for arch in binary_format.platform_machine() {
                    // A universal2 wheel can target an older macOS on Intel, but its ARM
                    // slice requires macOS 11 (Darwin 20) or later.
                    let release = if *arch == BinaryFormat::Arm64 {
                        release.clone().max(Version::new([20, 0, 0]))
                    } else {
                        release.clone()
                    };
                    let architecture = MarkerTree::expression(MarkerExpression::String {
                        key: MarkerValueString::PlatformMachine,
                        operator: MarkerOperator::Equal,
                        value: ArcStr::from(arch.name()),
                    });
                    let release = MarkerTree::expression(MarkerExpression::String {
                        key: MarkerValueString::PlatformRelease,
                        operator: MarkerOperator::GreaterEqual,
                        value: ArcStr::from(release.to_string()),
                    });
                    arch_marker = arch_marker.or(architecture.and(release));
                }
                tag_marker = tag_marker.and(arch_marker);

                marker = marker.or(tag_marker);
            }

            // Linux
            PlatformTag::Manylinux { arch, .. }
            | PlatformTag::Manylinux1 { arch, .. }
            | PlatformTag::Manylinux2010 { arch, .. }
            | PlatformTag::Manylinux2014 { arch, .. }
            | PlatformTag::Musllinux { arch, .. }
            | PlatformTag::Linux { arch } => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("linux"),
                });
                tag_marker = tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: ArcStr::from(arch.name()),
                }));
                marker = marker.or(tag_marker);
            }

            // Android
            //
            // The tag's API level is not encoded, since no marker exposes it: `platform_release`
            // reports the Android version.
            PlatformTag::Android { abi, .. } => {
                let mut tag_marker = MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("android"),
                });
                tag_marker = tag_marker.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformMachine,
                    operator: MarkerOperator::Equal,
                    value: ArcStr::from(abi.arch().name()),
                }));
                marker = marker.or(tag_marker);
            }

            tag => {
                debug!("Unknown platform tag in wheel tag: {tag}");
            }
        }
    }

    marker
}

/// Translate a macOS deployment target into the corresponding
/// [Darwin kernel release](<https://en.wikipedia.org/wiki/Darwin_(operating_system)#Darwin_20_onwards>).
///
/// macOS 10.16 is the compatibility spelling of macOS 11. macOS 26 uses Darwin 25;
/// starting with macOS 27, the major versions match.
fn macos_darwin_release(major: u16, minor: u16) -> Option<Version> {
    let release = match (major, minor) {
        (10, 0) => [1, 3, 0],
        (10, 1) => [1, 4, 1],
        (10, 2..=16) => [u64::from(minor) + 4, 0, 0],
        (11..=15, 0) => [u64::from(major) + 9, 0, 0],
        (26, 0) => [25, 0, 0],
        (27.., 0) => [u64::from(major), 0, 0],
        _ => return None,
    };
    Some(Version::new(release))
}

/// Given a wheel filename, determine the set of supported Python versions, in terms of their markers.
///
/// This is roughly the inverse of Python tag generation: given a tag, we want to infer the
/// supported Python version (rather than generating the supported tags from a given Python version).
pub fn implied_python_markers(filename: &WheelFilename) -> MarkerTree {
    let mut marker = MarkerTree::FALSE;

    // If any ABI tag is a stable ABI (`abi3` or `abi3t`), the python tag represents a minimum
    // version rather than an exact version. For example, `cp39-abi3` means "compatible with
    // CPython 3.9+".
    let is_abi3 = filename.abi_tags().iter().any(|tag| tag.is_stable_abi());

    for python_tag in filename.python_tags() {
        // First, construct the version marker based on the tag
        let mut tree = match python_tag {
            LanguageTag::None => {
                // No Python tag means no Python version requirement.
                return MarkerTree::TRUE;
            }
            LanguageTag::Python { major, minor: None } | LanguageTag::CPythonMajor { major } => {
                if is_abi3 {
                    MarkerTree::expression(MarkerExpression::Version {
                        key: uv_pep508::MarkerValueVersion::PythonVersion,
                        specifier: VersionSpecifier::greater_than_equal_version(Version::new([
                            u64::from(*major),
                        ])),
                    })
                } else {
                    MarkerTree::expression(MarkerExpression::Version {
                        key: uv_pep508::MarkerValueVersion::PythonVersion,
                        specifier: VersionSpecifier::equals_star_version(Version::new([
                            u64::from(*major),
                        ])),
                    })
                }
            }
            LanguageTag::Python {
                major,
                minor: Some(minor),
            } => {
                // Generic Python tags support later minor versions within the same major version.
                MarkerTree::expression(MarkerExpression::Version {
                    key: uv_pep508::MarkerValueVersion::PythonVersion,
                    specifier: VersionSpecifier::greater_than_equal_version(Version::new([
                        u64::from(*major),
                        u64::from(*minor),
                    ])),
                })
                .and(MarkerTree::expression(MarkerExpression::Version {
                    key: uv_pep508::MarkerValueVersion::PythonVersion,
                    specifier: VersionSpecifier::equals_star_version(Version::new([u64::from(
                        *major,
                    )])),
                }))
            }
            LanguageTag::CPython {
                python_version: (major, minor),
            }
            | LanguageTag::PyPy {
                python_version: (major, minor),
            }
            | LanguageTag::GraalPy {
                python_version: (major, minor),
            }
            | LanguageTag::Pyston {
                python_version: (major, minor),
            } => {
                if is_abi3 {
                    MarkerTree::expression(MarkerExpression::Version {
                        key: uv_pep508::MarkerValueVersion::PythonVersion,
                        specifier: VersionSpecifier::greater_than_equal_version(Version::new([
                            u64::from(*major),
                            u64::from(*minor),
                        ])),
                    })
                } else {
                    MarkerTree::expression(MarkerExpression::Version {
                        key: uv_pep508::MarkerValueVersion::PythonVersion,
                        specifier: VersionSpecifier::equals_star_version(Version::new([
                            u64::from(*major),
                            u64::from(*minor),
                        ])),
                    })
                }
            }
        };

        // Then, add implementation markers for implementation-specific tags
        match python_tag {
            LanguageTag::None | LanguageTag::Python { .. } => {
                // No implementation marker needed
            }
            LanguageTag::CPython { .. } | LanguageTag::CPythonMajor { .. } => {
                tree = tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("CPython"),
                }));
            }
            LanguageTag::PyPy { .. } => {
                tree = tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("PyPy"),
                }));
            }
            LanguageTag::GraalPy { .. } => {
                tree = tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("GraalPy"),
                }));
            }
            LanguageTag::Pyston { .. } => {
                tree = tree.and(MarkerTree::expression(MarkerExpression::String {
                    key: MarkerValueString::PlatformPythonImplementation,
                    operator: MarkerOperator::Equal,
                    value: arcstr::literal!("Pyston"),
                }));
            }
        }

        marker = marker.or(tree);
    }

    marker
}
