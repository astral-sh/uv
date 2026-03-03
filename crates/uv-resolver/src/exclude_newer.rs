use std::borrow::Cow;
use std::{
    ops::{Deref, DerefMut},
    str::FromStr,
};

use jiff::{Span, Timestamp, ToSpan, Unit, tz::TimeZone};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use uv_normalize::PackageName;

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
    PackageAdded(PackageName, PackageExcludeNewer),
    PackageRemoved(PackageName),
    PackageChanged(PackageName, Box<PackageExcludeNewerChange>),
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
            Self::PackageAdded(name, PackageExcludeNewer::Enabled(value)) => {
                write!(
                    f,
                    "addition of exclude newer `{}` for package `{name}`",
                    value.as_ref()
                )
            }
            Self::PackageAdded(name, PackageExcludeNewer::Disabled) => {
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
/// A timestamp that excludes files newer than it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeNewerValue {
    /// The resolved timestamp.
    timestamp: Timestamp,
    /// The span used to derive the [`Timestamp`], if any.
    span: Option<ExcludeNewerSpan>,
}

impl ExcludeNewerValue {
    pub fn into_parts(self) -> (Timestamp, Option<ExcludeNewerSpan>) {
        (self.timestamp, self.span)
    }

    pub fn compare(&self, other: &Self) -> Option<ExcludeNewerValueChange> {
        match (&self.span, &other.span) {
            (None, Some(span)) => Some(ExcludeNewerValueChange::SpanAdded(*span)),
            (Some(_), None) => Some(ExcludeNewerValueChange::SpanRemoved),
            (Some(self_span), Some(other_span)) if self_span != other_span => Some(
                ExcludeNewerValueChange::SpanChanged(*self_span, *other_span),
            ),
            (Some(_), Some(span)) if self.timestamp != other.timestamp => {
                Some(ExcludeNewerValueChange::RelativeTimestampChanged(
                    self.timestamp,
                    other.timestamp,
                    *span,
                ))
            }
            (None, None) if self.timestamp != other.timestamp => Some(
                ExcludeNewerValueChange::AbsoluteTimestampChanged(self.timestamp, other.timestamp),
            ),
            (Some(_), Some(_)) | (None, None) => None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ExcludeNewerSpan(Span);

impl std::fmt::Display for ExcludeNewerSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ExcludeNewerSpan {
    fn eq(&self, other: &Self) -> bool {
        self.0.fieldwise() == other.0.fieldwise()
    }
}

impl Eq for ExcludeNewerSpan {}

impl serde::Serialize for ExcludeNewerSpan {
    /// Serialize to an ISO 8601 duration string.
    ///
    /// We use ISO 8601 format for serialization (rather than the "friendly" format).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for ExcludeNewerSpan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <Cow<'_, str>>::deserialize(deserializer)?;
        let span: Span = s.parse().map_err(serde::de::Error::custom)?;
        Ok(Self(span))
    }
}

impl serde::Serialize for ExcludeNewerValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.timestamp.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ExcludeNewerValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Support both a simple string ("2024-03-11T00:00:00Z") and a table
        // ({ timestamp = "2024-03-11T00:00:00Z", span = "P2W" })
        #[derive(serde::Deserialize)]
        struct TableForm {
            timestamp: Timestamp,
            span: Option<ExcludeNewerSpan>,
        }

        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Helper {
            String(String),
            Table(Box<TableForm>),
        }

        match Helper::deserialize(deserializer)? {
            Helper::String(s) => Self::from_str(&s).map_err(serde::de::Error::custom),
            Helper::Table(table) => Ok(Self::new(table.timestamp, table.span)),
        }
    }
}

impl ExcludeNewerValue {
    /// Return the [`Timestamp`] in milliseconds.
    pub fn timestamp_millis(&self) -> i64 {
        self.timestamp.as_millisecond()
    }

    /// Return the [`Timestamp`].
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Return the [`ExcludeNewerSpan`] used to construct the [`Timestamp`], if any.
    pub fn span(&self) -> Option<&ExcludeNewerSpan> {
        self.span.as_ref()
    }

