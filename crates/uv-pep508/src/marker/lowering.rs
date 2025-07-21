use std::fmt::{Display, Formatter};
use uv_normalize::{ExtraName, GroupName};

use crate::marker::tree::MarkerValueList;
use crate::{MarkerValueExtra, MarkerValueString, MarkerValueVersion};

/// Those environment markers with a PEP 440 version as value such as `python_version`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum CanonicalMarkerValueVersion {
    /// `implementation_version`
    ImplementationVersion,
    /// `python_full_version`
    PythonFullVersion,
}

impl Display for CanonicalMarkerValueVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
        }
    }
}

impl From<CanonicalMarkerValueVersion> for MarkerValueVersion {
    fn from(value: CanonicalMarkerValueVersion) -> Self {
        match value {
            CanonicalMarkerValueVersion::ImplementationVersion => Self::ImplementationVersion,
            CanonicalMarkerValueVersion::PythonFullVersion => Self::PythonFullVersion,
        }
    }
}

/// Those environment markers with an arbitrary string as value such as `sys_platform`.
///
/// As in [`crate::marker::algebra::Variable`], this `enum` also defines the variable ordering for
/// all ADDs, which is in turn used when translating the ADD to DNF. As such, modifying the ordering
/// will modify the output of marker expressions.
///
/// Critically, any variants that could be involved in a known-incompatible marker pair should
/// be at the top of the ordering, i.e., given the maximum priority.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CanonicalMarkerValueString {
    /// `os_name`
    OsName,
    /// `sys_platform`
    SysPlatform,
    /// `platform_system`
    PlatformSystem,
    /// `platform_machine`
    PlatformMachine,
    /// Deprecated `platform.machine` from <https://peps.python.org/pep-0345/#environment-markers>
    /// `platform_python_implementation`
    PlatformPythonImplementation,
    /// `platform_release`
    PlatformRelease,
    /// `platform_version`
    PlatformVersion,
    /// `implementation_name`
    ImplementationName,
}

impl CanonicalMarkerValueString {
    /// Returns `true` if the marker is known to be involved in _at least_ one conflicting
    /// marker pair.
    ///
    /// For example, `sys_platform == 'win32'` and `platform_system == 'Darwin'` are known to
    /// never be true at the same time.
    pub(crate) fn is_conflicting(self) -> bool {
        self <= Self::PlatformSystem
    }
}

impl From<MarkerValueString> for CanonicalMarkerValueString {
    fn from(value: MarkerValueString) -> Self {
        match value {
            MarkerValueString::ImplementationName => Self::ImplementationName,
            MarkerValueString::OsName => Self::OsName,
            MarkerValueString::OsNameDeprecated => Self::OsName,
            MarkerValueString::PlatformMachine => Self::PlatformMachine,
            MarkerValueString::PlatformMachineDeprecated => Self::PlatformMachine,
            MarkerValueString::PlatformPythonImplementation => Self::PlatformPythonImplementation,
            MarkerValueString::PlatformPythonImplementationDeprecated => {
                Self::PlatformPythonImplementation
            }
            MarkerValueString::PythonImplementationDeprecated => Self::PlatformPythonImplementation,
            MarkerValueString::PlatformRelease => Self::PlatformRelease,
            MarkerValueString::PlatformSystem => Self::PlatformSystem,
            MarkerValueString::PlatformVersion => Self::PlatformVersion,
            MarkerValueString::PlatformVersionDeprecated => Self::PlatformVersion,
            MarkerValueString::SysPlatform => Self::SysPlatform,
            MarkerValueString::SysPlatformDeprecated => Self::SysPlatform,
        }
    }
}

