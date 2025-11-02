#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::{
    ops::{Deref, DerefMut},
    str::FromStr,
};

use jiff::{Timestamp, ToSpan, tz::TimeZone};
use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

/// A timestamp that excludes files newer than it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExcludeNewerTimestamp(Timestamp);

impl ExcludeNewerTimestamp {
    /// Returns the timestamp in milliseconds.
    pub fn timestamp_millis(&self) -> i64 {
        self.0.as_millisecond()
    }
}

impl From<Timestamp> for ExcludeNewerTimestamp {
    fn from(timestamp: Timestamp) -> Self {
        Self(timestamp)
    }
}

impl FromStr for ExcludeNewerTimestamp {
    type Err = String;

    /// Parse an [`ExcludeNewerTimestamp`] from a string.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same
    /// format (e.g., `2006-12-02`).
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // NOTE(burntsushi): Previously, when using Chrono, we tried
        // to parse as a date first, then a timestamp, and if both
        // failed, we combined both of the errors into one message.
        // But in Jiff, if an RFC 3339 timestamp could be parsed, then
        // it must necessarily be the case that a date can also be
        // parsed. So we can collapse the error cases here. That is,
        // if we fail to parse a timestamp and a date, then it should
        // be sufficient to just report the error from parsing the date.
        // If someone tried to write a timestamp but committed an error
        // in the non-date portion, the date parsing below will still
        // report a holistic error that will make sense to the user.
        // (I added a snapshot test for that case.)
        if let Ok(timestamp) = input.parse::<Timestamp>() {
            return Ok(Self(timestamp));
        }
        let date = input
            .parse::<jiff::civil::Date>()
            .map_err(|err| format!("`{input}` could not be parsed as a valid date: {err}"))?;
        let timestamp = date
            .checked_add(1.day())
            .and_then(|date| date.to_zoned(TimeZone::system()))
            .map(|zdt| zdt.timestamp())
            .map_err(|err| {
                format!(
                    "`{input}` parsed to date `{date}`, but could not \
                     be converted to a timestamp: {err}",
                )
            })?;
        Ok(Self(timestamp))
    }
}

impl std::fmt::Display for ExcludeNewerTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A package-specific exclude-newer entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewerPackageEntry {
    pub package: PackageName,
    pub timestamp: ExcludeNewerTimestamp,
}

impl FromStr for ExcludeNewerPackageEntry {
    type Err = String;

    /// Parses a [`ExcludeNewerPackageEntry`] from a string in the format `PACKAGE=DATE`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((package, date)) = s.split_once('=') else {
            return Err(format!(
                "Invalid `exclude-newer-package` value `{s}`: expected format `PACKAGE=DATE`"
            ));
        };

        let package = PackageName::from_str(package).map_err(|err| {
            format!("Invalid `exclude-newer-package` package name `{package}`: {err}")
        })?;
        let timestamp = ExcludeNewerTimestamp::from_str(date)
            .map_err(|err| format!("Invalid `exclude-newer-package` timestamp `{date}`: {err}"))?;

        Ok(Self { package, timestamp })
    }
}

impl From<(PackageName, ExcludeNewerTimestamp)> for ExcludeNewerPackageEntry {
    fn from((package, timestamp): (PackageName, ExcludeNewerTimestamp)) -> Self {
        Self { package, timestamp }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewerPackage(FxHashMap<PackageName, ExcludeNewerTimestamp>);

impl Deref for ExcludeNewerPackage {
    type Target = FxHashMap<PackageName, ExcludeNewerTimestamp>;

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
                .map(|entry| (entry.package, entry.timestamp))
                .collect(),
        )
    }
}

impl IntoIterator for ExcludeNewerPackage {
    type Item = (PackageName, ExcludeNewerTimestamp);
    type IntoIter = std::collections::hash_map::IntoIter<PackageName, ExcludeNewerTimestamp>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ExcludeNewerPackage {
    type Item = (&'a PackageName, &'a ExcludeNewerTimestamp);
    type IntoIter = std::collections::hash_map::Iter<'a, PackageName, ExcludeNewerTimestamp>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl ExcludeNewerPackage {
    /// Convert to the inner `HashMap`.
    pub fn into_inner(self) -> FxHashMap<PackageName, ExcludeNewerTimestamp> {
        self.0
    }
}

/// A setting that excludes files newer than a timestamp, at a global level or per-package.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewer {
    /// Global timestamp that applies to all packages if no package-specific timestamp is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<ExcludeNewerTimestamp>,
    /// Per-package timestamps that override the global timestamp.
    #[serde(default, skip_serializing_if = "FxHashMap::is_empty")]
    pub package: ExcludeNewerPackage,
}

impl ExcludeNewer {
    /// Create a new exclude newer configuration with just a global timestamp.
    pub fn global(global: ExcludeNewerTimestamp) -> Self {
        Self {
            global: Some(global),
            package: ExcludeNewerPackage::default(),
        }
    }

    /// Create a new exclude newer configuration.
    pub fn new(global: Option<ExcludeNewerTimestamp>, package: ExcludeNewerPackage) -> Self {
        Self { global, package }
    }

    /// Create from CLI arguments.
    pub fn from_args(
        global: Option<ExcludeNewerTimestamp>,
        package: Vec<ExcludeNewerPackageEntry>,
    ) -> Self {
        let package: ExcludeNewerPackage = package.into_iter().collect();

        Self { global, package }
    }

    /// Returns the timestamp for a specific package, falling back to the global timestamp if set.
    pub fn exclude_newer_package(
        &self,
        package_name: &PackageName,
    ) -> Option<ExcludeNewerTimestamp> {
        self.package.get(package_name).copied().or(self.global)
    }

    /// Returns true if this has any configuration (global or per-package).
    pub fn is_empty(&self) -> bool {
        self.global.is_none() && self.package.is_empty()
    }
}

impl std::fmt::Display for ExcludeNewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(global) = self.global {
            write!(f, "global: {global}")?;
            if !self.package.is_empty() {
                write!(f, ", ")?;
            }
        }
        let mut first = true;
        for (name, timestamp) in &self.package {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{name}: {timestamp}")?;
            first = false;
        }
        Ok(())
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ExcludeNewerTimestamp {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ExcludeNewerTimestamp")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(Z|[+-]\d{2}:\d{2}))?$",
            "description": "Exclude distributions uploaded after the given timestamp.\n\nAccepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same format (e.g., `2006-12-02`).",
        })
    }
}
