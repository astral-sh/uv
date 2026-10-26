use std::assert_matches;
use std::collections::BTreeMap;
use std::io::{self, Read};

use serde::Deserialize;
use serde::de::value::{Error, StringDeserializer};
use uv_macros::DeserializeUntagged;

#[derive(Debug, PartialEq, DeserializeUntagged)]
#[serde(deny_unknown_fields)]
enum Setting<'a, T> {
    #[untagged(string)]
    Name(&'a str),
    #[untagged(bool)]
    Enabled(bool),
    #[untagged(seq)]
    List(Vec<T>),
    #[serde(rename_all = "kebab-case")]
    Table {
        first_item: T,
        #[serde(default)]
        enabled: bool,
    },
}

#[test]
fn dispatches_by_shape() -> serde_json::Result<()> {
    let text = String::from(r#""uv""#);
    assert_eq!(
        serde_json::from_str::<Setting<u8>>(&text)?,
        Setting::Name(&text[1..3])
    );
    assert_eq!(
        serde_json::from_str::<Setting<u8>>("true")?,
        Setting::Enabled(true)
    );
    assert_eq!(
        serde_json::from_str::<Setting<u8>>("[5, 8]")?,
        Setting::List(vec![5, 8])
    );
    assert_eq!(
        serde_json::from_str::<Setting<u8>>(r#"{"first-item": 5}"#)?,
        Setting::Table {
            first_item: 5,
            enabled: false
        }
    );
    Ok(())
}

#[derive(Debug, PartialEq, DeserializeUntagged)]
enum Owned {
    #[untagged(string)]
    Name(String),
    #[untagged(map)]
    Table(BTreeMap<String, u8>),
}

#[test]
fn moves_owned_strings_into_the_variant() -> Result<(), Error> {
    let text = String::from("uv");
    let address = text.as_ptr();
    assert_matches!(Owned::deserialize(StringDeserializer::new(text))?, Owned::Name(value) if value.as_ptr() == address);
    Ok(())
}

#[test]
fn rejects_an_unknown_field_before_reading_its_value() {
    let error = serde_json::from_str::<Setting<u8>>(r#"{"unknown": !!!}"#).unwrap_err();
    assert_eq!(
        error.to_string(),
        "unknown field `unknown`, expected `first-item` or `enabled` at line 1 column 10"
    );
}

#[test]
fn retains_the_readers_error_inside_a_map() {
    struct FailedRead;

    impl Read for FailedRead {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read failed"))
        }
    }

    let reader = br#"{"key":"#.as_slice().chain(FailedRead);
    assert!(
        serde_json::from_reader::<_, Owned>(reader)
            .unwrap_err()
            .is_io()
    );
}