    /// Create a new [`ExcludeNewerValue`].
    pub fn new(timestamp: Timestamp, span: Option<ExcludeNewerSpan>) -> Self {
        Self { timestamp, span }
    }
}

impl From<Timestamp> for ExcludeNewerValue {
    fn from(timestamp: Timestamp) -> Self {
        Self {
            timestamp,
            span: None,
        }
    }
}

/// Determine what format the user likely intended and return an appropriate error message.
fn format_exclude_newer_error(
    input: &str,
    date_err: &jiff::Error,
    span_err: &jiff::Error,
) -> String {
    let trimmed = input.trim();

    // Check for ISO 8601 duration (`[-+]?[Pp]`), e.g., "P2W", "+P1D", "-P30D"
    let after_sign = trimmed.trim_start_matches(['+', '-']);
    if after_sign.starts_with('P') || after_sign.starts_with('p') {
        return format!("`{input}` could not be parsed as an ISO 8601 duration: {span_err}");
    }

    // Check for friendly duration (`[-+]?\s*[0-9]+\s*[A-Za-z]`), e.g., "2 weeks", "-30 days",
    // "1hour"
    let after_sign_trimmed = after_sign.trim_start();
    let mut chars = after_sign_trimmed.chars().peekable();

    // Check if we start with a digit
    if chars.peek().is_some_and(char::is_ascii_digit) {
        // Skip digits
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        }
        // Skip optional whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        // Check if next character is a letter (unit designator)
        if chars.peek().is_some_and(char::is_ascii_alphabetic) {
            return format!("`{input}` could not be parsed as a duration: {span_err}");
        }
    }

    // Check for date/timestamp (`[-+]?[0-9]{4}-`), e.g., "2024-01-01", "2024-01-01T00:00:00Z"
    let mut chars = after_sign.chars();
    let looks_like_date = chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c == '-');

    if looks_like_date {
        return format!("`{input}` could not be parsed as a valid date: {date_err}");
    }

    // If we can't tell, return a generic error message
    format!(
        "`{input}` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)"
    )
}

impl FromStr for ExcludeNewerValue {
    type Err = String;

    /// Parse an [`ExcludeNewerValue`] from a string.
    ///
    /// Accepts RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`), local dates in the same format
    /// (e.g., `2006-12-02`), "friendly" durations (e.g., `1 week`, `30 days`), and ISO 8601
    /// durations (e.g., `PT24H`, `P7D`, `P30D`).
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // Try parsing as a timestamp first
        if let Ok(timestamp) = input.parse::<Timestamp>() {
            return Ok(Self::new(timestamp, None));
        }

        // Try parsing as a date
        // In Jiff, if an RFC 3339 timestamp could be parsed, then it must necessarily be the case
        // that a date can also be parsed. So we can collapse the error cases here. That is, if we
        // fail to parse a timestamp and a date, then it should be sufficient to just report the
        // error from parsing the date. If someone tried to write a timestamp but committed an error
        // in the non-date portion, the date parsing below will still report a holistic error that
        // will make sense to the user. (I added a snapshot test for that case.)
        let date_err = match input.parse::<jiff::civil::Date>() {
            Ok(date) => {
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
                return Ok(Self::new(timestamp, None));
            }
            Err(err) => err,
        };

