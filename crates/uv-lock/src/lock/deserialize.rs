//! Deserializes the canonical lockfile layout without building a TOML tree.
//!
//! This parser is deliberately limited to the syntax emitted by the lockfile
//! serializer. The public entry point falls back to the general TOML parser
//! when a lock uses another valid representation.

use std::borrow::Cow;
use std::fmt;

use memchr::memchr3;
use rustc_hash::{FxBuildHasher, FxHashSet};
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::forward_to_deserialize_any;
use smallvec::SmallVec;
use toml_parser::decoder::{Encoding, IntegerRadix, ScalarKind};
use toml_parser::{Raw, Span};

use super::Lock;

/// Parses a canonical lock directly into its existing, validated wire format.
pub(super) fn from_str(input: &str) -> Result<Lock, Error> {
    let mut cursor = Cursor::new(input);
    let lock = Lock::deserialize(DocumentDeserializer {
        cursor: &mut cursor,
    })?;
    cursor.skip_whitespace();
    if cursor.peek().is_some() {
        return Err(cursor.unsupported("unexpected trailing input"));
    }
    Ok(lock)
}

/// An error returned when parsing a lockfile without the general TOML fallback.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported canonical lock syntax at byte {offset}: {message}")]
    Unsupported {
        offset: usize,
        message: &'static str,
    },
    #[error("invalid canonical lock syntax at byte {offset}: {message}")]
    Invalid { offset: usize, message: String },
    #[error("failed to deserialize canonical lock: {0}")]
    Deserialize(String),
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Deserialize(message.to_string())
    }
}

struct Cursor<'de> {
    input: &'de str,
    offset: usize,
    container_depth: u8,
}

impl<'de> Cursor<'de> {
    fn new(input: &'de str) -> Self {
        Self {
            input,
            offset: 0,
            container_depth: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }

    fn unsupported(&self, message: &'static str) -> Error {
        Error::Unsupported {
            offset: self.offset,
            message,
        }
    }

    fn invalid(&self, error: &toml_parser::ParseError) -> Error {
        Error::Invalid {
            offset: error
                .unexpected()
                .or_else(|| error.context())
                .map_or(self.offset, |span| span.start()),
            message: error.description().to_owned(),
        }
    }

    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n') => self.offset += 1,
                Some(b'\r') if self.input.as_bytes().get(self.offset + 1) == Some(&b'\n') => {
                    self.offset += 2;
                }
                _ => break,
            }
        }
    }

    fn skip_horizontal_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.offset += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> Result<(), Error> {
        if self.peek() == Some(expected) {
            self.offset += 1;
            Ok(())
        } else {
            Err(self.unsupported("unexpected delimiter"))
        }
    }

    fn header(&self) -> Result<&'de str, Error> {
        if self.peek() != Some(b'[') {
            return Err(self.unsupported("expected a table header"));
        }
        let remaining = &self.input[self.offset..];
        let length = remaining.find('\n').unwrap_or(remaining.len());
        Ok(remaining[..length].trim_end_matches('\r'))
    }

    fn consume_header(&mut self, expected: &'static str) -> Result<(), Error> {
        if self.header()? != expected {
            return Err(self.unsupported("unknown or noncanonical table header"));
        }
        self.offset += expected.len();

        match self.peek() {
            Some(b'\r') => {
                self.offset += 1;
                self.consume(b'\n')
            }
            Some(b'\n') => {
                self.offset += 1;
                Ok(())
            }
            None => Ok(()),
            _ => Err(self.unsupported("expected the end of a table header")),
        }
    }

    fn assignment_key(&mut self) -> Result<&'de str, Error> {
        let start = self.offset;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-')
        ) {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(self.unsupported("expected a canonical bare key"));
        }
        let key = &self.input[start..self.offset];
        self.skip_horizontal_whitespace();
        self.consume(b'=')?;
        self.skip_horizontal_whitespace();
        Ok(key)
    }

    fn finish_assignment(&mut self) -> Result<(), Error> {
        self.skip_horizontal_whitespace();

        if self.peek() == Some(b'#') {
            self.offset += 1;

            while let Some(byte) = self.peek() {
                match byte {
                    b'\n' | b'\r' => break,
                    b'\t' | b' '..=b'~' | 0x80..=0xff => self.offset += 1,
                    _ => return Err(self.unsupported("invalid character in a TOML comment")),
                }
            }
        }

        match self.peek() {
            Some(b'\r') => {
                self.offset += 1;
                self.consume(b'\n')
            }
            Some(b'\n') => {
                self.offset += 1;
                Ok(())
            }
            None => Ok(()),
            _ => Err(self.unsupported("expected the end of an assignment")),
        }
    }

    fn string(&mut self) -> Result<Cow<'de, str>, Error> {
        let start = self.offset;
        self.consume(b'"')?;
        if self.input[self.offset..].starts_with("\"\"") {
            return Err(self.unsupported("multiline strings require the TOML fallback"));
        }

        let mut escaped = false;
        loop {
            let remaining = &self.input.as_bytes()[self.offset..];
            let Some(delimiter) = memchr3(b'"', b'\\', 0x7f, remaining) else {
                return Err(self.unsupported("unterminated basic string"));
            };

            let content = &remaining[..delimiter];
            // The minimum is SIMD-vectorizable; locate the exact offset only for invalid input.
            if content
                .iter()
                .copied()
                .min()
                .is_some_and(|byte| byte < 0x20)
                && let Some(control) = content.iter().position(|byte| *byte < 0x20)
            {
                self.offset += control;
                return Err(self.unsupported("control character in a basic string"));
            }

            self.offset += delimiter;
            match self.peek() {
                Some(b'"') => {
                    let end = self.offset;
                    self.offset += 1;
                    if escaped {
                        let encoded = &self.input[start..self.offset];
                        let raw = Raw::new_unchecked(
                            encoded,
                            Some(Encoding::BasicString),
                            Span::new_unchecked(start, self.offset),
                        );
                        let mut decoded = Cow::Borrowed("");
                        let mut error = None;
                        let kind = raw.decode_scalar(&mut decoded, &mut error);
                        debug_assert_eq!(kind, ScalarKind::String);

                        if let Some(error) = error {
                            return Err(self.invalid(&error));
                        }

                        return Ok(decoded);
                    }
                    return Ok(Cow::Borrowed(&self.input[start + 1..end]));
                }
                Some(b'\\') => {
                    escaped = true;
                    self.offset += 1;
                    if self.peek().is_none() {
                        return Err(self.unsupported("unterminated string escape"));
                    }
                    self.offset += 1;
                }
                _ => return Err(self.unsupported("control character in a basic string")),
            }
        }
    }

    fn number(&mut self) -> Result<Cow<'de, str>, Error> {
        let start = self.offset;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' | b'_')
        ) {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(self.unsupported("expected a canonical number"));
        }

        let encoded = &self.input[start..self.offset];
        let raw = Raw::new_unchecked(encoded, None, Span::new_unchecked(start, self.offset));
        let mut decoded = Cow::Borrowed("");
        let mut error = None;
        let kind = raw.decode_scalar(&mut decoded, &mut error);

        if let Some(error) = error {
            return Err(self.invalid(&error));
        }

        if !matches!(
            kind,
            ScalarKind::Float | ScalarKind::Integer(IntegerRadix::Dec)
        ) {
            return Err(self.unsupported("expected a canonical number"));
        }

        Ok(decoded)
    }

    fn literal(&mut self, literal: &'static str) -> Result<(), Error> {
        if self.input[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            Err(self.unsupported("expected a canonical boolean"))
        }
    }

    fn with_container<T>(
        &mut self,
        deserialize: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.container_depth == 80 {
            return Err(self.unsupported("maximum TOML nesting depth exceeded"));
        }

        self.container_depth += 1;
        let result = deserialize(self);
        self.container_depth -= 1;
        result
    }
}

