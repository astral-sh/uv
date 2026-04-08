use std::{
    ops::{Deref, DerefMut},
    str::FromStr,
};

use jiff::Timestamp;
use rustc_hash::FxHashMap;
use serde::ser::SerializeMap;
use uv_distribution_types::{ExcludeNewerOverride, ExcludeNewerSpan, ExcludeNewerValue};
use uv_normalize::PackageName;
use uv_preview::PreviewFeature;
use uv_warnings::warn_user_once;

/// The configuration layer that supplied the effective `exclude-newer` cutoff for a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EffectiveExcludeNewerSource {
    /// The global `exclude-newer` setting.
    Global,
    /// A package-specific `exclude-newer-package` override.
    Package,
    /// An index-specific `[[tool.uv.index]].exclude-newer` override.
    Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeNewerValueChange {
    /// A relative span changed to a new value
    SpanChanged(ExcludeNewerSpan, ExcludeNewerSpan),
    /// A relative span was added
    SpanAdded(ExcludeNewerSpan),
    /// A relative span was removed
    SpanRemoved,
    /// A relative span is present and the timestamp changed
    RelativeTimestampChanged(Timestamp, Timestamp, ExcludeNewerSpan),
    /// The timestamp changed and a relative span is not present
    AbsoluteTimestampChanged(Timestamp, Timestamp),
}

impl ExcludeNewerValueChange {
    pub fn is_relative_timestamp_change(&self) -> bool {
        matches!(self, Self::RelativeTimestampChanged(_, _, _))
    }
}

impl std::fmt::Display for ExcludeNewerValueChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpanChanged(old, new) => {
                write!(f, "change of exclude newer span from `{old}` to `{new}`")
            }
            Self::SpanAdded(span) => {
                write!(f, "addition of exclude newer span `{span}`")
            }
            Self::SpanRemoved => {
                write!(f, "removal of exclude newer span")
            }
            Self::RelativeTimestampChanged(old, new, span) => {
                write!(
                    f,
                    "change of calculated ({span}) exclude newer timestamp from `{old}` to `{new}`"
                )
            }
            Self::AbsoluteTimestampChanged(old, new) => {
                write!(
                    f,
                    "change of exclude newer timestamp from `{old}` to `{new}`"
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeNewerChange {
    GlobalChanged(ExcludeNewerValueChange),
    GlobalAdded(ExcludeNewerValue),
    GlobalRemoved,
    Package(ExcludeNewerPackageChange),
}

impl ExcludeNewerChange {
    /// Whether the change is due to a change in a relative timestamp.
    pub fn is_relative_timestamp_change(&self) -> bool {
        match self {
            Self::GlobalChanged(change) => change.is_relative_timestamp_change(),
            Self::GlobalAdded(_) | Self::GlobalRemoved => false,
            Self::Package(change) => change.is_relative_timestamp_change(),
        }
    }
}

impl std::fmt::Display for ExcludeNewerChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GlobalChanged(change) => {
                write!(f, "{change}")
            }
            Self::GlobalAdded(value) => {
                write!(f, "addition of global exclude newer {value}")
            }
            Self::GlobalRemoved => write!(f, "removal of global exclude newer"),
            Self::Package(change) => {
                write!(f, "{change}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeNewerPackageChange {
    PackageAdded(PackageName, ExcludeNewerOverride),
    PackageRemoved(PackageName),
    PackageChanged(PackageName, Box<ExcludeNewerOverrideChange>),
}

impl ExcludeNewerPackageChange {
    pub fn is_relative_timestamp_change(&self) -> bool {
        match self {
            Self::PackageAdded(_, _) | Self::PackageRemoved(_) => false,
            Self::PackageChanged(_, change) => change.is_relative_timestamp_change(),
        }
    }
}

impl std::fmt::Display for ExcludeNewerPackageChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageAdded(name, ExcludeNewerOverride::Enabled(value)) => {
                write!(
                    f,
                    "addition of exclude newer `{}` for package `{name}`",
                    value.as_ref()
                )
            }
            Self::PackageAdded(name, ExcludeNewerOverride::Disabled) => {
                write!(
                    f,
                    "addition of exclude newer exclusion for package `{name}`"
                )
            }
            Self::PackageRemoved(name) => {
                write!(f, "removal of exclude newer for package `{name}`")
            }
            Self::PackageChanged(name, change) => write!(f, "{change} for package `{name}`"),
        }
    }
}

