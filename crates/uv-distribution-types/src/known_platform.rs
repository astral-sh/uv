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
            Self::Linux => "linux",
            Self::Windows => "win32",
            Self::MacOS => "darwin",
        }
    }

    /// Return a [`MarkerTree`] for the platform.
    pub fn marker(self) -> MarkerTree {
        MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::SysPlatform,
            operator: MarkerOperator::Equal,
            value: match self {
                Self::Linux => arcstr::literal!("linux"),
                Self::Windows => arcstr::literal!("win32"),
                Self::MacOS => arcstr::literal!("darwin"),
            },
        })
    }

    /// Determine the [`KnownPlatform`] from a marker tree.
    pub fn from_marker(marker: MarkerTree) -> Option<Self> {
        if marker == Self::Linux.marker() {
            Some(Self::Linux)
        } else if marker == Self::Windows.marker() {
            Some(Self::Windows)
        } else if marker == Self::MacOS.marker() {
            Some(Self::MacOS)
        } else {
            None
        }
    }
}

impl Display for KnownPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linux => write!(f, "Linux"),
            Self::Windows => write!(f, "Windows"),
            Self::MacOS => write!(f, "macOS"),
        }
    }
}
