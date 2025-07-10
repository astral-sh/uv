//! Taken directly from Ruff.
//!
//! See: <https://github.com/astral-sh/ruff/blob/dc8db1afb08704ad6a788c497068b01edf8b460d/crates/ruff_workspace/sr.rs>

use serde::{Serialize, Serializer};
use std::collections::BTreeMap;

use std::fmt::{Debug, Display, Formatter};

/// Visits [`OptionsMetadata`].
///
/// An instance of [`Visit`] represents the logic for inspecting an object's options metadata.
pub trait Visit {
    /// Visits an [`OptionField`] value named `name`.
    fn record_field(&mut self, name: &str, field: OptionField);

    /// Visits an [`OptionSet`] value named `name`.
    fn record_set(&mut self, name: &str, group: OptionSet);
}

/// Returns metadata for its options.
pub trait OptionsMetadata {
    /// Visits the options metadata of this object by calling `visit` for each option.
    fn record(visit: &mut dyn Visit);

    fn documentation() -> Option<&'static str> {
        None
    }

    /// Returns the extracted metadata.
    fn metadata() -> OptionSet
    where
        Self: Sized + 'static,
    {
        OptionSet::of::<Self>()
    }
}

impl<T> OptionsMetadata for Option<T>
where
    T: OptionsMetadata,
{
    fn record(visit: &mut dyn Visit) {
        T::record(visit);
    }
}

/// Metadata of an option that can either be a [`OptionField`] or [`OptionSet`].
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(untagged)]
pub enum OptionEntry {
    /// A single option.
    Field(OptionField),

    /// A set of options.
    Set(OptionSet),
}

impl Display for OptionEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionEntry::Set(set) => std::fmt::Display::fmt(set, f),
            OptionEntry::Field(field) => std::fmt::Display::fmt(&field, f),
        }
    }
}

/// A set of options.
///
/// It extracts the options by calling the [`OptionsMetadata::record`] of a type implementing
/// [`OptionsMetadata`].
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct OptionSet {
    record: fn(&mut dyn Visit),
    doc: fn() -> Option<&'static str>,
}

impl OptionSet {
    pub fn of<T>() -> Self
    where
        T: OptionsMetadata + 'static,
    {
        Self {
            record: T::record,
            doc: T::documentation,
        }
    }

    /// Visits the options in this set by calling `visit` for each option.
    pub fn record(&self, visit: &mut dyn Visit) {
        let record = self.record;
        record(visit);
    }

    pub fn documentation(&self) -> Option<&'static str> {
        let documentation = self.doc;
        documentation()
    }

    /// Returns `true` if this set has an option that resolves to `name`.
    ///
    /// The name can be separated by `.` to find a nested option.
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Returns `Some` if this set has an option that resolves to `name` and `None` otherwise.
    ///
    /// The name can be separated by `.` to find a nested option.
    pub fn find(&self, name: &str) -> Option<OptionEntry> {
        struct FindOptionVisitor<'a> {
            option: Option<OptionEntry>,
            parts: std::str::Split<'a, char>,
            needle: &'a str,
        }

        impl Visit for FindOptionVisitor<'_> {
            fn record_set(&mut self, name: &str, set: OptionSet) {
                if self.option.is_none() && name == self.needle {
                    if let Some(next) = self.parts.next() {
                        self.needle = next;
                        set.record(self);
                    } else {
                        self.option = Some(OptionEntry::Set(set));
                    }
                }
            }

            fn record_field(&mut self, name: &str, field: OptionField) {
                if self.option.is_none() && name == self.needle {
                    if self.parts.next().is_none() {
                        self.option = Some(OptionEntry::Field(field));
                    }
                }
            }
        }

        let mut parts = name.split('.');

        if let Some(first) = parts.next() {
            let mut visitor = FindOptionVisitor {
                parts,
                needle: first,
                option: None,
            };

            self.record(&mut visitor);
            visitor.option
        } else {
            None
        }
    }
}

