use std::str::FromStr;

use chrono::{DateTime, Days, NaiveDate, NaiveTime, Utc};

/// A timestamp that excludes files newer than it.
#[derive(Debug, Copy, Clone)]
pub struct ExcludeNewer(DateTime<Utc>);

impl ExcludeNewer {
    /// Returns the timestamp in milliseconds.
    pub fn timestamp_millis(&self) -> i64 {
        self.0.timestamp_millis()
    }
}

impl From<DateTime<Utc>> for ExcludeNewer {
    fn from(datetime: DateTime<Utc>) -> Self {
        Self(datetime)
    }
}

impl FromStr for ExcludeNewer {
    type Err = String;

    /// Parse an [`ExcludeNewer`] from a string.
    ///
    /// Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
    /// format (e.g., `2006-12-02`).
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let date_err = match NaiveDate::from_str(input) {
            Ok(date) => {
                // Midnight that day is 00:00:00 the next day
                return Ok(Self(
                    (date + Days::new(1)).and_time(NaiveTime::MIN).and_utc(),
                ));
            }
            Err(err) => err,
        };
        let datetime_err = match DateTime::parse_from_rfc3339(input) {
            Ok(datetime) => return Ok(Self(datetime.with_timezone(&Utc))),
            Err(err) => err,
        };
        Err(format!(
            "`{input}` is neither a valid date ({date_err}) nor a valid datetime ({datetime_err})"
        ))
    }
}

impl std::fmt::Display for ExcludeNewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