        // Try parsing as a span
        let span_err = match input.parse::<Span>() {
            Ok(span) => {
                // Allow overriding the current time in tests for deterministic snapshots
                let now = if let Ok(test_time) = std::env::var("UV_TEST_CURRENT_TIMESTAMP") {
                    test_time
                        .parse::<Timestamp>()
                        .expect("UV_TEST_CURRENT_TIMESTAMP must be a valid RFC 3339 timestamp")
                        .to_zoned(TimeZone::UTC)
                } else {
                    Timestamp::now().to_zoned(TimeZone::UTC)
                };

                // We do not allow years and months as units, as the amount of time they represent
                // is not fixed and can differ depending on the local time zone. We could allow this
                // via the CLI in the future, but shouldn't allow it via persistent configuration.
                if span.get_years() != 0 {
                    let years = span
                        .total((Unit::Year, &now))
                        .map(f64::ceil)
                        .unwrap_or(1.0)
                        .abs();
                    let days = years * 365.0;
                    return Err(format!(
                        "Duration `{input}` uses unit 'years' which is not allowed; use days instead, e.g., `{days:.0} days`.",
                    ));
                }
                if span.get_months() != 0 {
                    let months = span
                        .total((Unit::Month, &now))
                        .map(f64::ceil)
                        .unwrap_or(1.0)
                        .abs();
                    let days = months * 30.0;
                    return Err(format!(
                        "Duration `{input}` uses 'months' which is not allowed; use days instead, e.g., `{days:.0} days`."
                    ));
                }

                // We're using a UTC timezone so there are no transitions (e.g., DST) and days are
                // always 24 hours. This means that we can also allow weeks as a unit.
                //
                // Note we use `span.abs()` so `1 day ago` has the same effect as `1 day` instead
                // of resulting in a future date.
                let cutoff = now.checked_sub(span.abs()).map_err(|err| {
                    format!("Duration `{input}` is too large to subtract from current time: {err}")
                })?;

                return Ok(Self::new(cutoff.into(), Some(ExcludeNewerSpan(span))));
            }
            Err(err) => err,
        };

        // Return a targeted error message based on heuristics about what the user likely intended
        Err(format_exclude_newer_error(input, &date_err, &span_err))
    }
}

impl std::fmt::Display for ExcludeNewerValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.timestamp.fmt(f)
    }
}

/// Per-package exclude-newer setting.
///
/// This enum represents whether exclude-newer should be disabled for a package,
/// or if a specific cutoff (absolute or relative) should be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageExcludeNewer {
    /// Disable exclude-newer for this package (allow all versions regardless of upload date).
    Disabled,
    /// Enable exclude-newer with this cutoff for this package.
    Enabled(Box<ExcludeNewerValue>),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PackageExcludeNewer {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PackageExcludeNewer")
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "boolean",
                    "const": false,
                    "description": "Disable exclude-newer for this package."
                },
                generator.subschema_for::<ExcludeNewerValue>(),
            ]
        })
    }
}

/// A package-specific exclude-newer entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExcludeNewerPackageEntry {
    pub package: PackageName,
    pub setting: PackageExcludeNewer,
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
            PackageExcludeNewer::Disabled
        } else {
            PackageExcludeNewer::Enabled(Box::new(ExcludeNewerValue::from_str(value).map_err(
                |err| format!("Invalid `exclude-newer-package` value `{value}`: {err}"),
            )?))
        };

        Ok(Self { package, setting })
    }
}

impl From<(PackageName, PackageExcludeNewer)> for ExcludeNewerPackageEntry {
    fn from((package, setting): (PackageName, PackageExcludeNewer)) -> Self {
        Self { package, setting }
    }
}