struct DocumentDeserializer<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
}

impl<'de> de::Deserializer<'de> for DocumentDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_map(DocumentMapAccess {
            cursor: self.cursor,
            kind: MapKind::Root,
            pending: None,
            seen_keys: SeenKeys::default(),
        })
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
        struct enum identifier ignored_any
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MapKind {
    Root,
    Options,
    OptionsExcludeNewerPackage,
    Manifest,
    ManifestDependencyGroups,
    ManifestDependencyMetadata,
    Package,
    PackageOptionalDependencies,
    PackageDevDependencies,
    PackageMetadata,
    PackageMetadataRequiresDev,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SequenceKind {
    Packages,
    ManifestDependencyMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pending {
    Value,
    Map(MapKind),
    Sequence(SequenceKind),
}

/// Keeps common maps allocation-free and wide maps linear in the number of keys.
#[derive(Default)]
struct SeenKeys<'de> {
    inline: SmallVec<[&'de str; 8]>,
    overflow: Option<FxHashSet<&'de str>>,
}

impl<'de> SeenKeys<'de> {
    fn insert(&mut self, key: &'de str) -> bool {
        if let Some(overflow) = &mut self.overflow {
            return overflow.insert(key);
        }

        if self.inline.contains(&key) {
            return false;
        }

        if self.inline.len() < self.inline.inline_size() {
            self.inline.push(key);
            return true;
        }

        let mut overflow =
            FxHashSet::with_capacity_and_hasher(self.inline.len() + 1, FxBuildHasher);
        overflow.extend(self.inline.iter().copied());
        let inserted = overflow.insert(key);
        self.overflow = Some(overflow);
        inserted
    }
}

struct DocumentMapAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    kind: MapKind,
    pending: Option<Pending>,
    seen_keys: SeenKeys<'de>,
}

impl<'de> MapAccess<'de> for DocumentMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        self.cursor.skip_whitespace();
        match self.cursor.peek() {
            None => Ok(None),
            Some(b'[') => self.section_key(seed),
            Some(b'#') => Err(self
                .cursor
                .unsupported("comments require the TOML fallback")),
            Some(_) => {
                let key = self.cursor.assignment_key()?;
                self.track_key(key)?;
                self.pending = Some(Pending::Value);
                seed.deserialize(de::value::BorrowedStrDeserializer::new(key))
                    .map(Some)
            }
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let Some(pending) = self.pending.take() else {
            return Err(self.cursor.unsupported("map value has no matching key"));
        };

        match pending {
            Pending::Value => {
                let value = seed.deserialize(ValueDeserializer {
                    cursor: self.cursor,
                })?;
                self.cursor.finish_assignment()?;
                Ok(value)
            }
            Pending::Map(kind) => seed.deserialize(SectionDeserializer {
                cursor: self.cursor,
                kind,
            }),
            Pending::Sequence(kind) => seed.deserialize(SectionSequenceDeserializer {
                cursor: self.cursor,
                kind,
            }),
        }
    }
}

