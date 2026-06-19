use std::borrow::Cow;
use std::str::FromStr;

use jiff::{Span, Timestamp, ToSpan, Unit, tz::TimeZone};
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;

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

impl PartialOrd for ExcludeNewerSpan {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExcludeNewerSpan {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.to_string().cmp(&other.0.to_string())
    }
}

impl std::hash::Hash for ExcludeNewerSpan {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_string().hash(state);
    }
}

impl serde::Serialize for ExcludeNewerSpan {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExcludeNewerValue {
    /// An absolute timestamp.
    Absolute(Timestamp),
    /// A span used to compute a timestamp relative to the current time.
    Relative(ExcludeNewerSpan),
}

impl ExcludeNewerValue {
    /// A placeholder timestamp used when serializing a [`Relative`](Self::Relative)
    /// value to a wire format that requires a timestamp field.
    pub const PLACEHOLDER: &'static str = "0001-01-01T00:00:00Z";

    /// Return the effective [`Timestamp`].
    ///
    /// For [`Relative`](Self::Relative) values this is computed from the span and the current
    /// time on each call.
    pub fn timestamp(&self) -> Timestamp {
        match self {
            Self::Absolute(timestamp) => *timestamp,
            Self::Relative(span) => {
                let now = current_time();
                now.checked_sub(span.0.abs())
                    .map_or(now.timestamp(), |cutoff| cutoff.timestamp())
            }
        }
    }

    /// Return the [`ExcludeNewerSpan`], if any.
    pub fn span(&self) -> Option<&ExcludeNewerSpan> {
        match self {
            Self::Absolute(_) => None,
            Self::Relative(span) => Some(span),
        }
    }

    /// Create a new [`ExcludeNewerValue`] from an absolute timestamp.
    pub fn absolute(timestamp: Timestamp) -> Self {
        Self::Absolute(timestamp)
    }

    /// Create a new [`ExcludeNewerValue`] from a relative span.
    pub fn relative(span: ExcludeNewerSpan) -> Self {
        Self::Relative(span)
    }
}

/// Return the current time, respecting the `UV_TEST_CURRENT_TIMESTAMP` override.
fn current_time() -> jiff::Zoned {
    if let Ok(test_time) = std::env::var("UV_TEST_CURRENT_TIMESTAMP") {
        test_time
            .parse::<Timestamp>()
            .expect("UV_TEST_CURRENT_TIMESTAMP must be a valid RFC 3339 timestamp")
            .to_zoned(TimeZone::UTC)
    } else {
        Timestamp::now().to_zoned(TimeZone::UTC)
    }
}

impl serde::Serialize for ExcludeNewerValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.timestamp().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ExcludeNewerValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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
            Helper::Table(table) => Ok(match table.span {
                Some(span) => Self::relative(span),
                None => Self::absolute(table.timestamp),
            }),
        }
    }
}

impl From<Timestamp> for ExcludeNewerValue {
    fn from(timestamp: Timestamp) -> Self {
        Self::Absolute(timestamp)
    }
}

impl From<ExcludeNewerSpan> for ExcludeNewerValue {
    fn from(span: ExcludeNewerSpan) -> Self {
        Self::Relative(span)
    }
}

fn format_exclude_newer_error(
    input: &str,
    date_err: &jiff::Error,
    span_err: &jiff::Error,
) -> String {
    let trimmed = input.trim();

    let after_sign = trimmed.trim_start_matches(['+', '-']);
    if after_sign.starts_with('P') || after_sign.starts_with('p') {
        return format!("`{input}` could not be parsed as an ISO 8601 duration: {span_err}");
    }

    let after_sign_trimmed = after_sign.trim_start();
    let mut chars = after_sign_trimmed.chars().peekable();
    if chars.peek().is_some_and(char::is_ascii_digit) {
        while chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_some_and(char::is_ascii_alphabetic) {
            return format!("`{input}` could not be parsed as a duration: {span_err}");
        }
    }

    let mut chars = after_sign.chars();
    let looks_like_date = chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.next().is_some_and(|c| c == '-');

    if looks_like_date {
        return format!("`{input}` could not be parsed as a valid date: {date_err}");
    }

    format!(
        "`{input}` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)"
    )
}

impl FromStr for ExcludeNewerValue {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if let Ok(timestamp) = input.parse::<Timestamp>() {
            return Ok(Self::absolute(timestamp));
        }

        let date_err = match input.parse::<jiff::civil::Date>() {
            Ok(date) => {
                let timestamp = date
                    .checked_add(1.day())
                    .and_then(|date| date.to_zoned(TimeZone::system()))
                    .map(|zdt| zdt.timestamp())
                    .map_err(|err| {
                        format!(
                            "`{input}` parsed to date `{date}`, but could not be converted to a timestamp: {err}",
                        )
                    })?;
                return Ok(Self::absolute(timestamp));
            }
            Err(err) => err,
        };

        let span_err = match input.parse::<Span>() {
            Ok(span) => {
                let now = if let Ok(test_time) = std::env::var("UV_TEST_CURRENT_TIMESTAMP") {
                    test_time
                        .parse::<Timestamp>()
                        .expect("UV_TEST_CURRENT_TIMESTAMP must be a valid RFC 3339 timestamp")
                        .to_zoned(TimeZone::UTC)
                } else {
                    Timestamp::now().to_zoned(TimeZone::UTC)
                };

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

                now.checked_sub(span.abs()).map_err(|err| {
                    format!("Duration `{input}` is too large to subtract from current time: {err}")
                })?;
                return Ok(Self::relative(ExcludeNewerSpan(span)));
            }
            Err(err) => err,
        };

        Err(format_exclude_newer_error(input, &date_err, &span_err))
    }
}

impl std::fmt::Display for ExcludeNewerValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.timestamp().fmt(f)
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

/// Whether `exclude-newer` is disabled or enabled with an explicit cutoff.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExcludeNewerOverride {
    /// Disable exclude-newer (allow all versions regardless of upload date).
    Disabled,
    /// Enable exclude-newer with this cutoff.
    Enabled(Box<ExcludeNewerValue>),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ExcludeNewerOverride {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ExcludeNewerOverride")
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "boolean",
                    "const": false,
                    "description": "Disable exclude-newer."
                },
                generator.subschema_for::<ExcludeNewerValue>(),
            ]
        })
    }
}

impl<'de> serde::Deserialize<'de> for ExcludeNewerOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ExcludeNewerOverride;

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
                    .map(|ts| ExcludeNewerOverride::Enabled(Box::new(ts)))
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
                    Ok(ExcludeNewerOverride::Disabled)
                }
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                Ok(ExcludeNewerOverride::Enabled(Box::new(
                    ExcludeNewerValue::deserialize(MapAccessDeserializer::new(map))?,
                )))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl serde::Serialize for ExcludeNewerOverride {
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