fn compare_exclude_newer_value(
    this: &ExcludeNewerValue,
    other: &ExcludeNewerValue,
) -> Option<ExcludeNewerValueChange> {
    match (this.span(), other.span()) {
        (None, Some(span)) => Some(ExcludeNewerValueChange::SpanAdded(*span)),
        (Some(_), None) => Some(ExcludeNewerValueChange::SpanRemoved),
        (Some(self_span), Some(other_span)) if self_span != other_span => Some(
            ExcludeNewerValueChange::SpanChanged(*self_span, *other_span),
        ),
        (Some(_), Some(span)) if this.timestamp() != other.timestamp() => {
            Some(ExcludeNewerValueChange::RelativeTimestampChanged(
                this.timestamp(),
                other.timestamp(),
                *span,
            ))
        }
        (None, None) if this.timestamp() != other.timestamp() => Some(
            ExcludeNewerValueChange::AbsoluteTimestampChanged(this.timestamp(), other.timestamp()),
        ),
        (Some(_), Some(_)) | (None, None) => None,
    }
}

pub struct ExcludeNewerValueWithSpanRef<'a>(pub &'a ExcludeNewerValue);

impl serde::Serialize for ExcludeNewerValueWithSpanRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(span) = self.0.span() {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("timestamp", &self.0.timestamp())?;
            map.serialize_entry("span", span)?;
            map.end()
        } else {
            self.0.timestamp().serialize(serializer)
        }
    }
}

/// A package-specific exclude-newer entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewerPackageEntry {
    pub package: PackageName,
    pub setting: ExcludeNewerOverride,
}

impl FromStr for ExcludeNewerPackageEntry {
    type Err = String;

    /// Parses a [`ExcludeNewerPackageEntry`] from a string in the format `PACKAGE=DATE` or `PACKAGE=false`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((package, value)) = s.split_once('=') else {
            return Err(format!(
                "Invalid `exclude-newer-package` value `{s}`: expected format `PACKAGE=DATE` or `PACKAGE=false`"
            ));
        };

        let package = PackageName::from_str(package).map_err(|err| {
            format!("Invalid `exclude-newer-package` package name `{package}`: {err}")
        })?;

        let setting = if value == "false" {
            ExcludeNewerOverride::Disabled
        } else {
            ExcludeNewerOverride::Enabled(Box::new(ExcludeNewerValue::from_str(value).map_err(
                |err| format!("Invalid `exclude-newer-package` value `{value}`: {err}"),
            )?))
        };

        Ok(Self { package, setting })
    }
}

impl From<(PackageName, ExcludeNewerOverride)> for ExcludeNewerPackageEntry {
    fn from((package, setting): (PackageName, ExcludeNewerOverride)) -> Self {
        Self { package, setting }
    }
}

impl From<(PackageName, ExcludeNewerValue)> for ExcludeNewerPackageEntry {
    fn from((package, timestamp): (PackageName, ExcludeNewerValue)) -> Self {
        Self {
            package,
            setting: ExcludeNewerOverride::Enabled(Box::new(timestamp)),
        }
    }
}

