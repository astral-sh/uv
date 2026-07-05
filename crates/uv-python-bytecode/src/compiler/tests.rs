use ruff_python_ast::Stmt;
use ruff_python_parser::parse_module;

use super::{Compiler, Constant, FunctionPlan, NameId, NameInterner, ScopeBodyFacts, clean_doc};

#[test]
fn interns_adversarial_identifier_families_densely() {
    let mut interner = NameInterner::default();
    let names = (0..4096)
        .map(|index| format!("collision_prefix_{index:04}_collision_suffix"))
        .collect::<Vec<_>>();

    for (index, name) in names.iter().enumerate() {
        let id = interner.intern(name.clone());
        assert_eq!(id, NameId(u32::try_from(index).unwrap()));
        assert_eq!(interner.resolve(id), name);
    }
    for (index, name) in names.iter().enumerate().rev() {
        assert_eq!(
            interner.intern(name.clone()),
            NameId(u32::try_from(index).unwrap())
        );
    }
}

#[test]
fn name_ids_do_not_define_observable_name_order() {
    let mut interner = NameInterner::default();
    let source_order = ["zeta", "alpha", "beta", "_hidden", "éclair"]
        .map(|name| interner.intern(name.to_string()));
    let mut lexical_order = Vec::new();
    for name in source_order {
        interner.insert_lexical(&mut lexical_order, name);
    }

    assert_eq!(
        lexical_order
            .iter()
            .map(|name| interner.resolve(*name))
            .collect::<Vec<_>>(),
        ["_hidden", "alpha", "beta", "zeta", "éclair"]
    );
    assert_eq!(source_order[0], NameId(0));
    assert_eq!(source_order[1], NameId(1));

    let plan = FunctionPlan {
        locals: source_order.to_vec(),
        freevars: lexical_order,
        ..FunctionPlan::default()
    }
    .materialize(&interner);
    assert_eq!(plan.locals, ["zeta", "alpha", "beta", "_hidden", "éclair"]);
    assert_eq!(
        plan.freevars.into_iter().collect::<Vec<_>>(),
        ["_hidden", "alpha", "beta", "zeta", "éclair"]
    );
}

#[test]
fn name_ids_deduplicate_private_name_mangling_collisions() {
    let source = r"class Container:
    __value = 1
    _Container__value = 2

    def method(self):
        return self.__value
";
    let parsed = parse_module(source).unwrap();
    let module = Compiler::module("example.py", source)
        .compile_module(parsed.suite())
        .unwrap();
    let class = module
        .constants
        .iter()
        .find_map(|constant| match constant {
            Constant::Code(code) if code.name == "Container" => Some(code),
            _ => None,
        })
        .expect("module should contain the class body");

    assert_eq!(
        class
            .names
            .iter()
            .filter(|name| name.as_str() == "_Container__value")
            .count(),
        1
    );
}

#[test]
fn scope_body_facts_preserve_named_scope_and_annotation_boundaries() {
    let source = r"def outer():
    global module_name
    nonlocal captured
    item: annotation = value
    closure = lambda: captured
    generated = ((captured := element) for element in values)
    inlined = [lambda: element for element in values]

    def child[U: child_bound]():
        return child_body

    class Child[T: class_bound](base):
        body = class_body

    type Alias[V: alias_bound] = tuple[V, alias_value]
";
    let parsed = parse_module(source).unwrap();
    let Stmt::FunctionDef(definition) = &parsed.suite()[0] else {
        panic!("expected a function definition");
    };
    let facts = ScopeBodyFacts::for_function(&definition.body);

    assert!(facts.globals.contains("module_name"));
    assert!(facts.nonlocals.contains("captured"));
    assert!(facts.references.contains("value"));
    assert!(!facts.references.contains("annotation"));
    assert!(!facts.references.contains("child_body"));
    assert!(!facts.references.contains("class_body"));
    assert!(facts.nested_lambda_requirements.contains("captured"));
    assert!(facts.nested_generator_requirements.contains("captured"));
    assert!(facts.generator_named_targets.contains("captured"));
    assert!(facts.inlined_comprehension_cellvars.contains("element"));
    for name in [
        "child_bound",
        "class_bound",
        "base",
        "alias_bound",
        "alias_value",
    ] {
        assert!(
            facts.nested_annotation_scope_requirements.contains(name),
            "missing annotation-scope requirement {name}"
        );
    }
    assert!(facts.has_function_definition);
    assert!(facts.has_generic_definition);
    assert!(facts.has_type_alias);
    assert!(facts.has_simple_annotations);

    let source = "class Container:\n    item: annotation = value\n";
    let parsed = parse_module(source).unwrap();
    let Stmt::ClassDef(definition) = &parsed.suite()[0] else {
        panic!("expected a class definition");
    };
    let deferred = ScopeBodyFacts::for_class(&definition.body, true);
    assert!(deferred.references.contains("value"));
    assert!(!deferred.references.contains("annotation"));
    let eager = ScopeBodyFacts::for_class(&definition.body, false);
    assert!(eager.references.contains("value"));
    assert!(eager.references.contains("annotation"));
}

#[test]
fn compiles_a_simple_module() {
    let parsed = parse_module("answer = 6 * 7\nprint(answer)\n").unwrap();
    let code = Compiler::module("example.py", "answer = 6 * 7\nprint(answer)\n")
        .compile_module(parsed.suite())
        .unwrap();
    assert_eq!(code.name, "<module>");
    assert!(code.stack_size >= 2);
    assert!(code.names.iter().any(|name| name == "print"));
}

#[test]
fn compiles_type_alias() {
    let source = "type Alias = int\n";
    let parsed = parse_module(source).unwrap();
    Compiler::module("example.py", source)
        .compile_module(parsed.suite())
        .unwrap();
}

#[test]
fn source_positions_use_utf8_byte_columns() {
    let compiler = Compiler::module("example.py", "α = 1\nβγ = 2\n");

    assert_eq!(compiler.source_position(0), (1, 0));
    assert_eq!(compiler.source_position(2), (1, 2));
    assert_eq!(compiler.source_position(7), (2, 0));
    assert_eq!(compiler.source_position(9), (2, 2));
    assert_eq!(compiler.source_position(11), (2, 4));
    assert_eq!(compiler.line_number(11), 2);
}

#[test]
fn source_positions_clamp_to_eof() {
    let source = "first\né\n";
    let compiler = Compiler::module("example.py", source);
    let eof = u32::try_from(source.len()).unwrap();

    assert_eq!(compiler.source_position(eof), (3, 0));
    assert_eq!(compiler.source_position(u32::MAX), (3, 0));
    assert_eq!(compiler.line_number(u32::MAX), 3);
}

#[test]
fn cleans_docstrings_like_cpython() {
    assert_eq!(
        clean_doc("  first\n        second\n      third\n"),
        "first\n  second\nthird\n"
    );
    assert_eq!(clean_doc("\tfirst\n\t\tsecond"), "first\nsecond");
}
