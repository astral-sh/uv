use std::collections::{HashMap, HashSet};

use crate::compiler::{CodeObject, Constant};

use super::key::{
    ObjectKey, code_bytes_key, code_key, constant_key, constant_sort_key, constants_tuple_key,
    local_kinds_key, locals_tuple_key, should_intern, slice_member_uses_identity,
    strings_tuple_key,
};

#[derive(Debug, Default)]
pub(super) struct ObjectGraph {
    objects: HashMap<ObjectKey, usize>,
    interned_strings: HashSet<String>,
}

impl ObjectGraph {
    pub(super) fn for_code(code: &CodeObject) -> Self {
        let mut graph = Self::default();
        graph.visit_code(code);
        graph
    }

    pub(super) fn record(&mut self, key: ObjectKey) -> bool {
        let count = self.objects.entry(key).or_default();
        *count += 1;
        *count == 1
    }

    fn visit_code(&mut self, code: &CodeObject) {
        let key = code_key(code);
        if !self.record(key) {
            return;
        }
        self.record(code_bytes_key(code));
        self.interned_strings
            .extend(code.interned_constant_strings.iter().cloned());
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
            Constant::BigInt { negative, digits } => {
                self.record(ObjectKey::BigInt(*negative, digits.clone()));
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
            Constant::SurrogateString(value) => {
                self.record(ObjectKey::SurrogateString(value.clone()));
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
                    self.visit_slice_member(lower);
                    self.visit_slice_member(upper);
                    self.visit_slice_member(step);
                }
            }
            Constant::Code(code) => self.visit_code(code),
        }
    }

    fn visit_slice_member(&mut self, value: &Constant) {
        if slice_member_uses_identity(value) {
            self.record(ObjectKey::SliceMember(std::ptr::from_ref(value).addr()));
        } else {
            self.visit_constant(value);
        }
    }

    pub(super) fn is_shared(&self, key: &ObjectKey, force_reference: bool) -> bool {
        if force_reference || self.objects.get(key).is_some_and(|count| *count > 1) {
            return true;
        }
        match key {
            ObjectKey::Int(value) => *value <= 256,
            ObjectKey::SignedInt(value) => (-5..=256).contains(value),
            ObjectKey::String(value) => self.interned_strings.contains(value),
            ObjectKey::Bytes(value) => value.len() <= 1,
            ObjectKey::EmptyTuple => true,
            _ => false,
        }
    }

    pub(super) fn is_interned(&self, value: &str) -> bool {
        self.interned_strings.contains(value)
    }
}
