use std::fmt::{Display, Formatter};

use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};

/// A platform for which the resolver is solving.
#[derive(Debug, Clone, Copy)]
pub enum KnownPlatform {
    Linux,
    Windows,
    MacOS,
}

impl KnownPlatform {
    /// Return the platform's `sys.platform` value.
    pub fn sys_platform(self) -> &'static str {
        match self {
            KnownPlatform::Linux => "linux",
            KnownPlatform::Windows => "win32",
            KnownPlatform::MacOS => "darwin",
        }
    }

    /// Return a [`MarkerTree`] for the platform.
    pub fn marker(self) -> MarkerTree {
        MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::SysPlatform,
            operator: MarkerOperator::Equal,
            value: match self {
                KnownPlatform::Linux => arcstr::literal!("linux"),
                KnownPlatform::Windows => arcstr::literal!("win32"),
                KnownPlatform::MacOS => arcstr::literal!("darwin"),
            },
        })
    }

    /// Determine the [`KnownPlatform`] from a marker tree.
    pub fn from_marker(marker: MarkerTree) -> Option<KnownPlatform> {
        if marker == KnownPlatform::Linux.marker() {
            Some(KnownPlatform::Linux)
        } else if marker == KnownPlatform::Windows.marker() {
            Some(KnownPlatform::Windows)
        } else if marker == KnownPlatform::MacOS.marker() {
            Some(KnownPlatform::MacOS)
        } else {
            None
        }
    }
}

impl Display for KnownPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KnownPlatform::Linux => write!(f, "Linux"),
            KnownPlatform::Windows => write!(f, "Windows"),
            KnownPlatform::MacOS => write!(f, "macOS"),
        }
    }
}