/// Visitor that writes out the names of all fields and sets.
struct DisplayVisitor<'fmt, 'buf> {
    f: &'fmt mut Formatter<'buf>,
    result: std::fmt::Result,
}

impl<'fmt, 'buf> DisplayVisitor<'fmt, 'buf> {
    fn new(f: &'fmt mut Formatter<'buf>) -> Self {
        Self { f, result: Ok(()) }
    }

    fn finish(self) -> std::fmt::Result {
        self.result
    }
}

impl Visit for DisplayVisitor<'_, '_> {
    fn record_set(&mut self, name: &str, _: OptionSet) {
        self.result = self.result.and_then(|()| writeln!(self.f, "{name}"));
    }

    fn record_field(&mut self, name: &str, field: OptionField) {
        self.result = self.result.and_then(|()| {
            write!(self.f, "{name}")?;

            if field.deprecated.is_some() {
                write!(self.f, " (deprecated)")?;
            }

            writeln!(self.f)
        });
    }
}

impl Display for OptionSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut visitor = DisplayVisitor::new(f);
        self.record(&mut visitor);
        visitor.finish()
    }
}

struct SerializeVisitor<'a> {
    entries: &'a mut BTreeMap<String, OptionField>,
}

impl Visit for SerializeVisitor<'_> {
    fn record_set(&mut self, name: &str, set: OptionSet) {
        // Collect the entries of the set.
        let mut entries = BTreeMap::new();
        let mut visitor = SerializeVisitor {
            entries: &mut entries,
        };
        set.record(&mut visitor);

        // Insert the set into the entries.
        for (key, value) in entries {
            self.entries.insert(format!("{name}.{key}"), value);
        }
    }

    fn record_field(&mut self, name: &str, field: OptionField) {
        self.entries.insert(name.to_string(), field);
    }
}

impl Serialize for OptionSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries = BTreeMap::new();
        let mut visitor = SerializeVisitor {
            entries: &mut entries,
        };
        self.record(&mut visitor);
        entries.serialize(serializer)
    }
}

impl Debug for OptionSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize)]
pub struct OptionField {
    pub doc: &'static str,
    /// Ex) `"false"`
    pub default: &'static str,
    /// Ex) `"bool"`
    pub value_type: &'static str,
    /// Ex) `"per-file-ignores"`
    pub scope: Option<&'static str>,
    pub example: &'static str,
    pub deprecated: Option<Deprecated>,
    pub possible_values: Option<Vec<PossibleValue>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Deprecated {
    pub since: Option<&'static str>,
    pub message: Option<&'static str>,
}

impl Display for OptionField {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.doc)?;
        writeln!(f)?;

        writeln!(f, "Default value: {}", self.default)?;

        if let Some(possible_values) = self
            .possible_values
            .as_ref()
            .filter(|values| !values.is_empty())
        {
            writeln!(f, "Possible values:")?;
            writeln!(f)?;
            for value in possible_values {
                writeln!(f, "- {value}")?;
            }
        } else {
            writeln!(f, "Type: {}", self.value_type)?;
        }

        if let Some(deprecated) = &self.deprecated {
            write!(f, "Deprecated")?;

            if let Some(since) = deprecated.since {
                write!(f, " (since {since})")?;
            }

            if let Some(message) = deprecated.message {
                write!(f, ": {message}")?;
            }

            writeln!(f)?;
        }

        writeln!(f, "Example usage:\n```toml\n{}\n```", self.example)
    }
}

/// A possible value for an enum, similar to Clap's `PossibleValue` type (but without a dependency
/// on Clap).
#[derive(Debug, Eq, PartialEq, Clone, Serialize)]
pub struct PossibleValue {
    pub name: String,
    pub help: Option<String>,
}