pub fn serialize_exclude_newer_package_with_spans<S>(
    value: &Option<ExcludeNewerPackage>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let Some(value) = value else {
        return serializer.serialize_none();
    };

    let mut map = serializer.serialize_map(Some(value.len()))?;
    for (name, setting) in value {
        match setting {
            ExcludeNewerOverride::Disabled => map.serialize_entry(name, &false)?,
            ExcludeNewerOverride::Enabled(value) => {
                map.serialize_entry(name, &ExcludeNewerValueWithSpanRef(value.as_ref()))?;
            }
        }
    }
    map.end()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeNewerOverrideChange {
    Disabled { was: ExcludeNewerValue },
    Enabled { now: ExcludeNewerValue },
    TimestampChanged(ExcludeNewerValueChange),
}

impl ExcludeNewerOverrideChange {
    pub fn is_relative_timestamp_change(&self) -> bool {
        match self {
            Self::Disabled { .. } | Self::Enabled { .. } => false,
            Self::TimestampChanged(change) => change.is_relative_timestamp_change(),
        }
    }
}

impl std::fmt::Display for ExcludeNewerOverrideChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled { was } => {
                write!(f, "add exclude newer exclusion (was `{was}`)")
            }
            Self::Enabled { now } => {
                write!(f, "remove exclude newer exclusion (now `{now}`)")
            }
            Self::TimestampChanged(change) => write!(f, "{change}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewerPackage(FxHashMap<PackageName, ExcludeNewerOverride>);

impl Deref for ExcludeNewerPackage {
    type Target = FxHashMap<PackageName, ExcludeNewerOverride>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ExcludeNewerPackage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromIterator<ExcludeNewerPackageEntry> for ExcludeNewerPackage {
    fn from_iter<T: IntoIterator<Item = ExcludeNewerPackageEntry>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .map(|entry| (entry.package, entry.setting))
                .collect(),
        )
    }
}

impl IntoIterator for ExcludeNewerPackage {
    type Item = (PackageName, ExcludeNewerOverride);
    type IntoIter = std::collections::hash_map::IntoIter<PackageName, ExcludeNewerOverride>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ExcludeNewerPackage {
    type Item = (&'a PackageName, &'a ExcludeNewerOverride);
    type IntoIter = std::collections::hash_map::Iter<'a, PackageName, ExcludeNewerOverride>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl ExcludeNewerPackage {
    /// Convert to the inner `HashMap`.
    pub fn into_inner(self) -> FxHashMap<PackageName, ExcludeNewerOverride> {
        self.0
    }

    /// Returns true if this map is empty (no package-specific settings).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Recompute all relative span timestamps relative to the current time.
    #[must_use]
    pub fn recompute(self) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|(name, setting)| {
                    let setting = match setting {
                        ExcludeNewerOverride::Disabled => ExcludeNewerOverride::Disabled,
                        ExcludeNewerOverride::Enabled(value) => {
                            ExcludeNewerOverride::Enabled(Box::new((*value).recompute()))
                        }
                    };
                    (name, setting)
                })
                .collect(),
        )
    }

    pub fn compare(&self, other: &Self) -> Option<ExcludeNewerPackageChange> {
        for (package, setting) in self {
            match (setting, other.get(package)) {
                (
                    ExcludeNewerOverride::Enabled(self_timestamp),
                    Some(ExcludeNewerOverride::Enabled(other_timestamp)),
                ) => {
                    if let Some(change) =
                        compare_exclude_newer_value(self_timestamp, other_timestamp)
                    {
                        return Some(ExcludeNewerPackageChange::PackageChanged(
                            package.clone(),
                            Box::new(ExcludeNewerOverrideChange::TimestampChanged(change)),
                        ));
                    }
                }
                (
                    ExcludeNewerOverride::Enabled(self_timestamp),
                    Some(ExcludeNewerOverride::Disabled),
                ) => {
                    return Some(ExcludeNewerPackageChange::PackageChanged(
                        package.clone(),
                        Box::new(ExcludeNewerOverrideChange::Disabled {
                            was: self_timestamp.as_ref().clone(),
                        }),
                    ));
                }
                (
                    ExcludeNewerOverride::Disabled,
                    Some(ExcludeNewerOverride::Enabled(other_timestamp)),
                ) => {
                    return Some(ExcludeNewerPackageChange::PackageChanged(
                        package.clone(),
                        Box::new(ExcludeNewerOverrideChange::Enabled {
                            now: other_timestamp.as_ref().clone(),
                        }),
                    ));
                }
                (ExcludeNewerOverride::Disabled, Some(ExcludeNewerOverride::Disabled)) => {}
                (_, None) => {
                    return Some(ExcludeNewerPackageChange::PackageRemoved(package.clone()));
                }
            }
        }

        for (package, value) in other {
            if !self.contains_key(package) {
                return Some(ExcludeNewerPackageChange::PackageAdded(
                    package.clone(),
                    value.clone(),
                ));
            }
        }

        None
    }
}

/// A setting that excludes files newer than a timestamp, at a global level or per-package.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewer {
    /// Global timestamp that applies to all packages if no package-specific timestamp is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<ExcludeNewerValue>,
    /// Per-package timestamps that override the global timestamp.
    #[serde(default, skip_serializing_if = "FxHashMap::is_empty")]
    pub package: ExcludeNewerPackage,
}

impl ExcludeNewer {
    /// Create a new exclude newer configuration with just a global timestamp.
    pub fn global(global: ExcludeNewerValue) -> Self {
        Self {
            global: Some(global),
            package: ExcludeNewerPackage::default(),
        }
    }

    fn warn_index_exclude_newer_preview() {
        if !uv_preview::is_enabled(PreviewFeature::IndexExcludeNewer) {
            warn_user_once!(
                "Setting `exclude-newer` on configured indexes is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                PreviewFeature::IndexExcludeNewer
            );
        }
    }

