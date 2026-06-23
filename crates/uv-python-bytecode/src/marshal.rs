use std::collections::{HashMap, HashSet};

use crate::compiler::{CodeObject, Constant};

const FLAG_REF: u8 = 0x80;
const TYPE_NONE: u8 = b'N';
const TYPE_FALSE: u8 = b'F';
const TYPE_TRUE: u8 = b'T';
const TYPE_ELLIPSIS: u8 = b'.';
const TYPE_INT: u8 = b'i';
const TYPE_LONG: u8 = b'l';
const TYPE_BINARY_FLOAT: u8 = b'g';
const TYPE_BINARY_COMPLEX: u8 = b'y';
const TYPE_BYTES: u8 = b's';
const TYPE_SMALL_TUPLE: u8 = b')';
const TYPE_TUPLE: u8 = b'(';
const TYPE_CODE: u8 = b'c';
const TYPE_UNICODE: u8 = b'u';
const TYPE_INTERNED: u8 = b't';
const TYPE_ASCII: u8 = b'a';
const TYPE_ASCII_INTERNED: u8 = b'A';
const TYPE_SHORT_ASCII: u8 = b'z';
const TYPE_SHORT_ASCII_INTERNED: u8 = b'Z';
const TYPE_REF: u8 = b'r';
const TYPE_SLICE: u8 = b':';
const TYPE_FROZENSET: u8 = b'>';

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ObjectKey {
    None,
    Bool(bool),
    Ellipsis,
    Int(u64),
    SignedInt(i64),
    BigInt(Vec<u16>),
    Float(u64),
    Complex(u64, u64),
    String(String),
    Bytes(Vec<u8>),
    ConstantTuple(Vec<Self>),
    FrozenSet(Vec<Self>),
    StringTuple(Vec<String>),
    AnnotationLocals(usize),
    EmptyTuple,
    Slice(Box<Self>, Box<Self>, Box<Self>),
    Code(usize),
    CodeBytes(usize),
    LocalKinds(usize),
}

#[derive(Debug, Default)]
struct ObjectGraph {
    counts: HashMap<ObjectKey, usize>,
    expanded: HashSet<ObjectKey>,
    interned_strings: HashSet<String>,
}

impl ObjectGraph {
    fn for_code(code: &CodeObject) -> Self {
        let mut graph = Self::default();
        graph.visit_code(code);
        graph
    }

    fn record(&mut self, key: ObjectKey) -> bool {
        *self.counts.entry(key.clone()).or_default() += 1;
        self.expanded.insert(key)
    }

    fn visit_code(&mut self, code: &CodeObject) {
        let key = code_key(code);
        if !self.record(key) {
            return;
        }
        self.record(code_bytes_key(code));
        self.visit_constants_tuple(&code.constants);
        self.visit_strings_tuple(&code.names);
        self.visit_locals_tuple(code);
        self.record(local_kinds_key(code));
        self.visit_metadata_string(&code.filename);
        self.visit_metadata_string(&code.name);
        self.visit_metadata_string(&code.qualified_name);
        self.record(ObjectKey::Bytes(code.line_table.clone()));
        self.record(ObjectKey::Bytes(code.exception_table.clone()));
    }

    fn visit_constants_tuple(&mut self, values: &[Constant]) {
        let key = constants_tuple_key(values);
        if !self.record(key) {
            return;
        }
        for value in values {
            self.visit_constant(value);
        }
    }

    fn visit_strings_tuple(&mut self, values: &[String]) {
        let key = strings_tuple_key(values);
        if !self.record(key) {
            return;
        }
        for value in values {
            self.visit_metadata_string(value);
        }
    }

    fn visit_locals_tuple(&mut self, code: &CodeObject) {
        let key = locals_tuple_key(code);
        if !self.record(key) {
            return;
        }
        for value in &code.locals {
            self.visit_metadata_string(value);
        }
    }