impl Display for PossibleValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "`\"{}\"`", self.name)?;
        if let Some(help) = &self.help {
            write!(f, ": {help}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_child_option() {
        struct WithOptions;

        impl OptionsMetadata for WithOptions {
            fn record(visit: &mut dyn Visit) {
                visit.record_field(
                    "ignore-git-ignore",
                    OptionField {
                        doc: "Whether Ruff should respect the gitignore file",
                        default: "false",
                        value_type: "bool",
                        example: "",
                        scope: None,
                        deprecated: None,
                        possible_values: None,
                    },
                );
            }
        }

        assert!(WithOptions::metadata().has("ignore-git-ignore"));
        assert!(!WithOptions::metadata().has("does-not-exist"));
    }

    #[test]
    fn test_has_nested_option() {
        struct Root;

        impl OptionsMetadata for Root {
            fn record(visit: &mut dyn Visit) {
                visit.record_field(
                    "ignore-git-ignore",
                    OptionField {
                        doc: "Whether Ruff should respect the gitignore file",
                        default: "false",
                        value_type: "bool",
                        example: "",
                        scope: None,
                        deprecated: None,
                        possible_values: None,
                    },
                );

                visit.record_set("format", Nested::metadata());
            }
        }

        struct Nested;

        impl OptionsMetadata for Nested {
            fn record(visit: &mut dyn Visit) {
                visit.record_field(
                    "hard-tabs",
                    OptionField {
                        doc: "Use hard tabs for indentation and spaces for alignment.",
                        default: "false",
                        value_type: "bool",
                        example: "",
                        scope: None,
                        deprecated: None,
                        possible_values: None,
                    },
                );
            }
        }

        assert!(Root::metadata().has("format.hard-tabs"));
        assert!(!Root::metadata().has("format.spaces"));
        assert!(!Root::metadata().has("lint.hard-tabs"));
    }

    #[test]
    fn test_find_child_option() {
        struct WithOptions;

        static IGNORE_GIT_IGNORE: OptionField = OptionField {
            doc: "Whether Ruff should respect the gitignore file",
            default: "false",
            value_type: "bool",
            example: "",
            scope: None,
            deprecated: None,
            possible_values: None,
        };

        impl OptionsMetadata for WithOptions {
            fn record(visit: &mut dyn Visit) {
                visit.record_field("ignore-git-ignore", IGNORE_GIT_IGNORE.clone());
            }
        }

        assert_eq!(
            WithOptions::metadata().find("ignore-git-ignore"),
            Some(OptionEntry::Field(IGNORE_GIT_IGNORE.clone()))
        );
        assert_eq!(WithOptions::metadata().find("does-not-exist"), None);
    }

    #[test]
    fn test_find_nested_option() {
        static HARD_TABS: OptionField = OptionField {
            doc: "Use hard tabs for indentation and spaces for alignment.",
            default: "false",
            value_type: "bool",
            example: "",
            scope: None,
            deprecated: None,
            possible_values: None,
        };

        struct Root;

        impl OptionsMetadata for Root {
            fn record(visit: &mut dyn Visit) {
                visit.record_field(
                    "ignore-git-ignore",
                    OptionField {
                        doc: "Whether Ruff should respect the gitignore file",
                        default: "false",
                        value_type: "bool",
                        example: "",
                        scope: None,
                        deprecated: None,
                        possible_values: None,
                    },
                );

                visit.record_set("format", Nested::metadata());
            }
        }

        struct Nested;

        impl OptionsMetadata for Nested {
            fn record(visit: &mut dyn Visit) {
                visit.record_field("hard-tabs", HARD_TABS.clone());
            }
        }

        assert_eq!(
            Root::metadata().find("format.hard-tabs"),
            Some(OptionEntry::Field(HARD_TABS.clone()))
        );
        assert_eq!(
            Root::metadata().find("format"),
            Some(OptionEntry::Set(Nested::metadata()))
        );
        assert_eq!(Root::metadata().find("format.spaces"), None);
        assert_eq!(Root::metadata().find("lint.hard-tabs"), None);
    }
}
