use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};

/// A platform for which the resolver is solving.
#[derive(Debug, Clone, Copy)]
pub(crate) enum KnownPlatform {
    Linux,
    Windows,
    MacOS,
}

impl KnownPlatform {
    /// Return a [`MarkerTree`] for the platform.
    fn marker(self) -> MarkerTree {
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
    pub(crate) fn from_marker(marker: MarkerTree) -> Option<Self> {
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

// On Linux, get_interpreter_info.py adjusts the wheel architecture for
// a 32-bit interpreter but uses platform.machine() for dependency markers.
static LINUX_INTERPRETER_ARCHITECTURES: LazyLock<[(MarkerTree, MarkerTree); 3]> =
    LazyLock::new(|| {
        let machine = |name: &'static str| {
            MarkerTree::expression(MarkerExpression::String {
                key: MarkerValueString::PlatformMachine,
                operator: MarkerOperator::Equal,
                value: name.into(),
            })
        };
        [
            ("x86_64", "i686"),
            ("aarch64", "armv7l"),
            ("armv8l", "armv7l"),
        ]
        .map(|(host, interpreter)| (machine(host), machine(interpreter)))
    });

/// Include wheel architectures for interpreters that can run on a host in this fork.
/// Keep Python, OS, and all other constraints from the corresponding host environment.
pub(crate) fn wheel_retention_markers(fork: MarkerTree) -> MarkerTree {
    let windows = fork.and(KnownPlatform::Windows.marker());
    // A Windows host can run an interpreter with a different architecture under emulation.
    let mut retained = fork.or(windows.without_platform_machine());
    let linux = fork.and(KnownPlatform::Linux.marker());
    for &(host, interpreter) in LINUX_INTERPRETER_ARCHITECTURES.iter() {
        let hosts = linux.and(host);
        if !hosts.is_false() {
            retained = retained.or(hosts.without_platform_machine().and(interpreter));
        }
    }
    retained
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