    fn visit_metadata_string(&mut self, value: &str) {
        self.interned_strings.insert(value.to_string());
        self.record(ObjectKey::String(value.to_string()));
    }

    fn visit_constant(&mut self, value: &Constant) {
        match value {
            Constant::None | Constant::Bool(_) | Constant::Ellipsis => {}
            Constant::Int(value) => {
                self.record(ObjectKey::Int(*value));
            }
            Constant::SignedInt(value) => {
                self.record(ObjectKey::SignedInt(*value));
            }
            Constant::BigInt(value) => {
                self.record(ObjectKey::BigInt(value.clone()));
            }
            Constant::Float(value) => {
                self.record(ObjectKey::Float(value.to_bits()));
            }
            Constant::Complex { real, imag } => {
                self.record(ObjectKey::Complex(real.to_bits(), imag.to_bits()));
            }
            Constant::String(value) => {
                if should_intern(value) {
                    self.interned_strings.insert(value.clone());
                }
                self.record(ObjectKey::String(value.clone()));
            }
            Constant::Bytes(value) => {
                self.record(ObjectKey::Bytes(value.clone()));
            }
            Constant::Tuple(values) => self.visit_constants_tuple(values),
            Constant::FrozenSet(values) => {
                let key = constant_key(value);
                if self.record(key) {
                    let mut values: Vec<_> = values.iter().collect();
                    values.sort_by_key(|value| constant_sort_key(value));
                    for value in values {
                        self.visit_constant(value);
                    }
                }
            }
            Constant::Slice { lower, upper, step } => {
                let key = constant_key(value);
                if self.record(key) {
                    self.visit_constant(lower);
                    self.visit_constant(upper);
                    self.visit_constant(step);
                }
            }
            Constant::Code(code) => self.visit_code(code),
        }
    }

    fn is_shared(&self, key: &ObjectKey, force_reference: bool) -> bool {
        if force_reference || self.counts.get(key).copied().unwrap_or_default() > 1 {
            return true;
        }
        match key {
            ObjectKey::Int(value) => *value <= 256,
            ObjectKey::SignedInt(value) => (-5..=256).contains(value),
            ObjectKey::String(value) => self.interned_strings.contains(value),
            ObjectKey::Bytes(value) => value.is_empty(),
            ObjectKey::EmptyTuple => true,
            _ => false,
        }
    }
}

pub(crate) fn encode_code(code: &CodeObject) -> Vec<u8> {
    let graph = ObjectGraph::for_code(code);
    let mut writer = Writer {
        graph: &graph,
        output: Vec::new(),
        references: HashMap::new(),
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
        let Some(flag) = self.begin_object(ObjectKey::Bytes(value.to_vec()), force_reference)
        else {
            return;
        };
        self.byte(TYPE_BYTES | flag);
        self.long(value.len().try_into().expect("marshal value exceeds 4 GiB"));
        self.output.extend_from_slice(value);
    }

    fn code_bytes(&mut self, code: &CodeObject) {
        let Some(flag) = self.begin_object(code_bytes_key(code), true) else {
            return;
        };
        self.byte(TYPE_BYTES | flag);
        self.long(
            code.bytecode
                .len()
                .try_into()
                .expect("marshal value exceeds 4 GiB"),
        );
        self.output.extend_from_slice(&code.bytecode);
    }

    fn local_kinds(&mut self, code: &CodeObject) {
        let key = local_kinds_key(code);
        let Some(flag) = self.begin_object(key, false) else {
            return;
        };
        self.byte(TYPE_BYTES | flag);
        self.long(
            code.local_kinds
                .len()
                .try_into()
                .expect("marshal value exceeds 4 GiB"),
        );
        self.output.extend_from_slice(&code.local_kinds);
    }

    fn string(&mut self, value: &str) {
        self.string_with_force(value, false);
    }

    fn string_with_force(&mut self, value: &str, force_reference: bool) {
        let key = ObjectKey::String(value.to_string());
        let Some(flag) = self.begin_object(key, force_reference) else {
            return;
        };
        let interned = self.graph.interned_strings.contains(value);
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
            Constant::BigInt(value) => self.big_integer(value, force_reference),
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
            Constant::Bytes(value) => self.bytes(value, force_reference),
            Constant::Tuple(values) => {
                let key = constants_tuple_key(values);
                if self.tuple_header(key, values.len(), force_reference) {
                    for value in values {
                        self.constant(value);
                    }
                }
            }
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
                self.constant(lower);
                self.constant(upper);
                self.constant(step);
            }
            Constant::Code(code) => self.write_code(code, false),
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

    fn big_integer(&mut self, value: &[u16], force_reference: bool) {
        let Some(flag) = self.begin_object(ObjectKey::BigInt(value.to_vec()), force_reference)
        else {
            return;
        };
        self.byte(TYPE_LONG | flag);
        self.long(
            value
                .len()
                .try_into()
                .expect("marshal integer limit exceeded"),
        );
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
        self.code_bytes(code);
        self.constants(&code.constants, false);
        self.strings(&code.names, false);
        self.locals(code);
        self.local_kinds(code);
        self.string(&code.filename);
        self.string(&code.name);
        self.string(&code.qualified_name);
        self.long(code.first_line_number);
        self.bytes(&code.line_table, false);
        self.bytes(&code.exception_table, false);
    }
}

