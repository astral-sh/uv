use crate::compiler::{CodeObject, Constant};

use super::v5::{
    FLAG_REF, TYPE_ASCII, TYPE_ASCII_INTERNED, TYPE_BINARY_COMPLEX, TYPE_BINARY_FLOAT, TYPE_CODE,
    TYPE_ELLIPSIS, TYPE_FALSE, TYPE_FROZENSET, TYPE_INT, TYPE_INTERNED, TYPE_LONG, TYPE_NONE,
    TYPE_SHORT_ASCII, TYPE_SHORT_ASCII_INTERNED, TYPE_SLICE, TYPE_SMALL_TUPLE, TYPE_STRING,
    TYPE_TRUE, TYPE_TUPLE, TYPE_UNICODE,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum ObjectKey {
    // Scalar and immutable-container keys use Python value equality.
    None,
    Bool(bool),
    Ellipsis,
    Int(u64),
    SignedInt(i64),
    BigInt(bool, Vec<u16>),
    Float(u64),
    Complex(u64, u64),
    String(String),
    SurrogateString(Vec<u8>),
    Bytes(Vec<u8>),
    ConstantTuple(Vec<Self>),
    FrozenSet(Vec<Self>),
    StringTuple(Vec<String>),
    // Annotation-scope locals retain identity even when their contents compare equal.
    AnnotationLocals(usize),
    // CPython reuses one canonical empty tuple.
    EmptyTuple,
    Slice(Box<Self>, Box<Self>, Box<Self>),
    // These objects also retain identity. The address is used only as a marshal-graph key and is
    // never written to the output.
    SliceMember(usize),
    Code(usize),
    CodeBytes(usize),
    LocalKinds(usize),
}

pub(super) fn code_key(code: &CodeObject) -> ObjectKey {
    ObjectKey::Code(std::ptr::from_ref(code).addr())
}

pub(super) fn code_bytes_key(code: &CodeObject) -> ObjectKey {
    ObjectKey::CodeBytes(std::ptr::from_ref(code).addr())
}

pub(super) fn local_kinds_key(code: &CodeObject) -> ObjectKey {
    if code.local_kinds.is_empty() {
        ObjectKey::Bytes(Vec::new())
    } else {
        ObjectKey::LocalKinds(std::ptr::from_ref(code).addr())
    }
}

/// Returns whether CPython gives a scalar slice member fresh identity instead of using its cache.
pub(super) fn slice_member_uses_identity(value: &Constant) -> bool {
    match value {
        Constant::Int(value) => *value > 256,
        Constant::SignedInt(value) => !(-5..=256).contains(value),
        Constant::BigInt { .. } | Constant::Float(_) | Constant::Complex { .. } => true,
        Constant::String(value) => !is_cached_character(value),
        Constant::SurrogateString(_) => true,
        Constant::Bytes(value) => value.len() > 1,
        Constant::None
        | Constant::Bool(_)
        | Constant::Ellipsis
        | Constant::Tuple(_)
        | Constant::FrozenSet(_)
        | Constant::Slice { .. }
        | Constant::Code(_) => false,
    }
}

/// Builds the value key used for constants, except for code objects whose identity is significant.
pub(super) fn constant_key(value: &Constant) -> ObjectKey {
    match value {
        Constant::None => ObjectKey::None,
        Constant::Bool(value) => ObjectKey::Bool(*value),
        Constant::Ellipsis => ObjectKey::Ellipsis,
        Constant::Int(value) => ObjectKey::Int(*value),
        Constant::SignedInt(value) => ObjectKey::SignedInt(*value),
        Constant::BigInt { negative, digits } => ObjectKey::BigInt(*negative, digits.clone()),
        Constant::Float(value) => ObjectKey::Float(value.to_bits()),
        Constant::Complex { real, imag } => ObjectKey::Complex(real.to_bits(), imag.to_bits()),
        Constant::String(value) => ObjectKey::String(value.clone()),
        Constant::SurrogateString(value) => ObjectKey::SurrogateString(value.clone()),
        Constant::Bytes(value) => ObjectKey::Bytes(value.clone()),
        Constant::Tuple(values) => constants_tuple_key(values),
        Constant::FrozenSet(values) => {
            let mut values: Vec<_> = values.iter().map(constant_key).collect();
            values.sort_by_key(|value| format!("{value:?}"));
            ObjectKey::FrozenSet(values)
        }
        Constant::Slice { lower, upper, step } => ObjectKey::Slice(
            Box::new(constant_key(lower)),
            Box::new(constant_key(upper)),
            Box::new(constant_key(step)),
        ),
        Constant::Code(code) => code_key(code),
    }
}

pub(super) fn constant_sort_key(value: &Constant) -> Vec<u8> {
    fn append(output: &mut Vec<u8>, value: &Constant, force_reference: bool) {
        let flag = if force_reference { FLAG_REF } else { 0 };
        match value {
            Constant::None => output.push(TYPE_NONE),
            Constant::Bool(false) => output.push(TYPE_FALSE),
            Constant::Bool(true) => output.push(TYPE_TRUE),
            Constant::Ellipsis => output.push(TYPE_ELLIPSIS),
            Constant::Int(value) => {
                if let Ok(value) = i32::try_from(*value) {
                    output.push(TYPE_INT | flag);
                    output.extend_from_slice(&value.to_le_bytes());
                } else {
                    output.push(TYPE_LONG | flag);
                    let digit_count = (64 - value.leading_zeros()).div_ceil(15).max(1);
                    output.extend_from_slice(&digit_count.to_le_bytes());
                    let mut remaining = *value;
                    for _ in 0..digit_count {
                        output.extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
                        remaining >>= 15;
                    }
                }
            }
            Constant::SignedInt(value) => {
                if let Ok(value) = i32::try_from(*value) {
                    output.push(TYPE_INT | flag);
                    output.extend_from_slice(&value.to_le_bytes());
                } else {
                    output.push(TYPE_LONG | flag);
                    let magnitude = value.unsigned_abs();
                    let digit_count = (64 - magnitude.leading_zeros()).div_ceil(15).max(1);
                    output.extend_from_slice(&0_u32.wrapping_sub(digit_count).to_le_bytes());
                    let mut remaining = magnitude;
                    for _ in 0..digit_count {
                        output.extend_from_slice(&((remaining & 0x7fff) as u16).to_le_bytes());
                        remaining >>= 15;
                    }
                }
            }
            Constant::BigInt { negative, digits } => {
                output.push(TYPE_LONG | flag);
                let digit_count =
                    u32::try_from(digits.len()).expect("sort-key integer length fits in u32");
                output.extend_from_slice(
                    &if *negative {
                        0_u32.wrapping_sub(digit_count)
                    } else {
                        digit_count
                    }
                    .to_le_bytes(),
                );
                for digit in digits {
                    output.extend_from_slice(&digit.to_le_bytes());
                }
            }
            Constant::Float(value) => {
                output.push(TYPE_BINARY_FLOAT | flag);
                output.extend_from_slice(&value.to_le_bytes());
            }
            Constant::Complex { real, imag } => {
                output.push(TYPE_BINARY_COMPLEX | flag);
                output.extend_from_slice(&real.to_le_bytes());
                output.extend_from_slice(&imag.to_le_bytes());
            }
            Constant::String(value) => {
                let bytes = value.as_bytes();
                let interned = should_intern(value);
                if value.is_ascii() && bytes.len() < 256 {
                    output.push(
                        (if interned {
                            TYPE_SHORT_ASCII_INTERNED
                        } else {
                            TYPE_SHORT_ASCII
                        }) | flag,
                    );
                    output.push(
                        u8::try_from(bytes.len()).expect("short ASCII sort-key length fits in u8"),
                    );
                } else {
                    output.push(
                        (if value.is_ascii() {
                            if interned {
                                TYPE_ASCII_INTERNED
                            } else {
                                TYPE_ASCII
                            }
                        } else if interned {
                            TYPE_INTERNED
                        } else {
                            TYPE_UNICODE
                        }) | flag,
                    );
                    output.extend_from_slice(
                        &u32::try_from(bytes.len())
                            .expect("sort-key string length fits in u32")
                            .to_le_bytes(),
                    );
                }
                output.extend_from_slice(bytes);
            }
            Constant::SurrogateString(value) => {
                output.push(TYPE_UNICODE | flag);
                output.extend_from_slice(
                    &u32::try_from(value.len())
                        .expect("sort-key string length fits in u32")
                        .to_le_bytes(),
                );
                output.extend_from_slice(value);
            }
            Constant::Bytes(value) => {
                output.push(TYPE_STRING | flag);
                output.extend_from_slice(
                    &u32::try_from(value.len())
                        .expect("sort-key bytes length fits in u32")
                        .to_le_bytes(),
                );
                output.extend_from_slice(value);
            }
            Constant::Tuple(values) => {
                output.push(
                    (if values.len() < 256 {
                        TYPE_SMALL_TUPLE
                    } else {
                        TYPE_TUPLE
                    }) | flag,
                );
                if values.len() < 256 {
                    output.push(
                        u8::try_from(values.len()).expect("small tuple sort-key length fits in u8"),
                    );
                } else {
                    output.extend_from_slice(
                        &u32::try_from(values.len())
                            .expect("sort-key tuple length fits in u32")
                            .to_le_bytes(),
                    );
                }
                for value in values {
                    append(output, value, true);
                }
            }
            Constant::FrozenSet(values) => {
                output.push(TYPE_FROZENSET | flag);
                output.extend_from_slice(
                    &u32::try_from(values.len())
                        .expect("sort-key frozenset length fits in u32")
                        .to_le_bytes(),
                );
            }
            Constant::Slice { lower, upper, step } => {
                output.push(TYPE_SLICE | flag);
                append(output, lower, false);
                append(output, upper, false);
                append(output, step, false);
            }
            Constant::Code(_) => output.push(TYPE_CODE | flag),
        }
    }

    let mut output = Vec::new();
    append(&mut output, value, true);
    output
}

pub(super) fn constants_tuple_key(values: &[Constant]) -> ObjectKey {
    if values.is_empty() {
        ObjectKey::EmptyTuple
    } else if let Some(values) = values
        .iter()
        .map(|value| match value {
            Constant::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        ObjectKey::StringTuple(values)
    } else {
        ObjectKey::ConstantTuple(values.iter().map(constant_key).collect())
    }
}

pub(super) fn strings_tuple_key(values: &[String]) -> ObjectKey {
    if values.is_empty() {
        ObjectKey::EmptyTuple
    } else {
        ObjectKey::StringTuple(values.to_vec())
    }
}

pub(super) fn locals_tuple_key(code: &CodeObject) -> ObjectKey {
    if code.locals.is_empty() {
        ObjectKey::EmptyTuple
    } else if code.annotation_thunk {
        ObjectKey::AnnotationLocals(std::ptr::from_ref(code).addr())
    } else {
        strings_tuple_key(&code.locals)
    }
}

pub(super) fn should_intern(value: &str) -> bool {
    is_cached_character(value)
        || value.is_ascii()
            && value
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

fn is_cached_character(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| characters.next().is_none() && u32::from(character) <= 0xff)
}
