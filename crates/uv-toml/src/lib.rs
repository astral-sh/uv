mod de;

use std::fmt;

use serde::de::DeserializeOwned;
use toml_spanner::Arena;

/// Error type for TOML parsing and deserialization.
#[derive(Debug)]
pub enum Error {
    /// A parse error from `toml_spanner`.
    Parse(toml_spanner::Error),
    /// A serde deserialization error.
    Message(String),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(err) => write!(f, "{err}"),
            Error::Message(msg) => f.write_str(msg),
        }
    }
}

impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

/// Parse a TOML string into a deserializable type.
///
/// Uses `toml_spanner` for parsing and a serde `Deserializer` adapter for
/// deserialization, so existing `#[derive(serde::Deserialize)]` types work
/// unchanged.
pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T, Error> {
    let arena = Arena::new();
    let root = toml_spanner::parse(s, &arena).map_err(Error::Parse)?;
    T::deserialize(de::TableDeserializer::new(root.table()))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[test]
    fn test_basic_string() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            name: String,
        }
        let result: Config = from_str("name = 'hello'").unwrap();
        assert_eq!(result.name, "hello");
    }

    #[test]
    fn test_numbers() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            integer: i64,
            float: f64,
        }
        let result: Config = from_str("integer = 42\nfloat = 3.14").unwrap();
        assert_eq!(result.integer, 42);
        assert!((result.float - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_bool() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            enabled: bool,
        }
        let result: Config = from_str("enabled = true").unwrap();
        assert!(result.enabled);
    }

    #[test]
    fn test_array() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            items: Vec<String>,
        }
        let result: Config = from_str("items = ['a', 'b', 'c']").unwrap();
        assert_eq!(result.items, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_nested_table() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            server: Server,
        }
        #[derive(Deserialize, Debug, PartialEq)]
        struct Server {
            host: String,
            port: u16,
        }
        let result: Config = from_str("[server]\nhost = 'localhost'\nport = 8080").unwrap();
        assert_eq!(result.server.host, "localhost");
        assert_eq!(result.server.port, 8080);
    }

    #[test]
    fn test_option() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            name: String,
            #[serde(default)]
            missing: Option<String>,
        }
        let result: Config = from_str("name = 'hello'").unwrap();
        assert_eq!(result.name, "hello");
        assert_eq!(result.missing, None);
    }

    #[test]
    fn test_enum_string_variant() {
        #[derive(Deserialize, Debug, PartialEq)]
        #[serde(rename_all = "lowercase")]
        enum Color {
            Red,
            Green,
            Blue,
        }
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            color: Color,
        }
        let result: Config = from_str("color = 'red'").unwrap();
        assert_eq!(result.color, Color::Red);
    }

    #[test]
    fn test_datetime() {
        #[derive(Deserialize, Debug)]
        struct Config {
            date: String,
        }
        // DateTime is deserialized as a string when target is String
        let result: Config = from_str("date = 2024-01-15T10:30:00Z").unwrap();
        assert_eq!(result.date, "2024-01-15T10:30:00Z");
    }

    #[test]
    fn test_array_of_tables() {
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            items: Vec<Item>,
        }
        #[derive(Deserialize, Debug, PartialEq)]
        struct Item {
            name: String,
            value: i64,
        }
        let toml = "[[items]]\nname = 'a'\nvalue = 1\n\n[[items]]\nname = 'b'\nvalue = 2";
        let result: Config = from_str(toml).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].name, "a");
        assert_eq!(result.items[1].value, 2);
    }

    #[test]
    fn test_parse_error() {
        #[derive(Deserialize, Debug)]
        struct Config {
            _name: String,
        }
        let result = from_str::<Config>("invalid toml %%");
        assert!(result.is_err());
    }

    #[test]
    fn test_ignored_any() {
        // Test that extra fields can be ignored
        #[derive(Deserialize, Debug, PartialEq)]
        struct Config {
            name: String,
        }
        let result: Config = from_str("name = 'hello'\nextra = 42").unwrap();
        assert_eq!(result.name, "hello");
    }

    #[test]
    fn test_kebab_case_keys() {
        #[derive(Deserialize, Debug, PartialEq)]
        #[serde(rename_all = "kebab-case")]
        struct Config {
            my_value: String,
        }
        let result: Config = from_str("my-value = 'test'").unwrap();
        assert_eq!(result.my_value, "test");
    }
}
