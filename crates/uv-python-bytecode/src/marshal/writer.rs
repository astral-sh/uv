use std::collections::HashMap;

use crate::compiler::{CodeObject, Constant};

use super::graph::ObjectGraph;
use super::key::{
    ObjectKey, code_bytes_key, code_key, constant_key, constant_sort_key, constants_tuple_key,
    local_kinds_key, locals_tuple_key, slice_member_uses_identity, strings_tuple_key,
};
use super::{
    FLAG_REF, TYPE_ASCII, TYPE_ASCII_INTERNED, TYPE_BINARY_COMPLEX, TYPE_BINARY_FLOAT, TYPE_BYTES,
    TYPE_CODE, TYPE_ELLIPSIS, TYPE_FALSE, TYPE_FROZENSET, TYPE_INT, TYPE_INTERNED, TYPE_LONG,
    TYPE_NONE, TYPE_REF, TYPE_SHORT_ASCII, TYPE_SHORT_ASCII_INTERNED, TYPE_SLICE, TYPE_SMALL_TUPLE,
    TYPE_TRUE, TYPE_TUPLE, TYPE_UNICODE,
};

pub(crate) fn encode_code(code: &CodeObject) -> Vec<u8> {
    let graph = ObjectGraph::for_code(code);
    let mut writer = Writer {
        graph: &graph,
        output: Vec::new(),
        references: HashMap::default(),
    };
    writer.write_code(code, true);
    writer.output
}

#[derive(Debug)]
struct Writer<'a> {
    graph: &'a ObjectGraph,
    output: Vec<u8>,
    references: HashMap<ObjectKey, u32>,
}