fn code_key(code: &CodeObject) -> ObjectKey {
    ObjectKey::Code(std::ptr::from_ref(code).addr())
}

fn code_bytes_key(code: &CodeObject) -> ObjectKey {
    ObjectKey::CodeBytes(std::ptr::from_ref(code).addr())
}

fn local_kinds_key(code: &CodeObject) -> ObjectKey {
    if code.local_kinds.is_empty() {
        ObjectKey::Bytes(Vec::new())
    } else {
        ObjectKey::LocalKinds(std::ptr::from_ref(code).addr())
    }
}

fn constant_key(value: &Constant) -> ObjectKey {
    match value {
        Constant::None => ObjectKey::None,
        Constant::Bool(value) => ObjectKey::Bool(*value),
        Constant::Ellipsis => ObjectKey::Ellipsis,
        Constant::Int(value) => ObjectKey::Int(*value),
        Constant::SignedInt(value) => ObjectKey::SignedInt(*value),
        Constant::BigInt(value) => ObjectKey::BigInt(value.clone()),
        Constant::Float(value) => ObjectKey::Float(value.to_bits()),
        Constant::Complex { real, imag } => ObjectKey::Complex(real.to_bits(), imag.to_bits()),
        Constant::String(value) => ObjectKey::String(value.clone()),
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

fn constant_sort_key(value: &Constant) -> Vec<u8> {
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
            Constant::BigInt(value) => {
                output.push(TYPE_LONG | flag);
                output.extend_from_slice(
                    &u32::try_from(value.len())
                        .expect("sort-key integer length fits in u32")
                        .to_le_bytes(),
                );
                for digit in value {
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
            Constant::Bytes(value) => {
                output.push(TYPE_BYTES | flag);
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

fn constants_tuple_key(values: &[Constant]) -> ObjectKey {
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

fn strings_tuple_key(values: &[String]) -> ObjectKey {
    if values.is_empty() {
        ObjectKey::EmptyTuple
    } else {
        ObjectKey::StringTuple(values.to_vec())
    }
}

fn locals_tuple_key(code: &CodeObject) -> ObjectKey {
    if code.locals.is_empty() {
        ObjectKey::EmptyTuple
    } else if code.annotation_thunk {
        ObjectKey::AnnotationLocals(std::ptr::from_ref(code).addr())
    } else {
        strings_tuple_key(&code.locals)
    }
}

fn should_intern(value: &str) -> bool {
    value.len() <= 1
        || value
            .chars()
            .all(|character| character == '_' || character.is_alphanumeric())
}
