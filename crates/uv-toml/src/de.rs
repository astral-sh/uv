use std::mem::MaybeUninit;

use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use toml_spanner::{Array, DateTime, Item, Key, Table, Value};

use crate::Error;

/// A serde `Deserializer` wrapping a `toml_spanner::Item`.
pub(crate) struct ItemDeserializer<'a, 'de> {
    item: &'a Item<'de>,
}

impl<'a, 'de> ItemDeserializer<'a, 'de> {
    pub(crate) fn new(item: &'a Item<'de>) -> Self {
        Self { item }
    }
}

impl<'a: 'de, 'de> de::Deserializer<'de> for ItemDeserializer<'a, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.value() {
            Value::String(&s) => visitor.visit_borrowed_str(s),
            Value::Integer(&i) => visitor.visit_i64(i),
            Value::Float(&f) => visitor.visit_f64(f),
            Value::Boolean(&b) => visitor.visit_bool(b),
            Value::Array(arr) => visitor.visit_seq(ArraySeqAccess::new(arr)),
            Value::Table(table) => visitor.visit_map(TableMapAccess::new(table)),
            Value::DateTime(dt) => visit_datetime(dt, visitor),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_bool() {
            Some(b) => visitor.visit_bool(b),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_i64() {
            Some(i) => visitor.visit_i64(i),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_f64() {
            Some(f) => visitor.visit_f64(f),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_str() {
            Some(s) => visitor.visit_borrowed_str(s),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_array() {
            Some(arr) => visitor.visit_seq(ArraySeqAccess::new(arr)),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.item.as_table() {
            Some(table) => visitor.visit_map(TableMapAccess::new(table)),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.item.value() {
            Value::String(&s) => visitor.visit_enum(s.into_deserializer()),
            Value::Table(table) => {
                let entries = table.entries();
                if entries.len() != 1 {
                    return Err(de::Error::custom(format!(
                        "expected table with exactly 1 entry for enum, found {}",
                        entries.len()
                    )));
                }
                let (key, value) = &entries[0];
                visitor.visit_enum(EnumDeserializer {
                    variant: key.name,
                    value,
                })
            }
            _ => Err(de::Error::custom(format!(
                "expected string or table for enum, found {}",
                self.item.type_str()
            ))),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_unit()
    }
}

/// Deserializer for a table used as the root of `from_str`.
pub(crate) struct TableDeserializer<'a, 'de> {
    table: &'a Table<'de>,
}

impl<'a, 'de> TableDeserializer<'a, 'de> {
    pub(crate) fn new(table: &'a Table<'de>) -> Self {
        Self { table }
    }
}

impl<'a: 'de, 'de> de::Deserializer<'de> for TableDeserializer<'a, 'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_map(TableMapAccess::new(self.table))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf unit unit_struct seq tuple tuple_struct map identifier
        ignored_any
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_map(TableMapAccess::new(self.table))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let entries = self.table.entries();
        if entries.len() != 1 {
            return Err(de::Error::custom(format!(
                "expected table with exactly 1 entry for enum, found {}",
                entries.len()
            )));
        }
        let (key, value) = &entries[0];
        visitor.visit_enum(EnumDeserializer {
            variant: key.name,
            value,
        })
    }
}

// -- SeqAccess for arrays --

struct ArraySeqAccess<'a, 'de> {
    iter: std::slice::Iter<'a, Item<'de>>,
}

impl<'a, 'de> ArraySeqAccess<'a, 'de> {
    fn new(arr: &'a Array<'de>) -> Self {
        Self {
            iter: arr.as_slice().iter(),
        }
    }
}

impl<'a: 'de, 'de> SeqAccess<'de> for ArraySeqAccess<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        match self.iter.next() {
            Some(item) => seed.deserialize(ItemDeserializer::new(item)).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.iter.len())
    }
}

// -- MapAccess for tables --

struct TableMapAccess<'a, 'de> {
    entries: &'a [(Key<'de>, Item<'de>)],
    index: usize,
}

impl<'a, 'de> TableMapAccess<'a, 'de> {
    fn new(table: &'a Table<'de>) -> Self {
        Self {
            entries: table.entries(),
            index: 0,
        }
    }
}

impl<'a: 'de, 'de> MapAccess<'de> for TableMapAccess<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        if self.index >= self.entries.len() {
            return Ok(None);
        }
        let key = &self.entries[self.index].0;
        seed.deserialize(KeyDeserializer { key: key.name })
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let value = &self.entries[self.index].1;
        self.index += 1;
        seed.deserialize(ItemDeserializer::new(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len() - self.index)
    }
}

// -- Key deserializer --

struct KeyDeserializer<'de> {
    key: &'de str,
}

impl<'de> de::Deserializer<'de> for KeyDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_borrowed_str(self.key)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

// -- Enum support --

struct EnumDeserializer<'a, 'de> {
    variant: &'de str,
    value: &'a Item<'de>,
}

impl<'a: 'de, 'de> EnumAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = Error;
    type Variant = ItemDeserializer<'a, 'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = seed.deserialize(KeyDeserializer { key: self.variant })?;
        Ok((variant, ItemDeserializer::new(self.value)))
    }
}

impl<'a: 'de, 'de> VariantAccess<'de> for ItemDeserializer<'a, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        de::Deserializer::deserialize_struct(self, "", fields, visitor)
    }
}

// -- DateTime helper --

fn visit_datetime<'de, V: Visitor<'de>>(dt: &DateTime, visitor: V) -> Result<V::Value, Error> {
    let mut buf = MaybeUninit::uninit();
    let s = dt.format(&mut buf);
    visitor.visit_str(s)
}

// -- BorrowedStrDeserializer for enum variants from strings --

struct BorrowedStrEnumAccess<'de> {
    value: &'de str,
}

impl<'de> EnumAccess<'de> for BorrowedStrEnumAccess<'de> {
    type Error = Error;
    type Variant = UnitVariantAccess;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = seed.deserialize(KeyDeserializer { key: self.value })?;
        Ok((variant, UnitVariantAccess))
    }
}

struct UnitVariantAccess;

impl<'de> VariantAccess<'de> for UnitVariantAccess {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        _seed: T,
    ) -> Result<T::Value, Self::Error> {
        Err(de::Error::custom(
            "expected unit variant, found newtype variant",
        ))
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom(
            "expected unit variant, found tuple variant",
        ))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom(
            "expected unit variant, found struct variant",
        ))
    }
}

/// Helper trait to convert `&str` into an enum access for string enum variants.
trait IntoEnumDeserializer<'de> {
    fn into_deserializer(self) -> BorrowedStrEnumAccess<'de>;
}

impl<'de> IntoEnumDeserializer<'de> for &'de str {
    fn into_deserializer(self) -> BorrowedStrEnumAccess<'de> {
        BorrowedStrEnumAccess { value: self }
    }
}