    /// Create a new exclude newer configuration.
    pub fn new(global: Option<ExcludeNewerValue>, package: ExcludeNewerPackage) -> Self {
        Self { global, package }
    }

    /// Create from CLI arguments.
    pub fn from_args(
        global: Option<ExcludeNewerValue>,
        package: Vec<ExcludeNewerPackageEntry>,
    ) -> Self {
        let package: ExcludeNewerPackage = package.into_iter().collect();

        Self { global, package }
    }

    /// Returns the exclude-newer value for a specific package, returning `Some(value)` if the
    /// package has a package-specific setting or falls back to the global value if set, or `None`
    /// if exclude-newer is explicitly disabled for the package (set to `false`) or if no
    /// exclude-newer is configured.
    pub fn exclude_newer_package(&self, package_name: &PackageName) -> Option<ExcludeNewerValue> {
        match self.package.get(package_name) {
            Some(ExcludeNewerOverride::Enabled(timestamp)) => Some(timestamp.as_ref().clone()),
            Some(ExcludeNewerOverride::Disabled) => None,
            None => self.global.clone(),
        }
    }

    /// Returns the effective exclude-newer value for a package resolved from a specific index.
    pub fn exclude_newer_package_for_index(
        &self,
        package_name: &PackageName,
        index: Option<&ExcludeNewerOverride>,
    ) -> Option<ExcludeNewerValue> {
        self.exclude_newer_package_for_index_with_source(package_name, index)
            .map(|(exclude_newer, _)| exclude_newer)
    }

    /// Returns the effective exclude-newer value and its source for a package resolved from a
    /// specific index.
    pub(crate) fn exclude_newer_package_for_index_with_source(
        &self,
        package_name: &PackageName,
        index: Option<&ExcludeNewerOverride>,
    ) -> Option<(ExcludeNewerValue, EffectiveExcludeNewerSource)> {
        match self.package.get(package_name) {
            Some(ExcludeNewerOverride::Enabled(timestamp)) => Some((
                timestamp.as_ref().clone(),
                EffectiveExcludeNewerSource::Package,
            )),
            Some(ExcludeNewerOverride::Disabled) => None,
            None => match index {
                Some(ExcludeNewerOverride::Disabled) => {
                    Self::warn_index_exclude_newer_preview();
                    None
                }
                Some(ExcludeNewerOverride::Enabled(timestamp)) => Some((
                    {
                        Self::warn_index_exclude_newer_preview();
                        ExcludeNewerValue::from(timestamp.timestamp())
                    },
                    EffectiveExcludeNewerSource::Index,
                )),
                None => self
                    .global
                    .clone()
                    .map(|timestamp| (timestamp, EffectiveExcludeNewerSource::Global)),
            },
        }
    }

    /// Returns true if this has any configuration (global or per-package).
    pub fn is_empty(&self) -> bool {
        self.global.is_none() && self.package.is_empty()
    }

    /// Recompute all relative span timestamps relative to the current time.
    ///
    /// For values with an absolute timestamp (no span), the timestamp is unchanged.
    #[must_use]
    pub fn recompute(self) -> Self {
        Self {
            global: self.global.map(ExcludeNewerValue::recompute),
            package: self.package.recompute(),
        }
    }

    pub fn compare(&self, other: &Self) -> Option<ExcludeNewerChange> {
        match (&self.global, &other.global) {
            (Some(self_global), Some(other_global)) => {
                if let Some(change) = compare_exclude_newer_value(self_global, other_global) {
                    return Some(ExcludeNewerChange::GlobalChanged(change));
                }
            }
            (None, Some(global)) => {
                return Some(ExcludeNewerChange::GlobalAdded(global.clone()));
            }
            (Some(_), None) => return Some(ExcludeNewerChange::GlobalRemoved),
            (None, None) => (),
        }
        self.package
            .compare(&other.package)
            .map(ExcludeNewerChange::Package)
    }
}

impl std::fmt::Display for ExcludeNewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(global) = &self.global {
            write!(f, "global: {global}")?;
            if !self.package.is_empty() {
                write!(f, ", ")?;
            }
        }
        let mut first = true;
        for (name, setting) in &self.package {
            if !first {
                write!(f, ", ")?;
            }
            match setting {
                ExcludeNewerOverride::Enabled(timestamp) => {
                    write!(f, "{name}: {}", timestamp.as_ref())?;
                }
                ExcludeNewerOverride::Disabled => {
                    write!(f, "{name}: disabled")?;
                }
            }
            first = false;
        }
        Ok(())
    }
}
