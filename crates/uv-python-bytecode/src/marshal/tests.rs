use rustc_hash::FxHashSet;

use crate::compiler::{CodeObject, Constant};

use super::encode_code;
use super::graph::ObjectGraph;
use super::key::{
    ObjectKey, code_bytes_key, code_key, constant_key, local_kinds_key, locals_tuple_key,
    slice_member_uses_identity,
};

fn code_object(constants: Vec<Constant>) -> CodeObject {
    CodeObject {
        arg_count: 0,
        positional_only_arg_count: 0,
        keyword_only_arg_count: 0,
        stack_size: 1,
        flags: 0,
        bytecode: Vec::new(),
        constants,
        names: Vec::new(),
        locals: Vec::new(),
        local_kinds: Vec::new(),
        filename: "module.py".to_string(),
        name: "<module>".to_string(),
        qualified_name: "<module>".to_string(),
        first_line_number: 1,
        line_table: Vec::new(),
        exception_table: Vec::new(),
        annotation_thunk: false,
        interned_constant_strings: FxHashSet::default(),
    }
}

#[test]
fn counts_repeated_objects_without_reexpanding_them() {
    let mut graph = ObjectGraph::default();
    let key = ObjectKey::Float(1.0_f64.to_bits());

    assert!(graph.record(key.clone()));
    assert!(!graph.is_shared(&key, false));
    assert!(!graph.record(key.clone()));
    assert!(graph.is_shared(&key, false));
}

#[test]
fn uses_cpython_reference_cache_boundaries() {
    let graph = ObjectGraph::default();
    for key in [
        ObjectKey::Int(256),
        ObjectKey::SignedInt(-5),
        ObjectKey::Bytes(Vec::new()),
        ObjectKey::Bytes(vec![0]),
        ObjectKey::EmptyTuple,
    ] {
        assert!(graph.is_shared(&key, false), "expected cached key {key:?}");
    }
    for key in [
        ObjectKey::Int(257),
        ObjectKey::SignedInt(-6),
        ObjectKey::Bytes(vec![0, 1]),
        ObjectKey::Float(1.0_f64.to_bits()),
    ] {
        assert!(
            !graph.is_shared(&key, false),
            "unexpected cached key {key:?}"
        );
    }
    assert!(graph.is_shared(&ObjectKey::Float(0), true));
}

#[test]
fn tracks_interned_strings_from_constants_and_metadata() {
    let mut code = code_object(vec![
        Constant::String("identifier".to_string()),
        Constant::String("not interned".to_string()),
    ]);
    code.names.push("metadata name with spaces".to_string());
    let graph = ObjectGraph::for_code(&code);

    assert!(graph.is_interned("identifier"));
    assert!(graph.is_interned("metadata name with spaces"));
    assert!(graph.is_interned("module.py"));
    assert!(!graph.is_interned("not interned"));
}

#[test]
fn keeps_identity_and_value_keys_separate() {
    let mut first = code_object(Vec::new());
    let mut second = first.clone();
    assert_ne!(code_key(&first), code_key(&second));
    assert_ne!(code_bytes_key(&first), code_bytes_key(&second));

    assert_eq!(local_kinds_key(&first), local_kinds_key(&second));
    first.local_kinds.push(1);
    second.local_kinds.push(1);
    assert_ne!(local_kinds_key(&first), local_kinds_key(&second));

    first.locals.push("value".to_string());
    second.locals.push("value".to_string());
    assert_eq!(locals_tuple_key(&first), locals_tuple_key(&second));
    first.annotation_thunk = true;
    second.annotation_thunk = true;
    assert_ne!(locals_tuple_key(&first), locals_tuple_key(&second));
}

#[test]
fn distinguishes_cached_slice_members_from_fresh_objects() {
    for value in [
        Constant::Int(256),
        Constant::SignedInt(-5),
        Constant::String("é".to_string()),
        Constant::Bytes(vec![0]),
    ] {
        assert!(!slice_member_uses_identity(&value), "cached {value:?}");
    }
    for value in [
        Constant::Int(257),
        Constant::SignedInt(-6),
        Constant::String("Ā".to_string()),
        Constant::Bytes(vec![0, 1]),
        Constant::Float(1.0),
    ] {
        assert!(slice_member_uses_identity(&value), "fresh {value:?}");
    }
}

#[test]
fn frozen_set_keys_and_reference_order_ignore_input_order() {
    let values = vec![
        Constant::Int(1),
        Constant::String("value".to_string()),
        Constant::Tuple(vec![Constant::SignedInt(-1), Constant::Bool(true)]),
    ];
    let mut reversed = values.clone();
    reversed.reverse();

    let first_key = constant_key(&Constant::FrozenSet(values.clone()));
    let second_key = constant_key(&Constant::FrozenSet(reversed.clone()));
    assert_eq!(first_key, second_key);

    let first = code_object(vec![Constant::FrozenSet(values)]);
    let second = code_object(vec![Constant::FrozenSet(reversed)]);
    assert_eq!(encode_code(&first), encode_code(&second));
}