impl<'de> DocumentMapAccess<'_, 'de> {
    fn track_key(&mut self, key: &'de str) -> Result<(), Error> {
        if !self.seen_keys.insert(key) {
            return Err(self.cursor.unsupported("duplicate TOML key"));
        }
        Ok(())
    }

    fn section_key<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        let header = self.cursor.header()?;
        let child = match (self.kind, header) {
            (MapKind::Root, "[options]") => {
                Some(("options", Pending::Map(MapKind::Options), "[options]"))
            }
            (MapKind::Root, "[manifest]") => {
                Some(("manifest", Pending::Map(MapKind::Manifest), "[manifest]"))
            }
            (MapKind::Root, "[[package]]") => Some((
                "package",
                Pending::Sequence(SequenceKind::Packages),
                "[[package]]",
            )),
            (MapKind::Options, "[options.exclude-newer-package]") => Some((
                "exclude-newer-package",
                Pending::Map(MapKind::OptionsExcludeNewerPackage),
                "[options.exclude-newer-package]",
            )),
            (MapKind::Manifest, "[manifest.dependency-groups]") => Some((
                "dependency-groups",
                Pending::Map(MapKind::ManifestDependencyGroups),
                "[manifest.dependency-groups]",
            )),
            (MapKind::Manifest, "[[manifest.dependency-metadata]]") => Some((
                "dependency-metadata",
                Pending::Sequence(SequenceKind::ManifestDependencyMetadata),
                "[[manifest.dependency-metadata]]",
            )),
            (MapKind::Package, "[package.optional-dependencies]") => Some((
                "optional-dependencies",
                Pending::Map(MapKind::PackageOptionalDependencies),
                "[package.optional-dependencies]",
            )),
            (MapKind::Package, "[package.dev-dependencies]") => Some((
                "dev-dependencies",
                Pending::Map(MapKind::PackageDevDependencies),
                "[package.dev-dependencies]",
            )),
            (MapKind::Package, "[package.metadata]") => Some((
                "metadata",
                Pending::Map(MapKind::PackageMetadata),
                "[package.metadata]",
            )),
            (MapKind::PackageMetadata, "[package.metadata.requires-dev]") => Some((
                "requires-dev",
                Pending::Map(MapKind::PackageMetadataRequiresDev),
                "[package.metadata.requires-dev]",
            )),
            (MapKind::Root, _) => {
                return Err(self
                    .cursor
                    .unsupported("unknown or noncanonical lock table"));
            }
            _ => None,
        };

        let Some((key, pending, expected)) = child else {
            return Ok(None);
        };
        self.track_key(key)?;
        self.cursor.consume_header(expected)?;
        self.pending = Some(pending);
        seed.deserialize(de::value::BorrowedStrDeserializer::new(key))
            .map(Some)
    }
}

struct SectionDeserializer<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    kind: MapKind,
}

impl<'de> de::Deserializer<'de> for SectionDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_map(DocumentMapAccess {
            cursor: self.cursor,
            kind: self.kind,
            pending: None,
            seen_keys: SeenKeys::default(),
        })
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
        struct enum identifier ignored_any
    }
}

struct SectionSequenceDeserializer<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    kind: SequenceKind,
}

impl<'de> de::Deserializer<'de> for SectionSequenceDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_seq(SectionSequenceAccess {
            cursor: self.cursor,
            kind: self.kind,
            started: false,
        })
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
        struct enum identifier ignored_any
    }
}

struct SectionSequenceAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    kind: SequenceKind,
    started: bool,
}

impl<'de> SeqAccess<'de> for SectionSequenceAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if self.started {
            self.cursor.skip_whitespace();
            if self.cursor.peek() != Some(b'[') {
                return Ok(None);
            }

            let expected = match self.kind {
                SequenceKind::Packages => "[[package]]",
                SequenceKind::ManifestDependencyMetadata => "[[manifest.dependency-metadata]]",
            };
            if self.cursor.header()? != expected {
                return Ok(None);
            }
            self.cursor.consume_header(expected)?;
        }

        self.started = true;
        let kind = match self.kind {
            SequenceKind::Packages => MapKind::Package,
            SequenceKind::ManifestDependencyMetadata => MapKind::ManifestDependencyMetadata,
        };
        seed.deserialize(SectionDeserializer {
            cursor: self.cursor,
            kind,
        })
        .map(Some)
    }
}