impl From<CanonicalMarkerValueString> for MarkerValueString {
    fn from(value: CanonicalMarkerValueString) -> Self {
        match value {
            CanonicalMarkerValueString::ImplementationName => Self::ImplementationName,
            CanonicalMarkerValueString::OsName => Self::OsName,
            CanonicalMarkerValueString::PlatformMachine => Self::PlatformMachine,
            CanonicalMarkerValueString::PlatformPythonImplementation => {
                Self::PlatformPythonImplementation
            }
            CanonicalMarkerValueString::PlatformRelease => Self::PlatformRelease,
            CanonicalMarkerValueString::PlatformSystem => Self::PlatformSystem,
            CanonicalMarkerValueString::PlatformVersion => Self::PlatformVersion,
            CanonicalMarkerValueString::SysPlatform => Self::SysPlatform,
        }
    }
}

impl Display for CanonicalMarkerValueString {
    /// Normalizes deprecated names to the proper ones
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationName => f.write_str("implementation_name"),
            Self::OsName => f.write_str("os_name"),
            Self::PlatformMachine => f.write_str("platform_machine"),
            Self::PlatformPythonImplementation => f.write_str("platform_python_implementation"),
            Self::PlatformRelease => f.write_str("platform_release"),
            Self::PlatformSystem => f.write_str("platform_system"),
            Self::PlatformVersion => f.write_str("platform_version"),
            Self::SysPlatform => f.write_str("sys_platform"),
        }
    }
}

/// The [`ExtraName`] value used in `extra` and `extras` markers.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CanonicalMarkerValueExtra {
    /// A valid [`ExtraName`].
    Extra(ExtraName),
}

impl CanonicalMarkerValueExtra {
    /// Returns the [`ExtraName`] value.
    pub fn extra(&self) -> &ExtraName {
        match self {
            Self::Extra(extra) => extra,
        }
    }
}

impl From<CanonicalMarkerValueExtra> for MarkerValueExtra {
    fn from(value: CanonicalMarkerValueExtra) -> Self {
        match value {
            CanonicalMarkerValueExtra::Extra(extra) => Self::Extra(extra),
        }
    }
}

impl Display for CanonicalMarkerValueExtra {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extra(extra) => extra.fmt(f),
        }
    }
}

/// A key-value pair for `<value> in <key>` or `<value> not in <key>`, where the key is a list.
///
/// Used for PEP 751 and variant markers.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CanonicalMarkerListPair {
    /// A valid [`ExtraName`].
    Extras(ExtraName),
    /// A valid [`GroupName`].
    DependencyGroup(GroupName),
    /// A valid `variant_namespaces`.
    VariantNamespaces(String),
    /// A valid `variant_features`.
    VariantFeatures(String, String),
    /// A valid `variant_properties`.
    VariantProperties(String, String, String),
    /// For leniency, preserve invalid values.
    Arbitrary { key: MarkerValueList, value: String },
}

impl CanonicalMarkerListPair {
    /// The key (RHS) of the marker expression.
    pub(crate) fn key(&self) -> MarkerValueList {
        match self {
            Self::Extras(_) => MarkerValueList::Extras,
            Self::DependencyGroup(_) => MarkerValueList::DependencyGroups,
            Self::VariantNamespaces(_) => MarkerValueList::VariantNamespaces,
            Self::VariantFeatures(_, _) => MarkerValueList::VariantFeatures,
            Self::VariantProperties(_, _, _) => MarkerValueList::VariantProperties,
            Self::Arbitrary { key, .. } => *key,
        }
    }

    /// The value (LHS) of the marker expression.
    pub(crate) fn value(&self) -> String {
        match self {
            Self::Extras(extra) => extra.to_string(),
            Self::DependencyGroup(group) => group.to_string(),
            Self::VariantNamespaces(namespace) => namespace.clone(),
            Self::VariantFeatures(namespace, property) => {
                format!("{namespace} :: {property}")
            }
            Self::VariantProperties(namespace, property, value) => {
                format!("{namespace} :: {property} :: {value}")
            }
            Self::Arbitrary { value, .. } => value.clone(),
        }
    }
}