impl From<(PackageName, ExcludeNewerValue)> for ExcludeNewerPackageEntry {
    fn from((package, timestamp): (PackageName, ExcludeNewerValue)) -> Self {
        Self {
            package,
            setting: PackageExcludeNewer::Enabled(Box::new(timestamp)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for PackageExcludeNewer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = PackageExcludeNewer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a date/timestamp/duration string, false to disable exclude-newer, or a table \
                     with timestamp/span",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                ExcludeNewerValue::from_str(v)
                    .map(|ts| PackageExcludeNewer::Enabled(Box::new(ts)))
                    .map_err(|e| E::custom(format!("failed to parse exclude-newer value: {e}")))
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v {
                    Err(E::custom(
                        "expected false to disable exclude-newer, got true",
                    ))
                } else {
                    Ok(PackageExcludeNewer::Disabled)
                }
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                Ok(PackageExcludeNewer::Enabled(Box::new(
                    ExcludeNewerValue::deserialize(MapAccessDeserializer::new(map))?,
                )))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl serde::Serialize for PackageExcludeNewer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Enabled(timestamp) => timestamp.to_string().serialize(serializer),
            Self::Disabled => serializer.serialize_bool(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageExcludeNewerChange {
    Disabled { was: ExcludeNewerValue },
    Enabled { now: ExcludeNewerValue },
    TimestampChanged(ExcludeNewerValueChange),
}

impl PackageExcludeNewerChange {
    pub fn is_relative_timestamp_change(&self) -> bool {
        match self {
            Self::Disabled { .. } | Self::Enabled { .. } => false,
            Self::TimestampChanged(change) => change.is_relative_timestamp_change(),
        }
    }
}

impl std::fmt::Display for PackageExcludeNewerChange {
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
pub struct ExcludeNewerPackage(FxHashMap<PackageName, PackageExcludeNewer>);

impl Deref for ExcludeNewerPackage {
    type Target = FxHashMap<PackageName, PackageExcludeNewer>;

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
    type Item = (PackageName, PackageExcludeNewer);
    type IntoIter = std::collections::hash_map::IntoIter<PackageName, PackageExcludeNewer>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ExcludeNewerPackage {
    type Item = (&'a PackageName, &'a PackageExcludeNewer);
    type IntoIter = std::collections::hash_map::Iter<'a, PackageName, PackageExcludeNewer>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl ExcludeNewerPackage {
    /// Convert to the inner `HashMap`.
    pub fn into_inner(self) -> FxHashMap<PackageName, PackageExcludeNewer> {
        self.0
    }

    /// Returns true if this map is empty (no package-specific settings).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn compare(&self, other: &Self) -> Option<ExcludeNewerPackageChange> {
        for (package, setting) in self {
            match (setting, other.get(package)) {
                (
                    PackageExcludeNewer::Enabled(self_timestamp),
                    Some(PackageExcludeNewer::Enabled(other_timestamp)),
                ) => {
                    if let Some(change) = self_timestamp.compare(other_timestamp) {
                        return Some(ExcludeNewerPackageChange::PackageChanged(
                            package.clone(),
                            Box::new(PackageExcludeNewerChange::TimestampChanged(change)),
                        ));
                    }
                }
                (
                    PackageExcludeNewer::Enabled(self_timestamp),
                    Some(PackageExcludeNewer::Disabled),
                ) => {
                    return Some(ExcludeNewerPackageChange::PackageChanged(
                        package.clone(),
                        Box::new(PackageExcludeNewerChange::Disabled {
                            was: self_timestamp.as_ref().clone(),
                        }),
                    ));
                }
                (
                    PackageExcludeNewer::Disabled,
                    Some(PackageExcludeNewer::Enabled(other_timestamp)),
                ) => {
                    return Some(ExcludeNewerPackageChange::PackageChanged(
                        package.clone(),
                        Box::new(PackageExcludeNewerChange::Enabled {
                            now: other_timestamp.as_ref().clone(),
                        }),
                    ));
                }
                (PackageExcludeNewer::Disabled, Some(PackageExcludeNewer::Disabled)) => {}
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
            Some(PackageExcludeNewer::Enabled(timestamp)) => Some(timestamp.as_ref().clone()),
            Some(PackageExcludeNewer::Disabled) => None,
            None => self.global.clone(),
        }
    }

    /// Returns true if this has any configuration (global or per-package).
    pub fn is_empty(&self) -> bool {
        self.global.is_none() && self.package.is_empty()
    }

    pub fn compare(&self, other: &Self) -> Option<ExcludeNewerChange> {
        match (&self.global, &other.global) {
            (Some(self_global), Some(other_global)) => {
                if let Some(change) = self_global.compare(other_global) {
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
                PackageExcludeNewer::Enabled(timestamp) => {
                    write!(f, "{name}: {}", timestamp.as_ref())?;
                }
                PackageExcludeNewer::Disabled => {
                    write!(f, "{name}: disabled")?;
                }
            }
            first = false;
        }
        Ok(())
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ExcludeNewerValue {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ExcludeNewerValue")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Exclude distributions uploaded after the given timestamp.\n\nAccepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same format (e.g., `2006-12-02`), as well as relative durations (e.g., `1 week`, `30 days`, `6 months`). Relative durations are resolved to a timestamp at lock time.",
        })
    }
}