struct ValueDeserializer<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.cursor.peek() {
            Some(b'"') => match self.cursor.string()? {
                Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
                Cow::Owned(value) => visitor.visit_string(value),
            },
            Some(b'{') => self.cursor.with_container(|cursor| {
                cursor.consume(b'{')?;
                visitor.visit_map(InlineMapAccess {
                    cursor,
                    started: false,
                    seen_keys: SeenKeys::default(),
                })
            }),
            Some(b'[') => self.cursor.with_container(|cursor| {
                cursor.consume(b'[')?;
                visitor.visit_seq(InlineSequenceAccess {
                    cursor,
                    started: false,
                })
            }),
            Some(b't') => {
                self.cursor.literal("true")?;
                visitor.visit_bool(true)
            }
            Some(b'f') => {
                self.cursor.literal("false")?;
                visitor.visit_bool(false)
            }
            Some(b'-' | b'0'..=b'9') => {
                let number = self.cursor.number()?;
                if number.contains(['.', 'e', 'E']) {
                    let value = number
                        .parse::<f64>()
                        .map_err(|_| self.cursor.unsupported("invalid floating-point value"))?;
                    visitor.visit_f64(value)
                } else if number.starts_with('-') {
                    let value = number
                        .parse::<i64>()
                        .map_err(|_| self.cursor.unsupported("invalid signed integer"))?;
                    visitor.visit_i64(value)
                } else {
                    let value = number
                        .parse::<u64>()
                        .map_err(|_| self.cursor.unsupported("invalid unsigned integer"))?;
                    visitor.visit_u64(value)
                }
            }
            _ => Err(self.cursor.unsupported("unsupported canonical lock value")),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.cursor.peek() {
            Some(b'"') => match self.cursor.string()? {
                Cow::Borrowed(value) => {
                    visitor.visit_enum(de::value::BorrowedStrDeserializer::new(value))
                }
                Cow::Owned(value) => visitor.visit_enum(de::value::StringDeserializer::new(value)),
            },
            Some(b'{') => self.cursor.with_container(|cursor| {
                cursor.consume(b'{')?;
                visitor.visit_enum(InlineEnumAccess { cursor })
            }),
            _ => Err(self.cursor.unsupported("expected a canonical enum value")),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

struct InlineMapAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    started: bool,
    seen_keys: SeenKeys<'de>,
}

impl<'de> MapAccess<'de> for InlineMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        self.cursor.skip_whitespace();
        if self.started {
            if self.cursor.peek() == Some(b'}') {
                self.cursor.consume(b'}')?;
                return Ok(None);
            }
            self.cursor.consume(b',')?;
            self.cursor.skip_whitespace();
        } else if self.cursor.peek() == Some(b'}') {
            self.cursor.consume(b'}')?;
            return Ok(None);
        }

        let key = self.cursor.assignment_key()?;
        if !self.seen_keys.insert(key) {
            return Err(self.cursor.unsupported("duplicate TOML key"));
        }
        self.started = true;
        seed.deserialize(de::value::BorrowedStrDeserializer::new(key))
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(ValueDeserializer {
            cursor: self.cursor,
        })
    }
}

struct InlineSequenceAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
    started: bool,
}

impl<'de> SeqAccess<'de> for InlineSequenceAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.cursor.skip_whitespace();
        if self.started {
            if self.cursor.peek() == Some(b']') {
                self.cursor.consume(b']')?;
                return Ok(None);
            }
            self.cursor.consume(b',')?;
            self.cursor.skip_whitespace();
        }

        if self.cursor.peek() == Some(b']') {
            self.cursor.consume(b']')?;
            return Ok(None);
        }

        self.started = true;
        seed.deserialize(ValueDeserializer {
            cursor: self.cursor,
        })
        .map(Some)
    }
}

struct InlineEnumAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
}

impl<'a, 'de> EnumAccess<'de> for InlineEnumAccess<'a, 'de> {
    type Error = Error;
    type Variant = InlineVariantAccess<'a, 'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        self.cursor.skip_whitespace();
        let key = self.cursor.assignment_key()?;
        let variant = seed.deserialize(de::value::BorrowedStrDeserializer::new(key))?;
        Ok((
            variant,
            InlineVariantAccess {
                cursor: self.cursor,
            },
        ))
    }
}

struct InlineVariantAccess<'a, 'de> {
    cursor: &'a mut Cursor<'de>,
}