impl Writer<'_> {
    fn byte(&mut self, value: u8) {
        self.output.push(value);
    }

    fn long(&mut self, value: u32) {
        self.output.extend_from_slice(&value.to_le_bytes());
    }

    fn begin_object(&mut self, key: ObjectKey, force_reference: bool) -> Option<u8> {
        if !self.graph.is_shared(&key, force_reference) {
            return Some(0);
        }
        if let Some(index) = self.references.get(&key).copied() {
            self.byte(TYPE_REF);
            self.long(index);
            return None;
        }
        let index = u32::try_from(self.references.len()).expect("marshal reference limit exceeded");
        self.references.insert(key, index);
        Some(FLAG_REF)
    }

    fn bytes(&mut self, value: &[u8], force_reference: bool) {
        self.bytes_with_key(ObjectKey::Bytes(value.to_vec()), value, force_reference);
    }

    fn bytes_with_key(&mut self, key: ObjectKey, value: &[u8], force_reference: bool) {
        let Some(flag) = self.begin_object(key, force_reference) else {
            return;
        };
        self.byte(TYPE_BYTES | flag);
        self.long(value.len().try_into().expect("marshal value exceeds 4 GiB"));
        self.output.extend_from_slice(value);
    }

    fn string(&mut self, value: &str) {
        self.string_with_force(value, false);
    }

    fn string_with_force(&mut self, value: &str, force_reference: bool) {
        let key = ObjectKey::String(value.to_string());
        let Some(flag) = self.begin_object(key, force_reference) else {
            return;
        };
        let interned = self.graph.is_interned(value);
        let bytes = value.as_bytes();
        if value.is_ascii() && bytes.len() < 256 {
            self.byte(
                if interned {
                    TYPE_SHORT_ASCII_INTERNED
                } else {
                    TYPE_SHORT_ASCII
                } | flag,
            );
            self.byte(u8::try_from(bytes.len()).expect("short ASCII string length fits in u8"));
        } else {
            self.byte(
                if value.is_ascii() {
                    if interned {
                        TYPE_ASCII_INTERNED
                    } else {
                        TYPE_ASCII
                    }
                } else if interned {
                    TYPE_INTERNED
                } else {
                    TYPE_UNICODE
                } | flag,
            );
            self.long(
                bytes
                    .len()
                    .try_into()
                    .expect("marshal string exceeds 4 GiB"),
            );
        }
        self.output.extend_from_slice(bytes);
    }

    fn surrogate_string_with_force(&mut self, value: &[u8], force_reference: bool) {
        let Some(flag) =
            self.begin_object(ObjectKey::SurrogateString(value.to_vec()), force_reference)
        else {
            return;
        };
        self.byte(TYPE_UNICODE | flag);
        self.long(
            value
                .len()
                .try_into()
                .expect("marshal string exceeds 4 GiB"),
        );
        self.output.extend_from_slice(value);
    }

    fn tuple_header(&mut self, key: ObjectKey, length: usize, force_reference: bool) -> bool {
        let Some(flag) = self.begin_object(key, force_reference) else {
            return false;
        };
        if let Ok(length) = u8::try_from(length) {
            self.byte(TYPE_SMALL_TUPLE | flag);
            self.byte(length);
        } else {
            self.byte(TYPE_TUPLE | flag);
            self.long(length.try_into().expect("marshal tuple exceeds 4 GiB"));
        }
        true
    }

    fn strings(&mut self, values: &[String], force_reference: bool) {
        let key = strings_tuple_key(values);
        if self.tuple_header(key, values.len(), force_reference) {
            for value in values {
                self.string(value);
            }
        }
    }

    fn locals(&mut self, code: &CodeObject) {
        let key = locals_tuple_key(code);
        if self.tuple_header(key, code.locals.len(), false) {
            for value in &code.locals {
                self.string(value);
            }
        }
    }

    fn constants(&mut self, values: &[Constant], force_reference: bool) {
        let key = constants_tuple_key(values);
        if self.tuple_header(key, values.len(), force_reference) {
            for value in values {
                self.constant(value);
            }
        }
    }

    fn constant(&mut self, value: &Constant) {
        self.constant_with_force(value, false);
    }

    fn constant_with_force(&mut self, value: &Constant, force_reference: bool) {
        match value {
            Constant::None => self.byte(TYPE_NONE),
            Constant::Bool(false) => self.byte(TYPE_FALSE),
            Constant::Bool(true) => self.byte(TYPE_TRUE),
            Constant::Ellipsis => self.byte(TYPE_ELLIPSIS),
            Constant::Int(value) => self.integer(*value, force_reference),
            Constant::SignedInt(value) => self.signed_integer(*value, force_reference),
            Constant::BigInt { negative, digits } => {
                self.big_integer(digits, *negative, force_reference);
            }
            Constant::Float(value) => {
                let Some(flag) =
                    self.begin_object(ObjectKey::Float(value.to_bits()), force_reference)
                else {
                    return;
                };
                self.byte(TYPE_BINARY_FLOAT | flag);
                self.output.extend_from_slice(&value.to_le_bytes());
            }
            Constant::Complex { real, imag } => {
                let Some(flag) = self.begin_object(
                    ObjectKey::Complex(real.to_bits(), imag.to_bits()),
                    force_reference,
                ) else {
                    return;
                };
                self.byte(TYPE_BINARY_COMPLEX | flag);
                self.output.extend_from_slice(&real.to_le_bytes());
                self.output.extend_from_slice(&imag.to_le_bytes());
            }
            Constant::String(value) => self.string_with_force(value, force_reference),
            Constant::SurrogateString(value) => {
                self.surrogate_string_with_force(value, force_reference);
            }
            Constant::Bytes(value) => self.bytes(value, force_reference),
            Constant::Tuple(values) => self.constants(values, force_reference),
            Constant::FrozenSet(values) => {
                let Some(flag) = self.begin_object(constant_key(value), force_reference) else {
                    return;
                };
                self.byte(TYPE_FROZENSET | flag);
                self.long(
                    values
                        .len()
                        .try_into()
                        .expect("marshal frozenset exceeds 4 GiB"),
                );
                let mut values: Vec<_> = values.iter().collect();
                values.sort_by_key(|value| constant_sort_key(value));
                for value in values {
                    self.constant_with_force(value, true);
                }
            }
            Constant::Slice { lower, upper, step } => {
                let Some(flag) = self.begin_object(constant_key(value), force_reference) else {
                    return;
                };
                self.byte(TYPE_SLICE | flag);
                self.slice_member(lower);
                self.slice_member(upper);
                self.slice_member(step);
            }
            Constant::Code(code) => self.write_code(code, false),
        }
    }

    fn slice_member(&mut self, value: &Constant) {
        if !slice_member_uses_identity(value) {
            self.constant(value);
            return;
        }
        let key = ObjectKey::SliceMember(std::ptr::from_ref(value).addr());
        let Some(flag) = self.begin_object(key, false) else {
            return;
        };
        match value {
            Constant::Int(value) => {
                if let Ok(value) = i32::try_from(*value) {
                    self.byte(TYPE_INT | flag);
                    self.output.extend_from_slice(&value.to_le_bytes());
                } else {
                    self.byte(TYPE_LONG | flag);
                    let digit_count = (64 - value.leading_zeros()).div_ceil(15).max(1);
                    self.long(digit_count);
                    let mut remaining = *value;
                    for _ in 0..digit_count {
                        self.output
                            .extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
                        remaining >>= 15;
                    }
                }
            }
            Constant::SignedInt(value) => {
                if let Ok(value) = i32::try_from(*value) {
                    self.byte(TYPE_INT | flag);
                    self.output.extend_from_slice(&value.to_le_bytes());
                } else {
                    self.byte(TYPE_LONG | flag);
                    let magnitude = value.unsigned_abs();
                    let digit_count = (64 - magnitude.leading_zeros()).div_ceil(15).max(1);
                    self.long(0_u32.wrapping_sub(digit_count));
                    let mut remaining = magnitude;
                    for _ in 0..digit_count {
                        self.output
                            .extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
                        remaining >>= 15;
                    }
                }
            }
            Constant::BigInt { negative, digits } => {
                self.byte(TYPE_LONG | flag);
                let digit_count =
                    u32::try_from(digits.len()).expect("marshal integer limit exceeded");
                self.long(if *negative {
                    0_u32.wrapping_sub(digit_count)
                } else {
                    digit_count
                });
                for digit in digits {
                    self.output.extend_from_slice(&digit.to_le_bytes());
                }
            }
            Constant::Float(value) => {
                self.byte(TYPE_BINARY_FLOAT | flag);
                self.output.extend_from_slice(&value.to_le_bytes());
            }
            Constant::Complex { real, imag } => {
                self.byte(TYPE_BINARY_COMPLEX | flag);
                self.output.extend_from_slice(&real.to_le_bytes());
                self.output.extend_from_slice(&imag.to_le_bytes());
            }
            Constant::String(value) => {
                let bytes = value.as_bytes();
                if value.is_ascii() && bytes.len() < 256 {
                    self.byte(TYPE_SHORT_ASCII | flag);
                    self.byte(
                        u8::try_from(bytes.len()).expect("short ASCII string length fits in u8"),
                    );
                } else {
                    self.byte(if value.is_ascii() {
                        TYPE_ASCII | flag
                    } else {
                        TYPE_UNICODE | flag
                    });
                    self.long(
                        bytes
                            .len()
                            .try_into()
                            .expect("marshal string exceeds 4 GiB"),
                    );
                }
                self.output.extend_from_slice(bytes);
            }
            Constant::SurrogateString(value) => {
                self.byte(TYPE_UNICODE | flag);
                self.long(
                    value
                        .len()
                        .try_into()
                        .expect("marshal string exceeds 4 GiB"),
                );
                self.output.extend_from_slice(value);
            }
            Constant::Bytes(value) => {
                self.byte(TYPE_BYTES | flag);
                self.long(value.len().try_into().expect("marshal value exceeds 4 GiB"));
                self.output.extend_from_slice(value);
            }
            _ => unreachable!("only non-cached scalar slice members use identity keys"),
        }
    }

    fn integer(&mut self, value: u64, force_reference: bool) {
        let Some(flag) = self.begin_object(ObjectKey::Int(value), force_reference) else {
            return;
        };
        if let Ok(value) = i32::try_from(value) {
            self.byte(TYPE_INT | flag);
            self.output.extend_from_slice(&value.to_le_bytes());
            return;
        }

        self.byte(TYPE_LONG | flag);
        let digit_count = (64 - value.leading_zeros()).div_ceil(15).max(1);
        self.long(digit_count);
        let mut remaining = value;
        for _ in 0..digit_count {
            self.output
                .extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
            remaining >>= 15;
        }
    }

    fn signed_integer(&mut self, value: i64, force_reference: bool) {
        let Some(flag) = self.begin_object(ObjectKey::SignedInt(value), force_reference) else {
            return;
        };
        if let Ok(value) = i32::try_from(value) {
            self.byte(TYPE_INT | flag);
            self.output.extend_from_slice(&value.to_le_bytes());
            return;
        }

        self.byte(TYPE_LONG | flag);
        let magnitude = value.unsigned_abs();
        let digit_count = (64 - magnitude.leading_zeros()).div_ceil(15).max(1);
        self.long(0_u32.wrapping_sub(digit_count));
        let mut remaining = magnitude;
        for _ in 0..digit_count {
            self.output
                .extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
            remaining >>= 15;
        }
    }

    fn big_integer(&mut self, value: &[u16], negative: bool, force_reference: bool) {
        let Some(flag) =
            self.begin_object(ObjectKey::BigInt(negative, value.to_vec()), force_reference)
        else {
            return;
        };
        self.byte(TYPE_LONG | flag);
        let digit_count = u32::try_from(value.len()).expect("marshal integer limit exceeded");
        self.long(if negative {
            0_u32.wrapping_sub(digit_count)
        } else {
            digit_count
        });
        for digit in value {
            self.output.extend_from_slice(&digit.to_le_bytes());
        }
    }

    fn write_code(&mut self, code: &CodeObject, force_reference: bool) {
        let Some(flag) = self.begin_object(code_key(code), force_reference) else {
            return;
        };
        self.byte(TYPE_CODE | flag);
        self.long(code.arg_count);
        self.long(code.positional_only_arg_count);
        self.long(code.keyword_only_arg_count);
        self.long(code.stack_size);
        self.long(code.flags);
        self.bytes_with_key(code_bytes_key(code), &code.bytecode, true);
        self.constants(&code.constants, false);
        self.strings(&code.names, false);
        self.locals(code);
        self.bytes_with_key(local_kinds_key(code), &code.local_kinds, false);
        self.string(&code.filename);
        self.string(&code.name);
        self.string(&code.qualified_name);
        self.long(code.first_line_number);
        self.bytes(&code.line_table, false);
        self.bytes(&code.exception_table, false);
    }
}
