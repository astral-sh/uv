use std::str::FromStr;

use jiff::{tz::TimeZone, Timestamp, ToSpan};

/// A timestamp that excludes files newer than it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ExcludeNewer(Timestamp);

impl ExcludeNewer {
    /// Returns the timestamp in milliseconds.
    pub fn timestamp_millis(&self) -> i64 {
        self.0.as_millisecond()
    }
}

impl From<Timestamp> for ExcludeNewer {
    fn from(timestamp: Timestamp) -> Self {
        Self(timestamp)
    }
}

impl FromStr for ExcludeNewer {
    type Err = String;

    /// Parse an [`ExcludeNewer`] from a string.
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

impl std::fmt::Display for ExcludeNewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ExcludeNewer {
    fn schema_name() -> String {
        "ExcludeNewer".to_string()
    }

    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            string: Some(Box::new(schemars::schema::StringValidation {
                pattern: Some(
                    r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(Z|[+-]\d{2}:\d{2}))?$".to_string(),
                ),
                ..schemars::schema::StringValidation::default()
            })),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("Exclude distributions uploaded after the given timestamp.\n\nAccepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and local dates in the same format (e.g., `2006-12-02`).".to_string()),
              ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}