impl<'de> VariantAccess<'de> for InlineVariantAccess<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Err(self
            .cursor
            .unsupported("inline tables cannot contain unit variants"))
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        let value = seed.deserialize(ValueDeserializer {
            cursor: self.cursor,
        })?;
        self.cursor.skip_whitespace();
        self.cursor.consume(b'}')?;
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, length: usize, visitor: V) -> Result<V::Value, Error> {
        let value = de::Deserializer::deserialize_tuple(
            ValueDeserializer {
                cursor: self.cursor,
            },
            length,
            visitor,
        )?;
        self.cursor.skip_whitespace();
        self.cursor.consume(b'}')?;
        Ok(value)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let value = de::Deserializer::deserialize_struct(
            ValueDeserializer {
                cursor: self.cursor,
            },
            "",
            fields,
            visitor,
        )?;
        self.cursor.skip_whitespace();
        self.cursor.consume(b'}')?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::fmt::Write as _;

    use serde::Deserialize;

    use super::super::{LockParseError, VERSION};
    use super::{Cursor, Error, Lock, ValueDeserializer, from_str};

    const CANONICAL_LOCK: &str = r#"version = 1
revision = 3
requires-python = ">=3.12"

[[package]]
name = "dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }

[[package]]
name = "project"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
    { name = "dependency" },
]
"#;

    #[test]
    fn canonical_lock_matches_toml() {
        let expected: Lock = toml::from_str(CANONICAL_LOCK).expect("valid TOML lock");
        let actual = from_str(CANONICAL_LOCK).expect("valid canonical lock");

        assert_eq!(actual, expected);
    }

    #[test]
    fn repository_lock_matches_toml() {
        let input = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../uv.lock"));
        let expected: Lock = toml::from_str(input).expect("valid repository lock");
        let actual = from_str(input).expect("valid canonical repository lock");

        assert_eq!(actual, expected);
    }

    #[test]
    fn nested_lock_sections_match_toml() {
        let input = r#"version = 1
revision = 3
requires-python = ">=3.12"

[options]
resolution-mode = "highest"

[options.exclude-newer-package]
dependency = false

[manifest]
members = ["project"]
requirements = [{ name = "dependency", specifier = ">=1" }]

[manifest.dependency-groups]
dev = [{ name = "dependency", specifier = ">=1" }]

[[manifest.dependency-metadata]]
name = "dependency"
version = "1.0.0"

[[package]]
name = "dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }

[[package]]
name = "project"
version = "0.1.0"
source = { virtual = "." }
dependencies = [{ name = "dependency" }]

[package.optional-dependencies]
feature = [{ name = "dependency" }]

[package.dev-dependencies]
dev = [{ name = "dependency" }]

[package.metadata]
requires-dist = [{ name = "dependency", specifier = ">=1" }]
provides-extras = ["feature"]

[package.metadata.requires-dev]
dev = [{ name = "dependency", specifier = ">=1" }]
"#;
        let expected: Lock = toml::from_str(input).expect("valid nested TOML lock");
        let actual = from_str(input).expect("valid nested canonical lock");

        assert_eq!(actual, expected);
    }

    #[test]
    fn escaped_strings_match_toml() {
        let input = CANONICAL_LOCK.replace(
            "https://example.com/simple",
            "https://example.com/simple?name=quoted\\\"value",
        );
        let expected: Lock = toml::from_str(&input).expect("valid TOML lock");
        let actual = from_str(&input).expect("valid canonical lock");

        assert_eq!(actual, expected);
    }

    #[test]
    fn long_basic_strings_deserialize_directly() {
        let source = format!("https://example.com/{}", "a".repeat(512));
        let input = CANONICAL_LOCK.replace("https://example.com/simple", &source);
        let expected: Lock = toml::from_str(&input).expect("valid TOML lock with a long string");
        let actual = from_str(&input).expect("long basic string uses the direct parser");

        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_basic_string_deserializes_directly() {
        let mut cursor = Cursor::new(r#""""#);
        let actual = String::deserialize(ValueDeserializer {
            cursor: &mut cursor,
        })
        .expect("empty basic string deserializes directly");

        assert_eq!(actual, "");
    }

    #[test]
    fn noncanonical_lock_falls_back() {
        let input = CANONICAL_LOCK
            .replace("requires-python = \">=3.12\"", "requires-python = '>=3.12'")
            .replace("[[package]]", "# A hand-edited package.\n[[package]]");
        let expected: Lock = toml::from_str(&input).expect("valid noncanonical lock");

        assert!(
            Lock::from_canonical_toml(&input).is_err(),
            "the canonical lock parser must not fall back"
        );
        assert_eq!(
            Lock::from_toml(&input).expect("noncanonical lock falls back"),
            expected
        );
    }

    #[test]
    fn invalid_lock_preserves_toml_error() {
        let input = CANONICAL_LOCK.replace(
            "source = { registry = \"https://example.com/simple\" }",
            "source = { registry = \"https://example.com/simple\", registry = \"https://other.example/simple\" }",
        );
        let expected = toml::from_str::<Lock>(&input).expect_err("duplicate source is invalid");
        let actual = Lock::from_toml(&input).expect_err("duplicate source remains invalid");

        assert_eq!(actual.to_string(), expected.to_string());
    }

    #[test]
    fn unsupported_lock_version_is_rejected() {
        let version = VERSION + 1;
        let input = CANONICAL_LOCK.replacen("version = 1", &format!("version = {version}"), 1);
        let error = Lock::from_toml(&input).expect_err("unsupported lock versions are rejected");

        assert_matches!(
            error,
            LockParseError::UnsupportedVersion {
                supported: VERSION,
                version: actual,
            } if actual == version
        );
    }

    #[test]
    fn unparsable_unsupported_lock_version_is_identified() {
        let version = VERSION + 1;
        let input = CANONICAL_LOCK
            .replacen("version = 1", &format!("version = {version}"), 1)
            .replacen("name = \"dependency\"", "name = false", 1);
        let error =
            Lock::from_toml(&input).expect_err("unparsable unsupported lock versions are rejected");

        assert_matches!(
            error,
            LockParseError::UnparsableVersion {
                supported: VERSION,
                version: actual,
                ..
            } if actual == version
        );
    }

    #[test]
    fn syntax_errors_preserve_real_byte_offsets() {
        let input =
            CANONICAL_LOCK.replace("requires-python = \">=3.12\"", "requires-python = '>=3.12'");
        let unsupported = from_str(&input).expect_err("literal strings require the TOML fallback");

        assert_matches!(&unsupported, Error::Unsupported { offset, .. } if *offset > 0,
            "unsupported syntax must retain its real byte offset: {unsupported}"
        );

        let input = CANONICAL_LOCK.replace("revision = 3", "revision = 03");
        let invalid = from_str(&input).expect_err("TOML rejects a leading zero");

        assert_matches!(&invalid, Error::Invalid { offset, .. } if *offset > 0,
            "invalid TOML scalars must retain their real byte offset: {invalid}"
        );
    }

    #[test]
    fn inline_unit_variants_without_values_preserve_toml_error() {
        for variant in ["highest", "lowest", "lowest-direct"] {
            let input = CANONICAL_LOCK.replacen(
                "\n\n[[package]]",
                &format!("\n\n[options]\nresolution-mode = {{ {variant} = }}\n\n[[package]]"),
                1,
            );
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects an inline unit variant without a value");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject an inline `{variant}` without a value"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects an inline unit variant without a value");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn leading_zero_integers_preserve_toml_error() {
        for (valid, invalid) in [
            ("version = 1", "version = 01"),
            ("revision = 3", "revision = 03"),
        ] {
            let input = CANONICAL_LOCK.replace(valid, invalid);
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects decimal integers with leading zeros");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject `{invalid}`"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects decimal integers with leading zeros");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn invalid_numeric_syntax_preserves_toml_error() {
        for number in [
            "-01", "00.1", "1.", "1.e2", "1e", "1e+", "1__0", "1_", "0_1",
        ] {
            let input = CANONICAL_LOCK.replace(
                "requires-python = \">=3.12\"",
                &format!("requires-python = \">=3.12\"\nunknown-number = {number}"),
            );
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects invalid decimal number syntax");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject `{number}`"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects invalid decimal number syntax");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn json_only_string_escapes_preserve_toml_error() {
        for source in [
            r"https:\/\/example.com\/simple",
            r"https://example.com/\uD83D\uDE00",
        ] {
            let input = CANONICAL_LOCK.replace("https://example.com/simple", source);
            let expected =
                toml::from_str::<Lock>(&input).expect_err("TOML rejects JSON-only string escapes");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject JSON-only escapes in `{source}`"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects JSON-only string escapes");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn unicode_escapes_match_toml() {
        let input = CANONICAL_LOCK.replace(
            "https://example.com/simple",
            r"https://example.com/\u0073imple",
        );
        let expected: Lock = toml::from_str(&input).expect("valid TOML Unicode escape");
        let actual = from_str(&input).expect("valid canonical Unicode escape");

        assert_eq!(actual, expected);
    }

    #[test]
    fn toml_only_string_escapes_deserialize_directly() {
        for source in [
            r"https://example.com/simpl\x65",
            r"https://example.com/simpl\U00000065",
        ] {
            let input = CANONICAL_LOCK.replace("https://example.com/simple", source);
            let expected: Lock = toml::from_str(&input).expect("valid TOML-only string escape");

            assert_eq!(
                from_str(&input).expect("valid TOML-only string escape uses the direct parser"),
                expected,
                "the TOML decoder correctly handles escapes in `{source}`"
            );
        }
    }

    #[test]
    fn underscored_integers_deserialize_directly() {
        let input = CANONICAL_LOCK.replace("revision = 3", "revision = 3_0");
        let expected: Lock = toml::from_str(&input).expect("valid TOML integer separator");
        let actual = from_str(&input).expect("valid TOML integer separator uses the direct parser");

        assert_eq!(actual, expected);
    }

    #[test]
    fn toml_escape_character_deserializes_directly() {
        let input = CANONICAL_LOCK.replace(
            "revision = 3\n",
            "revision = 3\nignored = \"TOML\\eescape\"\n",
        );
        let expected: Lock = toml::from_str(&input).expect("valid TOML escape character");
        let actual = from_str(&input).expect("valid TOML escape character uses the direct parser");

        assert_eq!(actual, expected);
    }

    #[test]
    fn canonical_relative_exclude_newer_uses_fast_path() {
        let input = CANONICAL_LOCK.replace(
            "requires-python = \">=3.12\"\n",
            concat!(
                "requires-python = \">=3.12\"\n\n",
                "[options]\n",
                "exclude-newer = \"0001-01-01T00:00:00Z\" ",
                "# This has no effect and is included for backwards compatibility ",
                "when using relative exclude-newer values.\n",
                "exclude-newer-span = \"P3W\"\n",
            ),
        );
        let expected: Lock =
            toml::from_str(&input).expect("canonical relative exclude-newer lock is valid TOML");
        let actual =
            from_str(&input).expect("canonical relative exclude-newer lock uses the direct parser");

        assert_eq!(actual, expected);
    }

    #[test]
    fn inline_comments_match_toml() {
        let input = CANONICAL_LOCK
            .replace("version = 1\n", "version = 1 # root comment\n")
            .replace(
                "source = { virtual = \".\" }\n",
                "source = { virtual = \".\" } # package comment\n",
            );
        let expected: Lock = toml::from_str(&input).expect("inline comments are valid TOML");
        let actual = from_str(&input).expect("inline comments use the direct parser");

        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_comment_characters_preserve_toml_error() {
        for character in ['\u{0}', '\u{7}', '\u{b}', '\u{7f}'] {
            let input = CANONICAL_LOCK.replace(
                "version = 1\n",
                &format!("version = 1 # invalid{character}comment\n"),
            );
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects control characters in comments");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject U+{:04X} in a comment",
                u32::from(character)
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects control characters in comments");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn bare_carriage_returns_preserve_toml_error() {
        for input in [
            format!("\r{CANONICAL_LOCK}"),
            format!("{CANONICAL_LOCK}\r"),
            CANONICAL_LOCK.replace("revision = 3\n", "revision = 3\n\r"),
            CANONICAL_LOCK.replace("dependencies = [\n", "dependencies = [\r"),
            format!("{CANONICAL_LOCK}[options]\r"),
        ] {
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects standalone carriage returns");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject standalone carriage returns"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects standalone carriage returns");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn unescaped_delete_preserves_toml_error() {
        let input = CANONICAL_LOCK.replace(
            "requires-python = \">=3.12\"\n",
            "requires-python = \">=3.12\"\nunknown = \"invalid\u{7f}string\"\n",
        );
        let expected =
            toml::from_str::<Lock>(&input).expect_err("TOML rejects unescaped ASCII DELETE");

        assert!(
            from_str(&input).is_err(),
            "the direct parser must reject unescaped ASCII DELETE"
        );

        let actual =
            Lock::from_toml(&input).expect_err("the lock reader rejects unescaped ASCII DELETE");

        assert_eq!(actual.to_string(), expected.to_string());
    }

    #[test]
    fn unescaped_string_control_characters_preserve_toml_error() {
        for prefix_length in [0, 7, 8, 15, 16, 31, 32, 63, 64] {
            let prefix = "a".repeat(prefix_length);

            for character in ['\u{0}', '\u{7}', '\u{8}', '\n', '\r', '\u{1f}'] {
                let input = CANONICAL_LOCK.replace(
                    "requires-python = \">=3.12\"\n",
                    &format!(
                        "requires-python = \">=3.12\"\nunknown = \"{prefix}{character}string\"\n"
                    ),
                );
                let expected = toml::from_str::<Lock>(&input)
                    .expect_err("TOML rejects unescaped control characters in basic strings");

                assert!(
                    from_str(&input).is_err(),
                    "the direct parser must reject U+{:04X} after {prefix_length} bytes",
                    u32::from(character)
                );

                let actual = Lock::from_toml(&input)
                    .expect_err("the lock reader rejects unescaped string control characters");

                assert_eq!(actual.to_string(), expected.to_string());
            }
        }
    }

    #[test]
    fn duplicate_keys_preserve_toml_error() {
        for (kind, input) in [
            (
                "unknown root keys",
                CANONICAL_LOCK.replace(
                    "requires-python = \">=3.12\"\n",
                    "requires-python = \">=3.12\"\nunknown = 1\nunknown = 2\n",
                ),
            ),
            (
                "unknown inline-table keys",
                CANONICAL_LOCK.replace(
                    "requires-python = \">=3.12\"\n",
                    "requires-python = \">=3.12\"\nunknown = { nested = 1, nested = 2 }\n",
                ),
            ),
            (
                "dependency-group keys",
                format!("{CANONICAL_LOCK}\n[package.dev-dependencies]\ndev = []\ndev = []\n"),
            ),
            (
                "section headers",
                format!(
                    "{CANONICAL_LOCK}\n[package.metadata]\nunknown = 1\n\n[package.metadata]\nunknown = 2\n"
                ),
            ),
        ] {
            let expected = toml::from_str::<Lock>(&input).expect_err("TOML rejects duplicate keys");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject duplicate {kind}"
            );

            let actual =
                Lock::from_toml(&input).expect_err("the lock reader rejects duplicate keys");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn wide_maps_match_toml() {
        let mut assignments = String::new();
        let mut inline_entries = String::new();

        for index in 0..4_096 {
            writeln!(assignments, "ignored-{index:04} = true")
                .expect("writing to a string cannot fail");

            if !inline_entries.is_empty() {
                inline_entries.push_str(", ");
            }
            write!(inline_entries, "ignored-{index:04} = true")
                .expect("writing to a string cannot fail");
        }

        for (kind, entries) in [
            ("root", assignments),
            ("inline", format!("ignored = {{ {inline_entries} }}\n")),
        ] {
            let input = CANONICAL_LOCK.replace(
                "requires-python = \">=3.12\"\n",
                &format!("requires-python = \">=3.12\"\n{entries}"),
            );
            let expected: Lock = toml::from_str(&input).expect("wide lock is valid TOML");
            let actual = from_str(&input).expect("wide lock uses the direct parser");

            assert_eq!(actual, expected, "wide {kind} map matches TOML");
        }
    }

    #[test]
    fn duplicates_after_inline_capacity_preserve_toml_error() {
        let mut assignments = String::new();
        let mut inline_entries = String::new();

        for index in 0..16 {
            writeln!(assignments, "ignored-{index:02} = {index}")
                .expect("writing to a string cannot fail");

            if !inline_entries.is_empty() {
                inline_entries.push_str(", ");
            }
            write!(inline_entries, "ignored-{index:02} = {index}")
                .expect("writing to a string cannot fail");
        }

        assignments.push_str("ignored-08 = 8\n");
        inline_entries.push_str(", ignored-08 = 8");

        for (kind, entries) in [
            ("root", assignments),
            ("inline", format!("ignored = {{ {inline_entries} }}\n")),
        ] {
            let input = CANONICAL_LOCK.replace(
                "requires-python = \">=3.12\"\n",
                &format!("requires-python = \">=3.12\"\n{entries}"),
            );
            let expected = toml::from_str::<Lock>(&input)
                .expect_err("TOML rejects duplicate keys in wide maps");

            assert!(
                from_str(&input).is_err(),
                "the direct parser rejects duplicate keys in a wide {kind} map"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects duplicate keys in wide maps");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn canonical_mutations_match_toml() {
        const MUTATIONS: &[char] = &[
            '\0', '\t', '\n', '\r', ' ', '#', '"', '\'', '\\', '=', ',', '.', '{', '}', '[', ']',
            '_', '0', 'é', '🦀', '\u{85}', '\u{2028}',
        ];

        for offset in 0..CANONICAL_LOCK.len() {
            for &mutation in MUTATIONS {
                let mut input = CANONICAL_LOCK.to_owned();
                input.insert(offset, mutation);

                let Ok(actual) = from_str(&input) else {
                    continue;
                };

                let expected = toml::from_str::<Lock>(&input);
                assert!(
                    expected.is_ok(),
                    "direct parser accepted invalid TOML after inserting {mutation:?} at {offset}: {expected:?}"
                );

                assert_eq!(
                    actual,
                    expected.expect("direct parser accepted valid TOML"),
                    "direct parser disagreed with TOML after inserting {mutation:?} at {offset}"
                );
            }
        }
    }

    #[test]
    fn excessive_container_depth_preserves_toml_error() {
        for nested in [
            format!("{}0{}", "[".repeat(81), "]".repeat(81)),
            (0..81).fold(String::from("0"), |nested, index| {
                if index % 2 == 0 {
                    format!("[{nested}]")
                } else {
                    format!("{{ nested = {nested} }}")
                }
            }),
        ] {
            let input = CANONICAL_LOCK.replace(
                "requires-python = \">=3.12\"\n",
                &format!("requires-python = \">=3.12\"\nunknown = {nested}\n"),
            );
            let expected =
                toml::from_str::<Lock>(&input).expect_err("TOML rejects excessive nesting");

            assert!(
                from_str(&input).is_err(),
                "the direct parser must reject excessive inline-container nesting"
            );

            let actual = Lock::from_toml(&input)
                .expect_err("the lock reader rejects excessive inline-container nesting");

            assert_eq!(actual.to_string(), expected.to_string());
        }
    }

    #[test]
    fn supported_container_depth_matches_toml() {
        let nested = format!("{}0{}", "[".repeat(80), "]".repeat(80));
        let input = CANONICAL_LOCK.replace(
            "requires-python = \">=3.12\"\n",
            &format!("requires-python = \">=3.12\"\nunknown = {nested}\n"),
        );
        let expected: Lock = toml::from_str(&input).expect("TOML supports 80 nested containers");
        let actual = from_str(&input).expect("the direct parser supports 80 nested containers");

        assert_eq!(actual, expected);
    }

    #[test]
    fn canonical_round_trip_uses_fast_path() {
        let lock: Lock = toml::from_str(CANONICAL_LOCK).expect("valid TOML lock");
        let canonical = lock.to_toml().expect("lock serializes canonically");

        assert_eq!(
            Lock::from_canonical_toml(&canonical).expect("writer output uses fast path"),
            lock
        );
    }
}
