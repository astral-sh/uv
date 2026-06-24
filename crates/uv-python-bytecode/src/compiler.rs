use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_pattern, walk_stmt};
use ruff_python_ast::{
    BoolOp, CmpOp, ConversionFlag, Expr, ExprBinOp, ExprContext, FStringPart,
    InterpolatedStringElement, Keyword, Number, Operator, Pattern, Singleton, Stmt, StmtClassDef,
    StmtFunctionDef, Suite, TypeParam, UnaryOp,
};
use ruff_python_codegen::{Generator, Indentation, Mode as CodegenMode};
use ruff_source_file::LineEnding;
use ruff_text_size::Ranged;

use crate::CompileError;
use crate::assembler::{Assembler, InstructionId, Label, Opcode, SourceLocation};

const RESUME: Opcode = Opcode::new(128, 0);
const BUILD_TEMPLATE: Opcode = Opcode::new(2, 0);
const BINARY_SLICE: Opcode = Opcode::new(1, 0);
const CLEANUP_THROW: Opcode = Opcode::new(7, 0);
const DELETE_SUBSCR: Opcode = Opcode::new(8, 0);
const END_FOR: Opcode = Opcode::new(9, 0);
const GET_AITER: Opcode = Opcode::new(14, 0);
const GET_ANEXT: Opcode = Opcode::new(15, 0);
const GET_ITER: Opcode = Opcode::new(16, 0);
const GET_AWAITABLE: Opcode = Opcode::new(71, 0);
const GET_YIELD_FROM_ITER: Opcode = Opcode::new(19, 0);
const END_SEND: Opcode = Opcode::new(10, 0);
const LOAD_BUILD_CLASS: Opcode = Opcode::new(21, 0);
const LOAD_LOCALS: Opcode = Opcode::new(22, 0);
const NOP: Opcode = Opcode::new(27, 0);
const POP_EXCEPT: Opcode = Opcode::new(29, 0);
const PUSH_EXC_INFO: Opcode = Opcode::new(32, 0);
const RETURN_GENERATOR: Opcode = Opcode::new(34, 0);
const WITH_EXCEPT_START: Opcode = Opcode::new(43, 0);
const LOAD_CONST: Opcode = Opcode::new(82, 0);
const LOAD_DEREF: Opcode = Opcode::new(83, 0);
const LOAD_NAME: Opcode = Opcode::new(93, 0);
const LOAD_SMALL_INT: Opcode = Opcode::new(94, 0);
const LOAD_SPECIAL: Opcode = Opcode::new(95, 0);
const LOAD_SUPER_ATTR: Opcode = Opcode::new(96, 1);
const STORE_NAME: Opcode = Opcode::new(116, 0);
const DELETE_NAME: Opcode = Opcode::new(65, 0);
const STORE_SLICE: Opcode = Opcode::new(37, 0);
// CPython initially emits owned local loads, then strength-reduces safe loads
// to borrowed references after the control-flow graph has reached its final
// shape. The assembler mirrors that dataflow pass.
const LOAD_FAST: Opcode = Opcode::new(84, 0);
const LOAD_FAST_CHECK: Opcode = Opcode::new(88, 0);
// Internal marker: the assembler lowers this to `LOAD_FAST`, while retaining
// the code generator's conservative ownership hint for the final CFG pass.
const LOAD_FAST_OWNED: Opcode = Opcode::new(121, 0);
const LOAD_FAST_AND_CLEAR: Opcode = Opcode::new(85, 0);
const LOAD_FROM_DICT_OR_DEREF: Opcode = Opcode::new(90, 0);
const LOAD_FROM_DICT_OR_GLOBALS: Opcode = Opcode::new(91, 0);
const STORE_FAST: Opcode = Opcode::new(112, 0);
const DELETE_FAST: Opcode = Opcode::new(63, 0);
const DELETE_DEREF: Opcode = Opcode::new(62, 0);
const LOAD_GLOBAL: Opcode = Opcode::new(92, 4);
const STORE_GLOBAL: Opcode = Opcode::new(115, 0);
const STORE_DEREF: Opcode = Opcode::new(111, 0);
const DELETE_GLOBAL: Opcode = Opcode::new(64, 0);
const LOAD_ATTR: Opcode = Opcode::new(80, 9);
const STORE_ATTR: Opcode = Opcode::new(110, 4);
const DELETE_ATTR: Opcode = Opcode::new(61, 0);
const STORE_SUBSCR: Opcode = Opcode::new(38, 1);
const SETUP_ANNOTATIONS: Opcode = Opcode::new(36, 0);
const PUSH_NULL: Opcode = Opcode::new(33, 0);
const CALL: Opcode = Opcode::new(52, 3);
const CALL_KW: Opcode = Opcode::new(55, 3);
const CALL_FUNCTION_EX: Opcode = Opcode::new(4, 0);
const MAKE_FUNCTION: Opcode = Opcode::new(23, 0);
const SET_FUNCTION_ATTRIBUTE: Opcode = Opcode::new(108, 0);
const COPY_FREE_VARS: Opcode = Opcode::new(60, 0);
const MAKE_CELL: Opcode = Opcode::new(97, 0);
const POP_TOP: Opcode = Opcode::new(31, 0);
const POP_ITER: Opcode = Opcode::new(30, 0);
const NOT_TAKEN: Opcode = Opcode::new(28, 0);
const RETURN_VALUE: Opcode = Opcode::new(35, 0);
const COPY: Opcode = Opcode::new(59, 0);
const SWAP: Opcode = Opcode::new(117, 0);
const TO_BOOL: Opcode = Opcode::new(39, 3);
const UNARY_INVERT: Opcode = Opcode::new(40, 0);
const UNARY_NEGATIVE: Opcode = Opcode::new(41, 0);
const UNARY_NOT: Opcode = Opcode::new(42, 0);
const CALL_INTRINSIC_1: Opcode = Opcode::new(53, 0);
const CALL_INTRINSIC_2: Opcode = Opcode::new(54, 0);
const BINARY_OP: Opcode = Opcode::new(44, 5);
const BUILD_INTERPOLATION: Opcode = Opcode::new(45, 0);
const COMPARE_OP: Opcode = Opcode::new(56, 1);
const CONTAINS_OP: Opcode = Opcode::new(57, 1);
const IS_OP: Opcode = Opcode::new(74, 0);
const BUILD_LIST: Opcode = Opcode::new(46, 0);
const BUILD_MAP: Opcode = Opcode::new(47, 0);
const BUILD_SET: Opcode = Opcode::new(48, 0);
const BUILD_SLICE: Opcode = Opcode::new(49, 0);
const BUILD_STRING: Opcode = Opcode::new(50, 0);
const BUILD_TUPLE: Opcode = Opcode::new(51, 0);
const CONVERT_VALUE: Opcode = Opcode::new(58, 0);
const FORMAT_SIMPLE: Opcode = Opcode::new(12, 0);
const FORMAT_WITH_SPEC: Opcode = Opcode::new(13, 0);
const GET_LEN: Opcode = Opcode::new(18, 0);
const MATCH_KEYS: Opcode = Opcode::new(24, 0);
const MATCH_MAPPING: Opcode = Opcode::new(25, 0);
const MATCH_SEQUENCE: Opcode = Opcode::new(26, 0);
const LIST_APPEND: Opcode = Opcode::new(78, 0);
const LIST_EXTEND: Opcode = Opcode::new(79, 0);
const DICT_MERGE: Opcode = Opcode::new(66, 0);
const DICT_UPDATE: Opcode = Opcode::new(67, 0);
const END_ASYNC_FOR: Opcode = Opcode::new(68, 0);
const SET_ADD: Opcode = Opcode::new(107, 0);
const SET_UPDATE: Opcode = Opcode::new(109, 0);
const MAP_ADD: Opcode = Opcode::new(98, 0);
const MATCH_CLASS: Opcode = Opcode::new(99, 0);
const IMPORT_FROM: Opcode = Opcode::new(72, 0);
const IMPORT_NAME: Opcode = Opcode::new(73, 0);
const LOAD_COMMON_CONSTANT: Opcode = Opcode::new(81, 0);
const RAISE_VARARGS: Opcode = Opcode::new(104, 0);
const RERAISE: Opcode = Opcode::new(105, 0);
const SEND: Opcode = Opcode::new(106, 1);
const YIELD_VALUE: Opcode = Opcode::new(120, 0);
const CHECK_EXC_MATCH: Opcode = Opcode::new(6, 0);
const CHECK_EG_MATCH: Opcode = Opcode::new(5, 0);
const UNPACK_EX: Opcode = Opcode::new(118, 0);
const UNPACK_SEQUENCE: Opcode = Opcode::new(119, 1);
const FOR_ITER: Opcode = Opcode::new(70, 1);
const POP_JUMP_IF_FALSE: Opcode = Opcode::new(100, 1);
const POP_JUMP_IF_NONE: Opcode = Opcode::new(101, 1);
const POP_JUMP_IF_NOT_NONE: Opcode = Opcode::new(102, 1);
const POP_JUMP_IF_TRUE: Opcode = Opcode::new(103, 1);
const JUMP_FORWARD: Opcode = Opcode::new(77, 0);
const JUMP_BACKWARD: Opcode = Opcode::new(75, 1);
const JUMP_BACKWARD_NO_INTERRUPT: Opcode = Opcode::new(76, 0);

const CO_OPTIMIZED: u32 = 0x0001;
const CO_NEWLOCALS: u32 = 0x0002;
const CO_VARARGS: u32 = 0x0004;
const CO_VARKEYWORDS: u32 = 0x0008;
const CO_NESTED: u32 = 0x0010;
const CO_GENERATOR: u32 = 0x0020;
const CO_COROUTINE: u32 = 0x0080;
const CO_ASYNC_GENERATOR: u32 = 0x0200;
const CO_HAS_DOCSTRING: u32 = 0x0400_0000;
const CO_METHOD: u32 = 0x0800_0000;
const CO_FUTURE_BARRY_AS_BDFL: u32 = 0x0040_0000;
const CO_FUTURE_ANNOTATIONS: u32 = 0x0100_0000;
const CO_FUTURE_MASK: u32 = CO_FUTURE_BARRY_AS_BDFL | CO_FUTURE_ANNOTATIONS;
const CO_FAST_LOCAL: u8 = 0x20;
const CO_FAST_HIDDEN: u8 = 0x10;
const CO_FAST_CELL: u8 = 0x40;
const CO_FAST_FREE: u8 = 0x80;
const CO_FAST_ARG_POS: u8 = 0x02;
const CO_FAST_ARG_KW: u8 = 0x04;
const CO_FAST_ARG_VAR: u8 = 0x08;
const SHADOWED_SUPER_SENTINEL: &str = "\0shadowed super";

#[derive(Clone, Debug)]
pub(crate) enum Constant {
    None,
    Bool(bool),
    Ellipsis,
    Int(u64),
    SignedInt(i64),
    BigInt {
        negative: bool,
        digits: Vec<u16>,
    },
    Float(f64),
    Complex {
        real: f64,
        imag: f64,
    },
    String(String),
    SurrogateString(Vec<u8>),
    Bytes(Vec<u8>),
    Tuple(Vec<Self>),
    FrozenSet(Vec<Self>),
    Slice {
        lower: Box<Self>,
        upper: Box<Self>,
        step: Box<Self>,
    },
    Code(Box<CodeObject>),
}

#[derive(Clone, Debug)]
pub(crate) struct CodeObject {
    pub(crate) arg_count: u32,
    pub(crate) positional_only_arg_count: u32,
    pub(crate) keyword_only_arg_count: u32,
    pub(crate) stack_size: u32,
    pub(crate) flags: u32,
    pub(crate) bytecode: Vec<u8>,
    pub(crate) constants: Vec<Constant>,
    pub(crate) names: Vec<String>,
    pub(crate) locals: Vec<String>,
    pub(crate) local_kinds: Vec<u8>,
    pub(crate) filename: String,
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) first_line_number: u32,
    pub(crate) line_table: Vec<u8>,
    pub(crate) exception_table: Vec<u8>,
    pub(crate) annotation_thunk: bool,
    pub(crate) interned_constant_strings: HashSet<String>,
}

#[derive(Debug)]
enum Scope {
    Module,
    Class {
        globals: HashSet<String>,
        nonlocals: HashSet<String>,
        free_indices: HashMap<String, u32>,
    },
    Function {
        indices: HashMap<String, u32>,
        free_indices: HashMap<String, u32>,
        cells: HashSet<String>,
        globals: HashSet<String>,
    },
}

#[derive(Clone, Copy, Debug)]
struct LoopContext {
    continue_label: Label,
    break_label: Label,
    iterator_cleanup: IteratorCleanup,
    with_depth: usize,
    finally_end_depth: usize,
    exception_region_depth: usize,
    break_returns: bool,
    preserve_break_exit: bool,
}

#[derive(Clone, Debug)]
struct ExceptionHandlerContext {
    name: Option<String>,
    loop_depth: usize,
}

#[derive(Clone, Debug)]
struct ReturnFinallyContext {
    body: Vec<Stmt>,
    loop_depth: usize,
    handler: Label,
    depth: u32,
}

#[derive(Clone, Copy, Debug)]
struct WithExitContext {
    location: SourceLocation,
    is_async: bool,
}

#[derive(Debug)]
struct MatchContext {
    stores: Vec<String>,
    fail_pop: Vec<Label>,
    on_top: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IteratorCleanup {
    None,
    Sync,
    Async,
}

#[derive(Clone, Debug)]
struct DeferredComprehensionCleanup {
    label: Label,
    base_depth: i32,
    temporary_indices: Vec<u32>,
    location: SourceLocation,
    parent: Option<(Label, u32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionKind {
    List,
    Tuple,
    Set,
}

impl CollectionKind {
    const fn build_opcode(self) -> Opcode {
        match self {
            Self::List => BUILD_LIST,
            Self::Tuple => BUILD_TUPLE,
            Self::Set => BUILD_SET,
        }
    }

    const fn build_opcode_for_unpacking(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => BUILD_LIST,
            Self::Set => BUILD_SET,
        }
    }

    const fn append_opcode(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => LIST_APPEND,
            Self::Set => SET_ADD,
        }
    }

    const fn extend_opcode(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => LIST_EXTEND,
            Self::Set => SET_UPDATE,
        }
    }
}

#[derive(Clone, Debug)]
struct FunctionPlan {
    key: (u32, u32),
    locals: Vec<String>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
    references: HashSet<String>,
    annotation_references: HashSet<String>,
    cellvars: HashSet<String>,
    freevars: BTreeSet<String>,
    annotation_freevars: BTreeSet<String>,
    children: Vec<Self>,
}

impl FunctionPlan {
    fn analyze(definition: &StmtFunctionDef, future_annotations: bool) -> Self {
        let mut plan = Self::build(definition, future_annotations);
        for name in plan.resolve() {
            plan.mark_global(&name);
        }
        plan
    }

    fn analyze_in_class(
        definition: &StmtFunctionDef,
        future_annotations: bool,
        class_freevars: &HashSet<&str>,
    ) -> Self {
        let mut plan = Self::build(definition, future_annotations);
        for name in plan.resolve() {
            if name == "__class__" || class_freevars.contains(name.as_str()) {
                plan.freevars.insert(name);
            } else {
                plan.mark_global(&name);
            }
        }
        // Method annotation thunks are created by the class body, so they can close over the
        // class body's free variables even when the method body does not reference them.
        for name in &plan.annotation_references {
            if class_freevars.contains(name.as_str()) {
                plan.annotation_freevars.insert(name.clone());
            }
        }
        plan
    }

    fn build(definition: &StmtFunctionDef, future_annotations: bool) -> Self {
        let mut bindings = LocalCollector::default();
        bindings.collect_globals(&definition.body);
        bindings.collect_suite(&definition.body);
        let mut locals = LocalCollector {
            known_bindings: bindings.names.into_iter().collect(),
            ..LocalCollector::default()
        };
        locals.collect_globals(&definition.body);
        for parameter in definition
            .parameters
            .posonlyargs
            .iter()
            .chain(&definition.parameters.args)
            .chain(&definition.parameters.kwonlyargs)
        {
            locals.insert(parameter.name().as_str());
        }
        if let Some(parameter) = &definition.parameters.vararg {
            locals.insert(parameter.name.as_str());
        }
        if let Some(parameter) = &definition.parameters.kwarg {
            locals.insert(parameter.name.as_str());
        }
        locals.collect_suite(&definition.body);

        let mut reference_collector = ReferenceCollector {
            skip_annotations: true,
            ..ReferenceCollector::default()
        };
        for statement in &definition.body {
            reference_collector.visit_stmt(statement);
        }

        let mut annotation_collector = ReferenceCollector::default();
        if !future_annotations {
            for parameter in definition
                .parameters
                .posonlyargs
                .iter()
                .chain(&definition.parameters.args)
            {
                if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                    annotation_collector.visit_expr(annotation);
                }
            }
            if let Some(parameter) = &definition.parameters.vararg
                && let Some(annotation) = parameter.annotation.as_deref()
            {
                annotation_collector.visit_expr(annotation);
            }
            for parameter in &definition.parameters.kwonlyargs {
                if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                    annotation_collector.visit_expr(annotation);
                }
            }
            if let Some(parameter) = &definition.parameters.kwarg
                && let Some(annotation) = parameter.annotation.as_deref()
            {
                annotation_collector.visit_expr(annotation);
            }
            if let Some(annotation) = definition.returns.as_deref() {
                annotation_collector.visit_expr(annotation);
            }
        }

        let mut children = Vec::new();
        collect_nested_functions(&definition.body, &mut children, future_annotations);
        let local_names: HashSet<_> = locals.names.iter().cloned().collect();
        let class_requirements = nested_class_required_names(&definition.body, future_annotations);
        let lambda_requirements = nested_lambda_required_names_in_suite(&definition.body);
        let generator_requirements = nested_generator_required_names_in_suite(&definition.body);
        let nested_function_requirements = children
            .iter()
            .cloned()
            .flat_map(|mut child| child.resolve())
            .collect::<HashSet<_>>();
        locals.names.retain(|name| {
            !locals.annotation_only.contains(name)
                || reference_collector.references.contains(name)
                    && (name != "__class__" || reference_collector.explicit_dunder_class_reference)
                || class_requirements.contains(name)
                || lambda_requirements.contains(name)
                || generator_requirements.contains(name)
                || nested_function_requirements.contains(name)
        });
        reference_collector
            .references
            .extend(class_requirements.iter().cloned());
        reference_collector
            .references
            .extend(lambda_requirements.iter().cloned());
        reference_collector
            .references
            .extend(generator_requirements.iter().cloned());
        let mut cellvars = generator_named_targets_in_suite(&definition.body)
            .into_iter()
            .filter(|name| local_names.contains(name))
            .collect::<HashSet<_>>();
        cellvars.extend(
            class_requirements
                .into_iter()
                .filter(|name| local_names.contains(name)),
        );
        cellvars.extend(
            lambda_requirements
                .into_iter()
                .filter(|name| local_names.contains(name)),
        );
        cellvars.extend(
            generator_requirements
                .into_iter()
                .filter(|name| local_names.contains(name)),
        );
        Self {
            key: function_key(definition),
            locals: locals.names,
            globals: locals.globals,
            nonlocals: locals.nonlocals,
            references: reference_collector.references,
            annotation_references: annotation_collector.references,
            cellvars,
            freevars: BTreeSet::new(),
            annotation_freevars: BTreeSet::new(),
            children,
        }
    }

    fn resolve(&mut self) -> BTreeSet<String> {
        let local_names: HashSet<_> = self.locals.iter().map(String::as_str).collect();
        let mut needed: BTreeSet<_> = self
            .references
            .iter()
            .chain(&self.nonlocals)
            .filter(|name| !local_names.contains(name.as_str()) && !self.globals.contains(*name))
            .cloned()
            .collect();

        for child in &mut self.children {
            for name in child.resolve() {
                if local_names.contains(name.as_str()) {
                    child.freevars.insert(name.clone());
                    self.cellvars.insert(name);
                } else if self.globals.contains(&name) {
                    child.mark_global(&name);
                } else if !self.globals.contains(&name) {
                    child.freevars.insert(name.clone());
                    needed.insert(name);
                }
            }
            for name in &child.annotation_references {
                if local_names.contains(name.as_str()) {
                    child.annotation_freevars.insert(name.clone());
                    self.cellvars.insert(name.clone());
                } else if !self.globals.contains(name) {
                    child.annotation_freevars.insert(name.clone());
                    needed.insert(name.clone());
                }
            }
        }
        needed
    }

    fn mark_global(&mut self, name: &str) {
        self.annotation_freevars.remove(name);
        if self.locals.iter().any(|local| local == name) {
            return;
        }
        self.freevars.remove(name);
        self.globals.insert(name.to_string());
        for child in &mut self.children {
            child.annotation_freevars.remove(name);
            if child.freevars.contains(name) {
                child.mark_global(name);
            }
        }
    }

    fn child(&self, definition: &StmtFunctionDef) -> Option<Self> {
        let key = function_key(definition);
        self.children.iter().find(|child| child.key == key).cloned()
    }
}

#[derive(Debug)]
pub(crate) struct Compiler {
    assembler: Assembler,
    constants: Vec<Constant>,
    deferred_constants_before_return: Vec<(InstructionId, Constant)>,
    deferred_constants: Vec<(Option<InstructionId>, Constant)>,
    deferred_names: Vec<(InstructionId, String)>,
    names: Vec<String>,
    name_indices: HashMap<String, u32>,
    locals: Vec<String>,
    fast_local_count: usize,
    cell_names: HashSet<String>,
    free_names: Vec<String>,
    temporary_indices: HashMap<String, u32>,
    hidden_names: HashSet<String>,
    active_temporaries: HashSet<String>,
    initialized_locals: HashSet<String>,
    owned_load_locals: HashSet<String>,
    imported_scope_names: HashSet<String>,
    interned_constant_strings: HashSet<String>,
    type_parameter_names: BTreeSet<String>,
    generic_target_qualified_name: Option<String>,
    child_qualified_name_parent: Option<String>,
    class_scope_is_nested: bool,
    defer_async_comprehension_restore: bool,
    pending_comprehension_restores: Vec<(Vec<u32>, SourceLocation)>,
    active_comprehension_cleanups: Vec<(Label, u32)>,
    active_comprehension_region_exclusions: Vec<Vec<(Label, Label)>>,
    active_with_exits: Vec<WithExitContext>,
    active_with_region_exclusions: Vec<Vec<(Label, Label)>>,
    active_terminal_withs: usize,
    // CPython retains the empty end block when an async for ends a protected suite.
    emit_protected_async_for_end_nop: bool,
    active_exception_region_exclusions: Vec<Vec<(Label, Label)>>,
    active_exception_handlers: Vec<ExceptionHandlerContext>,
    active_return_finally_contexts: Vec<ReturnFinallyContext>,
    // CPython's normalized `NOT_TAKEN` can reuse a normal-finally CFG slot that still owns the
    // coroutine stop-iteration handler.
    active_normal_finally_bodies: usize,
    active_finally_end_blocks: usize,
    active_finally_try_bodies: usize,
    active_pass_finally_locations: Vec<SourceLocation>,
    active_overriding_finally_returns: usize,
    preserve_finally_break_exit_loop_range: Option<ruff_text_size::TextRange>,
    exclude_terminal_if_not_taken: bool,
    exclude_condition_not_taken_from_exception: bool,
    exclude_condition_not_taken_from_all_exception_regions: bool,
    exclude_loop_tail_not_taken_from_control_flow_regions: bool,
    unwind_exception_handlers_for_implicit_return: bool,
    prevent_try_exit_inlining: bool,
    deferred_comprehension_cleanups: Vec<DeferredComprehensionCleanup>,
    scope: Scope,
    function_plan: Option<FunctionPlan>,
    module_globals: HashSet<String>,
    module_annotation_index: u32,
    annotation_classdict_index: Option<u32>,
    generator_region_start: Option<Label>,
    generator_region_exclusions: Vec<(Label, Label)>,
    match_temporary_index: u32,
    emitted_fallthrough_return: bool,
    loops: Vec<LoopContext>,
    depth: i32,
    max_depth: u32,
    filename: String,
    source: Arc<str>,
    name: String,
    qualified_name: String,
    private_name: Option<String>,
    arg_count: u32,
    positional_only_arg_count: u32,
    keyword_only_arg_count: u32,
    flags: u32,
    first_line_number: u32,
    annotation_thunk: bool,
}

impl Compiler {
    pub(crate) fn module(filename: &str, source: &str) -> Self {
        Self {
            assembler: Assembler::default(),
            constants: Vec::new(),
            deferred_constants_before_return: Vec::new(),
            deferred_constants: Vec::new(),
            deferred_names: Vec::new(),
            names: Vec::new(),
            name_indices: HashMap::new(),
            locals: Vec::new(),
            fast_local_count: 0,
            cell_names: HashSet::new(),
            free_names: Vec::new(),
            temporary_indices: HashMap::new(),
            hidden_names: HashSet::new(),
            active_temporaries: HashSet::new(),
            initialized_locals: HashSet::new(),
            owned_load_locals: HashSet::new(),
            imported_scope_names: HashSet::new(),
            interned_constant_strings: HashSet::new(),
            type_parameter_names: BTreeSet::new(),
            generic_target_qualified_name: None,
            child_qualified_name_parent: None,
            class_scope_is_nested: false,
            defer_async_comprehension_restore: false,
            pending_comprehension_restores: Vec::new(),
            active_comprehension_cleanups: Vec::new(),
            active_comprehension_region_exclusions: Vec::new(),
            active_with_exits: Vec::new(),
            active_with_region_exclusions: Vec::new(),
            active_terminal_withs: 0,
            emit_protected_async_for_end_nop: false,
            active_exception_region_exclusions: Vec::new(),
            active_exception_handlers: Vec::new(),
            active_return_finally_contexts: Vec::new(),
            active_normal_finally_bodies: 0,
            active_finally_end_blocks: 0,
            active_finally_try_bodies: 0,
            active_pass_finally_locations: Vec::new(),
            active_overriding_finally_returns: 0,
            preserve_finally_break_exit_loop_range: None,
            exclude_terminal_if_not_taken: false,
            exclude_condition_not_taken_from_exception: false,
            exclude_condition_not_taken_from_all_exception_regions: false,
            exclude_loop_tail_not_taken_from_control_flow_regions: false,
            unwind_exception_handlers_for_implicit_return: false,
            prevent_try_exit_inlining: false,
            deferred_comprehension_cleanups: Vec::new(),
            scope: Scope::Module,
            function_plan: None,
            module_globals: HashSet::new(),
            module_annotation_index: 0,
            annotation_classdict_index: None,
            generator_region_start: None,
            generator_region_exclusions: Vec::new(),
            match_temporary_index: 0,
            emitted_fallthrough_return: false,
            loops: Vec::new(),
            depth: 0,
            max_depth: 0,
            filename: filename.to_string(),
            source: Arc::from(source),
            name: "<module>".to_string(),
            qualified_name: "<module>".to_string(),
            private_name: None,
            arg_count: 0,
            positional_only_arg_count: 0,
            keyword_only_arg_count: 0,
            flags: 0,
            first_line_number: 1,
            annotation_thunk: false,
        }
    }

    fn function(
        filename: &str,
        source: Arc<str>,
        name: &str,
        qualified_name: String,
        first_line_number: u32,
        plan: FunctionPlan,
        arg_count: u32,
        positional_only_arg_count: u32,
        keyword_only_arg_count: u32,
        parameter_flags: u32,
    ) -> Result<Self, CompileError> {
        let parameter_count = usize::try_from(
            arg_count
                + keyword_only_arg_count
                + u32::from(parameter_flags & CO_VARARGS != 0)
                + u32::from(parameter_flags & CO_VARKEYWORDS != 0),
        )
        .unwrap_or(usize::MAX);
        let mut locals: Vec<_> = plan
            .locals
            .iter()
            .enumerate()
            .filter(|(index, name)| *index < parameter_count || !plan.cellvars.contains(*name))
            .map(|(_, name)| name.clone())
            .collect();
        let fast_local_count = locals.len();
        let mut cell_only_locals: Vec<_> = plan
            .locals
            .iter()
            .enumerate()
            .filter(|(index, name)| *index >= parameter_count && plan.cellvars.contains(*name))
            .map(|(_, name)| name.clone())
            .collect();
        cell_only_locals.sort();
        locals.extend(cell_only_locals);
        let indices = locals
            .iter()
            .enumerate()
            .map(|(index, name)| Ok((name.clone(), to_u32(index, "local variable count")?)))
            .collect::<Result<HashMap<_, _>, CompileError>>()?;
        let local_count = locals.len();
        let free_indices = plan
            .freevars
            .iter()
            .enumerate()
            .map(|(index, name)| {
                Ok((
                    name.clone(),
                    to_u32(local_count + index, "free variable count")?,
                ))
            })
            .collect::<Result<HashMap<_, _>, CompileError>>()?;
        let initialized_locals = locals.iter().take(parameter_count).cloned().collect();
        locals.extend(plan.freevars.iter().cloned());

        Ok(Self {
            assembler: Assembler::default(),
            constants: Vec::new(),
            deferred_constants_before_return: Vec::new(),
            deferred_constants: Vec::new(),
            deferred_names: Vec::new(),
            names: Vec::new(),
            name_indices: HashMap::new(),
            locals,
            fast_local_count,
            cell_names: plan.cellvars.clone(),
            free_names: plan.freevars.iter().cloned().collect(),
            temporary_indices: HashMap::new(),
            hidden_names: HashSet::new(),
            active_temporaries: HashSet::new(),
            initialized_locals,
            owned_load_locals: HashSet::new(),
            imported_scope_names: HashSet::new(),
            interned_constant_strings: HashSet::new(),
            type_parameter_names: BTreeSet::new(),
            generic_target_qualified_name: None,
            child_qualified_name_parent: None,
            class_scope_is_nested: false,
            defer_async_comprehension_restore: false,
            pending_comprehension_restores: Vec::new(),
            active_comprehension_cleanups: Vec::new(),
            active_comprehension_region_exclusions: Vec::new(),
            active_with_exits: Vec::new(),
            active_with_region_exclusions: Vec::new(),
            active_terminal_withs: 0,
            emit_protected_async_for_end_nop: false,
            active_exception_region_exclusions: Vec::new(),
            active_exception_handlers: Vec::new(),
            active_return_finally_contexts: Vec::new(),
            active_normal_finally_bodies: 0,
            active_finally_end_blocks: 0,
            active_finally_try_bodies: 0,
            active_pass_finally_locations: Vec::new(),
            active_overriding_finally_returns: 0,
            preserve_finally_break_exit_loop_range: None,
            exclude_terminal_if_not_taken: false,
            exclude_condition_not_taken_from_exception: false,
            exclude_condition_not_taken_from_all_exception_regions: false,
            exclude_loop_tail_not_taken_from_control_flow_regions: false,
            unwind_exception_handlers_for_implicit_return: false,
            prevent_try_exit_inlining: false,
            deferred_comprehension_cleanups: Vec::new(),
            scope: Scope::Function {
                indices,
                free_indices,
                cells: plan.cellvars.clone(),
                globals: plan.globals.clone(),
            },
            function_plan: Some(plan),
            module_globals: HashSet::new(),
            module_annotation_index: 0,
            annotation_classdict_index: None,
            generator_region_start: None,
            generator_region_exclusions: Vec::new(),
            match_temporary_index: 0,
            emitted_fallthrough_return: false,
            loops: Vec::new(),
            depth: 0,
            max_depth: 0,
            filename: filename.to_string(),
            source,
            name: name.to_string(),
            qualified_name,
            private_name: None,
            arg_count,
            positional_only_arg_count,
            keyword_only_arg_count,
            flags: CO_OPTIMIZED | CO_NEWLOCALS | parameter_flags,
            first_line_number,
            annotation_thunk: false,
        })
    }

    fn class(
        filename: &str,
        source: Arc<str>,
        name: &str,
        qualified_name: String,
        first_line_number: u32,
        globals: HashSet<String>,
        nonlocals: HashSet<String>,
        freevars: BTreeSet<String>,
        needs_class_closure: bool,
        has_methods: bool,
        future_flags: u32,
    ) -> Self {
        let mut locals = Vec::new();
        let mut cell_names = HashSet::new();
        if needs_class_closure {
            locals.push("__class__".to_string());
            cell_names.insert("__class__".to_string());
        }
        if has_methods {
            locals.push("__classdict__".to_string());
            cell_names.insert("__classdict__".to_string());
        }
        let free_start = locals.len();
        let free_indices = freevars
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    name.clone(),
                    u32::try_from(free_start + index).unwrap_or(u32::MAX),
                )
            })
            .collect();
        let free_names: Vec<_> = freevars.into_iter().collect();
        locals.extend(free_names.iter().cloned());
        Self {
            assembler: Assembler::default(),
            constants: Vec::new(),
            deferred_constants_before_return: Vec::new(),
            deferred_constants: Vec::new(),
            deferred_names: Vec::new(),
            names: Vec::new(),
            name_indices: HashMap::new(),
            locals,
            fast_local_count: 0,
            cell_names,
            free_names,
            temporary_indices: HashMap::new(),
            hidden_names: HashSet::new(),
            active_temporaries: HashSet::new(),
            initialized_locals: HashSet::new(),
            owned_load_locals: HashSet::new(),
            imported_scope_names: HashSet::new(),
            interned_constant_strings: HashSet::new(),
            type_parameter_names: BTreeSet::new(),
            generic_target_qualified_name: None,
            child_qualified_name_parent: None,
            class_scope_is_nested: false,
            defer_async_comprehension_restore: false,
            pending_comprehension_restores: Vec::new(),
            active_comprehension_cleanups: Vec::new(),
            active_comprehension_region_exclusions: Vec::new(),
            active_with_exits: Vec::new(),
            active_with_region_exclusions: Vec::new(),
            active_terminal_withs: 0,
            emit_protected_async_for_end_nop: false,
            active_exception_region_exclusions: Vec::new(),
            active_exception_handlers: Vec::new(),
            active_return_finally_contexts: Vec::new(),
            active_normal_finally_bodies: 0,
            active_finally_end_blocks: 0,
            active_finally_try_bodies: 0,
            active_pass_finally_locations: Vec::new(),
            active_overriding_finally_returns: 0,
            preserve_finally_break_exit_loop_range: None,
            exclude_terminal_if_not_taken: false,
            exclude_condition_not_taken_from_exception: false,
            exclude_condition_not_taken_from_all_exception_regions: false,
            exclude_loop_tail_not_taken_from_control_flow_regions: false,
            unwind_exception_handlers_for_implicit_return: false,
            prevent_try_exit_inlining: false,
            deferred_comprehension_cleanups: Vec::new(),
            scope: Scope::Class {
                globals,
                nonlocals,
                free_indices,
            },
            function_plan: None,
            module_globals: HashSet::new(),
            module_annotation_index: 0,
            annotation_classdict_index: None,
            generator_region_start: None,
            generator_region_exclusions: Vec::new(),
            match_temporary_index: 0,
            emitted_fallthrough_return: false,
            loops: Vec::new(),
            depth: 0,
            max_depth: 0,
            filename: filename.to_string(),
            source,
            name: name.to_string(),
            qualified_name,
            private_name: Some(name.to_string()),
            arg_count: 0,
            positional_only_arg_count: 0,
            keyword_only_arg_count: 0,
            flags: future_flags,
            first_line_number,
            annotation_thunk: false,
        }
    }

    pub(crate) fn compile_module(mut self, body: &Suite) -> Result<CodeObject, CompileError> {
        self.module_globals = module_global_names(body);
        self.imported_scope_names = module_imported_names(body);
        let mut module_bindings = LocalCollector::default();
        module_bindings.collect_suite(body);
        for name in &module_bindings.comprehension_targets {
            let index = to_u32(self.locals.len(), "module local count")?;
            self.locals.push(name.clone());
            self.temporary_indices.insert(name.clone(), index);
            self.hidden_names.insert(name.clone());
        }
        self.fast_local_count = self.locals.len();
        if module_bindings.seen.contains("super") {
            // This set is propagated to every child compiler. An impossible
            // Python identifier keeps the module-shadowing fact alongside the
            // imported-name facts used by the same optimization checks.
            self.imported_scope_names
                .insert(SHADOWED_SUPER_SENTINEL.to_string());
        }
        self.flags |= future_feature_flags(body);
        let simple_module_annotations = has_simple_annotations(body);
        let module_annotations = has_annotations(body);
        let future_module_annotations =
            self.flags & CO_FUTURE_ANNOTATIONS != 0 && module_annotations;
        if module_annotations {
            let annotation_cell = to_u32(self.locals.len(), "module annotation cell index")?;
            self.locals.push("__conditional_annotations__".to_string());
            self.cell_names
                .insert("__conditional_annotations__".to_string());
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(MAKE_CELL, annotation_cell, 0)?;
        }
        self.assembler.set_location(SourceLocation::new(0, 1, 0, 0));
        self.emit(RESUME, 0, 0)?;
        if module_annotations {
            self.name_index("__conditional_annotations__")?;
            if let Some(statement) = body.first() {
                self.assembler
                    .set_location(self.source_location(statement.range()));
            }
            if !future_module_annotations {
                if simple_module_annotations {
                    let annotation_child = self.compile_module_annotations(body)?;
                    self.emit_deferred_constant_before_return(Constant::Code(Box::new(
                        annotation_child,
                    )))?;
                    self.emit(MAKE_FUNCTION, 0, 0)?;
                    self.emit_deferred_store_name("__annotate__")?;
                }
            }
            self.emit(BUILD_SET, 0, 1)?;
            self.store_name("__conditional_annotations__")?;
            if future_module_annotations {
                self.emit(SETUP_ANNOTATIONS, 0, 0)?;
            }
        }

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                let constant =
                    self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.emit(LOAD_CONST, constant, 1)?;
                self.store_name("__doc__")?;
                &body[1..]
            } else if module_annotations && fold_constant(&expression.value).is_some() {
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };

        let body_start = self.assembler.instruction_count();
        let add_implicit_return = !suite_terminates(body);
        self.compile_suite_inner(body, add_implicit_return)?;
        let add_implicit_return = add_implicit_return && !self.emitted_fallthrough_return;
        if add_implicit_return
            && body
                .last()
                .is_some_and(statement_uses_implicit_return_location)
        {
            self.assembler.set_location(
                self.source_location(implicit_return_range(&self, body.last().unwrap())),
            );
        } else if self.assembler.instruction_count() > body_start {
            self.assembler.set_location(SourceLocation::NONE);
        } else if let Some(statement) = body
            .last()
            .filter(|statement| !matches!(statement, Stmt::Global(_) | Stmt::Nonlocal(_)))
        {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
        self.finish_inner(add_implicit_return)
    }

    fn compile_function_body(mut self, body: &Suite) -> Result<CodeObject, CompileError> {
        self.emit_function_prologue()?;

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.flags |= CO_HAS_DOCSTRING;
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };

        let body_start = self.assembler.instruction_count();
        let add_implicit_return = !suite_terminates(body);
        self.compile_suite_inner(body, add_implicit_return)?;
        let add_implicit_return = add_implicit_return && !self.emitted_fallthrough_return;
        if add_implicit_return
            && body
                .last()
                .is_some_and(statement_uses_implicit_return_location)
        {
            self.assembler.set_location(
                self.source_location(implicit_return_range(&self, body.last().unwrap())),
            );
        } else if self.assembler.instruction_count() > body_start {
            self.assembler.set_location(SourceLocation::NONE);
        } else if let Some(statement @ Stmt::Pass(_)) = body.last() {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
        self.finish_inner(add_implicit_return)
    }

    fn compile_class_body(mut self, body: &Suite) -> Result<CodeObject, CompileError> {
        let has_classdict_cell = self.cell_names.contains("__classdict__");
        let has_class_cell = self.cell_names.contains("__class__");
        self.assembler.set_location(SourceLocation::NONE);
        if !self.free_names.is_empty() {
            self.emit(
                COPY_FREE_VARS,
                to_u32(self.free_names.len(), "class free variable count")?,
                0,
            )?;
        }
        let cell_indices = self
            .locals
            .iter()
            .take(self.locals.len() - self.free_names.len())
            .enumerate()
            .filter(|(_, name)| self.cell_names.contains(name.as_str()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for index in cell_indices {
            self.emit(MAKE_CELL, to_u32(index, "class cell index")?, 0)?;
        }
        let line = i32::try_from(self.first_line_number).unwrap_or(i32::MAX);
        self.assembler
            .set_location(SourceLocation::new(line, line, 0, 0));
        self.emit(RESUME, 0, 0)?;
        self.load_name("__name__")?;
        self.store_name("__module__")?;
        let qualified_name = self.add_constant(Constant::String(self.qualified_name.clone()))?;
        self.emit(LOAD_CONST, qualified_name, 1)?;
        self.store_name("__qualname__")?;
        if self.first_line_number <= u32::from(u8::MAX) {
            self.emit(LOAD_SMALL_INT, self.first_line_number, 1)?;
        } else {
            let first_line_number =
                self.add_constant(Constant::Int(u64::from(self.first_line_number)))?;
            self.emit(LOAD_CONST, first_line_number, 1)?;
        }
        self.store_name("__firstlineno__")?;
        if self.free_names.iter().any(|name| name == ".type_params") {
            self.load_name(".type_params")?;
            self.store_name("__type_params__")?;
        }
        if has_classdict_cell {
            self.emit(LOAD_LOCALS, 0, 1)?;
            let index = self
                .locals
                .iter()
                .position(|name| name == "__classdict__")
                .expect("classdict cell is present");
            self.emit(STORE_DEREF, to_u32(index, "classdict cell index")?, -1)?;
        }
        if self.flags & CO_FUTURE_ANNOTATIONS != 0 && has_simple_annotations(body) {
            self.emit(SETUP_ANNOTATIONS, 0, 0)?;
        }

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                let docstring =
                    self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.emit(LOAD_CONST, docstring, 1)?;
                self.store_name("__doc__")?;
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };
        let body_start = self.assembler.instruction_count();
        let mut footer_location = SourceLocation::NONE;
        self.compile_suite(body)?;
        if let Some(Stmt::Expr(expression)) = body.last()
            && fold_constant(&expression.value).is_some()
        {
            let range = if let Expr::FString(fstring) = expression.value.as_ref() {
                self.fstring_result_range(fstring)
            } else {
                expression.range
            };
            self.assembler.set_location(self.source_location(range));
            self.emit(NOP, 0, 0)?;
        } else if let Some(Stmt::If(statement)) = body.last()
            && matches!(statement.test.as_ref(), Expr::EllipsisLiteral(_))
            && let Some(Stmt::Expr(expression)) = statement.body.last()
            && fold_constant(&expression.value).is_some()
        {
            // CPython transfers the final folded expression's location to the class footer.
            footer_location = self.source_location(expression.range);
        } else if self.assembler.instruction_count() > body_start
            && let Some(location) = self.assembler.last_instruction_location()
        {
            self.assembler.set_location(location);
        } else if let Some(statement) = body.last() {
            if !matches!(
                statement,
                Stmt::AnnAssign(annotation) if annotation.simple && annotation.value.is_none()
            ) {
                self.assembler
                    .set_location(self.source_location(statement.range()));
                self.emit(NOP, 0, 0)?;
            }
        }

        if self.flags & CO_FUTURE_ANNOTATIONS == 0
            && let Some((annotation_child, closure_names)) = self.compile_class_annotations(body)?
        {
            let line = i32::try_from(self.first_line_number).unwrap_or(i32::MAX);
            self.assembler
                .set_location(SourceLocation::new(line, line, 0, 0));
            self.emit_closure_tuple(&closure_names)?;
            let annotation = self.add_constant(Constant::Code(Box::new(annotation_child)))?;
            self.emit(LOAD_CONST, annotation, 1)?;
            self.emit(MAKE_FUNCTION, 0, 0)?;
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
            self.store_name("__annotate_func__")?;
        }

        self.assembler.set_location(footer_location);
        let static_attributes = self.add_constant(Constant::Tuple(
            class_static_attributes(body)
                .into_iter()
                .map(Constant::String)
                .collect(),
        ))?;
        self.emit(LOAD_CONST, static_attributes, 1)?;
        self.store_name("__static_attributes__")?;
        if has_classdict_cell {
            let index = self
                .locals
                .iter()
                .position(|name| name == "__classdict__")
                .expect("classdict cell is present");
            self.emit(LOAD_FAST, to_u32(index, "classdict cell index")?, 1)?;
            self.store_name("__classdictcell__")?;
        }
        if has_class_cell {
            let index = self
                .locals
                .iter()
                .position(|name| name == "__class__")
                .expect("class cell is present");
            self.emit(LOAD_FAST, to_u32(index, "class cell index")?, 1)?;
            self.emit(COPY, 1, 1)?;
            self.store_name("__classcell__")?;
            self.emit(RETURN_VALUE, 0, -1)?;
            self.finish_inner(false)
        } else {
            self.finish()
        }
    }

    fn finish(self) -> Result<CodeObject, CompileError> {
        self.finish_inner(true)
    }

    fn finish_inner(mut self, add_implicit_return: bool) -> Result<CodeObject, CompileError> {
        for (instruction, constant) in std::mem::take(&mut self.deferred_constants_before_return) {
            let index = self.add_constant(constant)?;
            self.assembler.patch_argument(instruction, index);
        }
        if add_implicit_return {
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(RETURN_VALUE, 0, -1)?;
        }
        if self.constants.is_empty() && !(self.name == "<lambda>" && self.flags & CO_GENERATOR != 0)
        {
            self.add_constant(Constant::None)?;
        }
        let deferred_cleanups = std::mem::take(&mut self.deferred_comprehension_cleanups);
        for cleanup in deferred_cleanups {
            self.assembler.mark(cleanup.label);
            self.set_depth(
                cleanup.base_depth + i32::try_from(cleanup.temporary_indices.len()).unwrap() + 2,
            );
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(SWAP, 2, 0)?;
            self.emit(POP_TOP, 0, -1)?;
            self.assembler.set_location(cleanup.location);
            self.emit(
                SWAP,
                to_u32(
                    cleanup.temporary_indices.len() + 1,
                    "comprehension local count",
                )?,
                0,
            )?;
            for (position, index) in cleanup.temporary_indices.iter().rev().enumerate() {
                if position > 0 {
                    self.assembler.fusion_barrier();
                }
                self.emit(STORE_FAST, *index, -1)?;
            }
            self.emit(RERAISE, 0, -1)?;
            let cleanup_end = self.assembler.label();
            self.assembler.mark(cleanup_end);
            if let Some((parent, depth)) = cleanup.parent {
                self.assembler.add_exception_region(
                    cleanup.label,
                    cleanup_end,
                    parent,
                    depth,
                    false,
                );
            }
            self.set_depth(0);
        }
        if let Some(start) = self.generator_region_start {
            let handler = self.assembler.label();
            self.assembler.mark(handler);
            let mut region_start = start;
            for (exclusion_start, exclusion_end) in &self.generator_region_exclusions {
                self.assembler.add_exception_region(
                    region_start,
                    *exclusion_start,
                    handler,
                    0,
                    true,
                );
                region_start = *exclusion_end;
            }
            self.assembler
                .add_exception_region(region_start, handler, handler, 0, true);
            self.assembler.set_location(SourceLocation::NONE);
            self.set_depth(1);
            self.emit(CALL_INTRINSIC_1, 3, 0)?;
            self.emit(RERAISE, 1, -1)?;
        }

        if self.depth != 0 {
            return Err(CompileError::Internal(format!(
                "compiler finished with stack depth {}",
                self.depth
            )));
        }

        for (instruction, constant) in std::mem::take(&mut self.deferred_constants) {
            let index = self.add_constant(constant)?;
            if let Some(instruction) = instruction {
                self.assembler.patch_argument(instruction, index);
            }
        }
        for (instruction, name) in std::mem::take(&mut self.deferred_names) {
            let index = self.name_index(&name)?;
            self.assembler.patch_argument(instruction, index);
        }
        self.assembler.optimize_constant_pops();
        self.remove_unused_constants()?;

        let local_kinds = self
            .locals
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let positional_only = usize::try_from(self.positional_only_arg_count).unwrap();
                let positional = usize::try_from(self.arg_count).unwrap();
                let keyword_only = usize::try_from(self.keyword_only_arg_count).unwrap();
                let argument_kind = if index < positional_only {
                    CO_FAST_ARG_POS
                } else if index < positional {
                    CO_FAST_ARG_POS | CO_FAST_ARG_KW
                } else if index < positional + keyword_only {
                    CO_FAST_ARG_KW
                } else if self.flags & CO_VARARGS != 0 && index == positional + keyword_only {
                    CO_FAST_ARG_POS | CO_FAST_ARG_VAR
                } else if self.flags & CO_VARKEYWORDS != 0
                    && index
                        == positional + keyword_only + usize::from(self.flags & CO_VARARGS != 0)
                {
                    CO_FAST_ARG_KW | CO_FAST_ARG_VAR
                } else {
                    0
                };
                let storage_kind = if index >= self.locals.len() - self.free_names.len() {
                    CO_FAST_FREE
                } else if self.cell_names.contains(name) {
                    if index < self.fast_local_count {
                        CO_FAST_LOCAL | CO_FAST_CELL
                    } else {
                        CO_FAST_CELL
                    }
                } else {
                    CO_FAST_LOCAL
                };
                argument_kind
                    | storage_kind
                    | if self.hidden_names.contains(name) {
                        CO_FAST_HIDDEN
                    } else {
                        0
                    }
            })
            .collect();
        let output_locals = self
            .locals
            .iter()
            .map(|name| self.mangled_name(name))
            .collect();
        let parameter_count = usize::try_from(
            self.arg_count
                + self.keyword_only_arg_count
                + u32::from(self.flags & CO_VARARGS != 0)
                + u32::from(self.flags & CO_VARKEYWORDS != 0),
        )
        .unwrap_or(usize::MAX);
        let (bytecode, line_table, exception_table, assembled_max_depth, removed_max_depth) =
            self.assembler.finish_code(
                self.first_line_number,
                self.fast_local_count,
                parameter_count,
            )?;
        let max_depth =
            if removed_max_depth == Some(self.max_depth) && assembled_max_depth < self.max_depth {
                assembled_max_depth
            } else {
                self.max_depth
            };
        let stack_size = if self.flags & (CO_GENERATOR | CO_COROUTINE | CO_ASYNC_GENERATOR) != 0
            && self.generator_region_start.is_some()
        {
            max_depth.max(2)
        } else {
            max_depth.max(1)
        };
        Ok(CodeObject {
            arg_count: self.arg_count,
            positional_only_arg_count: self.positional_only_arg_count,
            keyword_only_arg_count: self.keyword_only_arg_count,
            stack_size,
            flags: self.flags,
            bytecode,
            constants: self.constants,
            names: self.names,
            locals: output_locals,
            local_kinds,
            filename: self.filename,
            name: self.name,
            qualified_name: self.qualified_name,
            first_line_number: self.first_line_number,
            line_table,
            exception_table,
            annotation_thunk: self.annotation_thunk,
            interned_constant_strings: self.interned_constant_strings,
        })
    }

    fn emit_function_prologue(&mut self) -> Result<(), CompileError> {
        self.assembler.set_location(SourceLocation::NONE);
        if !self.free_names.is_empty() {
            self.emit(
                COPY_FREE_VARS,
                to_u32(self.free_names.len(), "free variable count")?,
                0,
            )?;
        }
        let cells: Vec<_> = self
            .locals
            .iter()
            .take(self.locals.len() - self.free_names.len())
            .enumerate()
            .filter(|(_, name)| self.cell_names.contains(name.as_str()))
            .map(|(index, _)| index)
            .collect();
        for index in cells {
            self.emit(MAKE_CELL, to_u32(index, "cell variable index")?, 0)?;
        }
        if self.flags & (CO_GENERATOR | CO_COROUTINE | CO_ASYNC_GENERATOR) != 0 {
            let line = i32::try_from(self.first_line_number).unwrap_or(i32::MAX);
            self.assembler
                .set_location(SourceLocation::new(line, line, -1, -1));
            self.assembler.emit(RETURN_GENERATOR, 0);
            self.assembler.emit(POP_TOP, 0);
            let start = self.assembler.label();
            self.assembler.mark(start);
            self.generator_region_start = Some(start);
        }
        let line = i32::try_from(self.first_line_number).unwrap_or(i32::MAX);
        self.assembler
            .set_location(SourceLocation::new(line, line, 0, 0));
        self.emit(RESUME, 0, 0)
    }

    fn compile_suite(&mut self, body: &[Stmt]) -> Result<(), CompileError> {
        self.compile_suite_inner(body, false)
    }

    fn compile_suite_inner(
        &mut self,
        body: &[Stmt],
        final_expression_becomes_return: bool,
    ) -> Result<(), CompileError> {
        for (index, statement) in body.iter().enumerate() {
            let is_final = index + 1 == body.len();
            if is_final && final_expression_becomes_return {
                match statement {
                    Stmt::While(statement)
                        if statement.orelse.is_empty()
                            && !suite_contains_loop_break(&statement.body)
                            && matches!(statement.test.as_ref(), Expr::BoolOp(boolean) if boolean.op == BoolOp::And && boolean.values.len() > 1) =>
                    {
                        self.compile_terminal_while_and(statement)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::While(statement)
                        if statement.orelse.is_empty()
                            && suite_contains_loop_break(&statement.body) =>
                    {
                        self.compile_terminal_while_break(statement)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::For(statement)
                        if !statement.is_async
                            && statement.orelse.is_empty()
                            && matches!(statement.body.as_slice(), [Stmt::Break(_)]) =>
                    {
                        self.compile_terminal_for_break(statement)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::If(statement) => {
                        self.compile_terminal_if(statement)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::If(_)) => {
                        let Expr::If(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_if_expression(expression)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::Tuple(tuple) if matches!(tuple.elts.last(), Some(Expr::If(_)))) =>
                    {
                        let Expr::Tuple(tuple) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_tuple_if_expression(tuple)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Assign(assignment)
                        if matches!(assignment.value.as_ref(), Expr::If(_)) =>
                    {
                        let Expr::If(expression) = assignment.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_assignment_if_expression(assignment, expression)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::With(statement) if !statement.is_async => {
                        self.compile_with_items(&statement.items, &statement.body, true)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::With(statement) => {
                        self.compile_async_with_items(&statement.items, &statement.body, true)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Try(statement) => {
                        let previous = self.assembler.location();
                        self.assembler
                            .set_location(self.source_location(statement.range));
                        let result = self.compile_try_inner(statement, true, true, false);
                        self.assembler.set_location(previous);
                        result?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Match(statement) => {
                        self.compile_match_inner(statement, true)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression)
                        if matches!(expression.value.as_ref(), Expr::BoolOp(_)) =>
                    {
                        let Expr::BoolOp(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_bool_expression(expression)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::Compare(compare) if compare.ops.len() > 1) =>
                    {
                        let Expr::Compare(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_compare_expression(expression)?;
                        self.emitted_fallthrough_return = true;
                        continue;
                    }
                    _ => {}
                }
            }
            if let Stmt::Pass(statement) = statement
                && self.active_exception_region_exclusions.is_empty()
                && (self.active_with_region_exclusions.is_empty() || body.len() > 1)
            {
                self.assembler
                    .set_location(self.source_location(statement.range));
                let noop_start = self.assembler.label();
                self.assembler.mark(noop_start);
                self.emit(NOP, 0, 0)?;
                let noop_end = self.assembler.label();
                self.assembler.mark(noop_end);
                if is_final {
                    for exclusions in &mut self.active_with_region_exclusions {
                        exclusions.push((noop_start, noop_end));
                    }
                }
            } else if let Stmt::Expr(expression) = statement
                && fold_constant(&expression.value).is_some()
            {
                if literal_constant(&expression.value).is_none() {
                    self.record_folded_value(&expression.value)?;
                }
                let range = if let Expr::FString(fstring) = expression.value.as_ref() {
                    self.fstring_result_range(fstring)
                } else {
                    statement.range()
                };
                self.assembler.set_location(self.source_location(range));
                if index + 1 < body.len()
                    || (!final_expression_becomes_return
                        && !matches!(self.scope, Scope::Class { .. }))
                {
                    let noop_start = self.assembler.label();
                    self.assembler.mark(noop_start);
                    self.emit(NOP, 0, 0)?;
                    let noop_end = self.assembler.label();
                    self.assembler.mark(noop_end);
                    let final_statement = index + 1 == body.len();
                    if final_statement && self.generator_region_start.is_some() {
                        self.generator_region_exclusions
                            .push((noop_start, noop_end));
                    }
                    if final_statement {
                        for exclusions in &mut self.active_with_region_exclusions {
                            exclusions.push((noop_start, noop_end));
                        }
                        for exclusions in &mut self.active_exception_region_exclusions {
                            exclusions.push((noop_start, noop_end));
                        }
                    }
                }
            } else {
                let emit_protected_async_for_end_nop = is_final
                    && matches!(statement, Stmt::For(statement) if statement.is_async)
                    && (!self.active_with_region_exclusions.is_empty()
                        || !self.active_exception_region_exclusions.is_empty());
                let previous = std::mem::replace(
                    &mut self.emit_protected_async_for_end_nop,
                    emit_protected_async_for_end_nop,
                );
                let result = self.compile_statement(statement);
                self.emit_protected_async_for_end_nop = previous;
                result?;
            }
            if statement_terminates(statement) {
                let unreachable = &body[index + 1..];
                // CPython visits unreachable statements before the flow graph removes them.
                // Its constant compaction always retains slot zero, so the first literal in
                // an unreachable tail survives when no reachable constant preceded it.
                if self.constants.is_empty()
                    && let Some(constant) = first_suite_literal_constant(unreachable)
                {
                    self.add_constant(constant)?;
                }
                self.pre_register_suite_names(unreachable)?;
                break;
            }
        }
        Ok(())
    }

    fn emit_implicit_return(&mut self) -> Result<(), CompileError> {
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(RETURN_VALUE, 0, -1)
    }

    fn emit_deferred_implicit_return(&mut self) -> Result<(), CompileError> {
        let initialized_locals = self
            .unwind_exception_handlers_for_implicit_return
            .then(|| self.initialized_locals.clone());
        let exclusion_start = (self.unwind_exception_handlers_for_implicit_return
            && !self.active_exception_region_exclusions.is_empty())
        .then(|| {
            let start = self.assembler.label();
            self.assembler.mark(start);
            start
        });
        if self.unwind_exception_handlers_for_implicit_return {
            for handler in self.active_exception_handlers.clone().iter().rev() {
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &handler.name {
                    self.emit_deferred_constant_before_return(Constant::None)?;
                    self.store_name(name)?;
                    self.delete_name(name)?;
                }
            }
        }
        self.emit_deferred_constant_before_return(Constant::None)?;
        self.emit(RETURN_VALUE, 0, -1)?;
        if let Some(exclusion_start) = exclusion_start {
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
        }
        if let Some(initialized_locals) = initialized_locals {
            // The cleanup edge returned, so its deletes do not affect subsequently emitted cold
            // handler blocks.
            self.initialized_locals = initialized_locals;
        }
        Ok(())
    }

    fn compile_terminal_while_break(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let start = self.assembler.label();
        let condition_false = self.assembler.label();
        self.assembler.mark(start);
        if early_condition_truthiness(&statement.test) == Some(true) {
            if self.constants.is_empty()
                && let Some(constant) = fold_constant(&statement.test)
            {
                self.add_constant(constant)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
        } else {
            self.compile_jump_if(&statement.test, false, condition_false)?;
        }
        if self.constants.is_empty()
            && let Some(constant) = first_suite_literal_constant(&statement.body)
        {
            self.add_constant(constant)?;
        }
        self.loops.push(LoopContext {
            continue_label: start,
            break_label: condition_false,
            iterator_cleanup: IteratorCleanup::None,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: true,
            preserve_break_exit: false,
        });
        self.compile_suite(&statement.body)?;
        self.loops.pop();
        if !suite_terminates(&statement.body) {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(condition_false);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_while_and(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let Expr::BoolOp(condition) = statement.test.as_ref() else {
            unreachable!("terminal while-and requires a boolean condition");
        };
        let base_depth = self.depth;
        let start = self.assembler.label();
        self.assembler.mark(start);
        let exits = condition
            .values
            .iter()
            .map(|value| {
                let exit = self.assembler.label();
                self.compile_jump_if(value, false, exit)?;
                Ok((value, exit))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;

        self.loops.push(LoopContext {
            continue_label: start,
            break_label: exits[0].1,
            iterator_cleanup: IteratorCleanup::None,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: true,
            preserve_break_exit: false,
        });
        let body_start = self.assembler.instruction_count();
        self.compile_suite(&statement.body)?;
        self.loops.pop();
        if !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }

        for (value, exit) in exits.into_iter().rev() {
            self.assembler.mark(exit);
            self.set_depth(base_depth);
            let range = match value {
                Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => unary.operand.range(),
                _ => value.range(),
            };
            self.assembler.set_location(self.source_location(range));
            self.emit_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_for_break(
        &mut self,
        statement: &ruff_python_ast::StmtFor,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let cleanup = self.assembler.label();
        self.compile_expression(&statement.iter)?;
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(GET_ITER, 0, 0)?;
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&statement.target)?;
        self.assembler
            .set_location(self.source_location(statement.body[0].range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
    ) -> Result<(), CompileError> {
        if statement.elif_else_clauses.is_empty()
            && let Expr::Compare(comparison) = statement.test.as_ref()
            && comparison.ops.len() > 1
        {
            return self.compile_terminal_if_chained_compare(statement, comparison);
        }
        if let Some(truthiness) = early_condition_truthiness(&statement.test)
            && statement.elif_else_clauses.len() <= 1
            && statement
                .elif_else_clauses
                .first()
                .is_none_or(|clause| clause.test.is_none())
        {
            if let Some(constant) = fold_constant(&statement.test)
                && (self.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            if !truthiness && statement.elif_else_clauses.is_empty() {
                self.pre_register_suite_names(&statement.body)?;
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
                self.emit_deferred_implicit_return()?;
                return Ok(());
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            let body = if truthiness {
                &statement.body
            } else if let Some(clause) = statement.elif_else_clauses.first() {
                self.pre_register_suite_names(&statement.body)?;
                &clause.body
            } else {
                self.pre_register_suite_names(&statement.body)?;
                self.emit_deferred_implicit_return()?;
                return Ok(());
            };
            let emitted_fallthrough = if matches!(body.last(), Some(Stmt::If(_))) {
                let previous_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, false);
                self.compile_suite_inner(body, true)?;
                std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough)
            } else {
                self.compile_suite(body)?;
                false
            };
            if truthiness {
                for clause in &statement.elif_else_clauses {
                    self.pre_register_suite_names(&clause.body)?;
                }
            }
            if !emitted_fallthrough {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit_deferred_implicit_return()?;
            }
            return Ok(());
        }

        let base_depth = self.depth;
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if let Some(test) = test {
                let next = self.assembler.label();
                let exclude_not_taken = self.exclude_terminal_if_not_taken
                    || (!self.active_with_region_exclusions.is_empty()
                        && branch_index > 0
                        && suite_terminates(body));
                let previous_exclusion =
                    std::mem::replace(&mut self.exclude_terminal_if_not_taken, exclude_not_taken);
                let exclude_from_generator = self.flags & CO_COROUTINE != 0
                    && self.generator_region_start.is_some()
                    && self.assembler.contains_opcode(YIELD_VALUE)
                    && self.active_with_region_exclusions.is_empty()
                    && self.active_exception_region_exclusions.is_empty()
                    && self.active_normal_finally_bodies == 0;
                let previous_exception_exclusion = std::mem::replace(
                    &mut self.exclude_condition_not_taken_from_exception,
                    exclude_from_generator,
                );
                let condition_result = self.compile_jump_if(test, false, next);
                self.exclude_terminal_if_not_taken = previous_exclusion;
                self.exclude_condition_not_taken_from_exception = previous_exception_exclusion;
                condition_result?;
                self.mark_definitely_evaluated_locals(test);
                let body_start = self.assembler.instruction_count();
                let previous_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, false);
                self.compile_suite_inner(body, true)?;
                let emitted_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough);
                if !suite_terminates(body) && !emitted_fallthrough {
                    if let Some(statement) = body
                        .last()
                        .filter(|statement| statement_uses_implicit_return_location(statement))
                    {
                        self.assembler.set_location(
                            self.source_location(implicit_return_range(self, statement)),
                        );
                    } else if self.assembler.instruction_count() > body_start
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else if let Some(statement) = body.last() {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                    } else {
                        self.assembler
                            .set_location(self.source_location(test.range()));
                    }
                    self.emit_deferred_implicit_return()?;
                }
                self.assembler.mark(next);
                self.set_depth(base_depth);
                self.assembler
                    .set_location(self.source_location(test.range()));
            } else {
                let body_start = self.assembler.instruction_count();
                let previous_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, false);
                self.compile_suite_inner(body, true)?;
                let emitted_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough);
                if !suite_terminates(body) && !emitted_fallthrough {
                    if let Some(statement) = body
                        .last()
                        .filter(|statement| statement_uses_implicit_return_location(statement))
                    {
                        self.assembler.set_location(
                            self.source_location(implicit_return_range(self, statement)),
                        );
                    } else if self.assembler.instruction_count() > body_start
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else if let Some(statement) = body.last() {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                    }
                    self.emit_deferred_implicit_return()?;
                }
            }
        }
        if !has_else {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_deferred_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if_chained_compare(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        comparison: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let cleanup = self.assembler.label();
        let body = self.assembler.label();
        let condition_false = self.assembler.label();
        self.compile_expression(&comparison.left)?;
        for (operator, comparator) in comparison
            .ops
            .iter()
            .zip(&comparison.comparators)
            .take(comparison.ops.len() - 1)
        {
            self.compile_expression(comparator)?;
            self.assembler
                .set_location(self.source_location(comparison.range));
            self.emit(SWAP, 2, 0)?;
            self.emit(COPY, 2, 1)?;
            let (opcode, argument) = comparison_operator_boolean(*operator);
            self.emit(opcode, argument, -1)?;
            self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
        }
        self.compile_expression(comparison.comparators.last().unwrap())?;
        self.assembler
            .set_location(self.source_location(comparison.range));
        let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
        self.emit(opcode, argument, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, condition_false, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_forward(JUMP_FORWARD, body, 0)?;
        self.assembler.prevent_last_jump_inlining();

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 1);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_deferred_implicit_return()?;

        self.assembler.mark(body);
        self.set_depth(base_depth);
        self.compile_suite(&statement.body)?;
        if let Some(last) = statement.body.last() {
            self.assembler
                .set_location(self.source_location(last.range()));
        }
        self.emit_deferred_implicit_return()?;

        self.assembler.mark(condition_false);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit_deferred_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if_expression(
        &mut self,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(expression.body.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(otherwise);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        // CPython's conditional-expression join is a jump target. Its redundant-pair
        // optimizer therefore does not remove a constant else value with this POP_TOP.
        let end = self.assembler.label();
        self.assembler.mark(end);
        self.assembler
            .set_location(self.source_location(expression.orelse.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_tuple_if_expression(
        &mut self,
        tuple: &ruff_python_ast::ExprTuple,
    ) -> Result<(), CompileError> {
        let Some((Expr::If(expression), leading)) = tuple.elts.split_last() else {
            return Err(CompileError::Internal(
                "terminal tuple does not end in a conditional expression".to_string(),
            ));
        };
        let base_depth = self.depth;
        for element in leading {
            self.compile_expression(element)?;
        }
        let branch_depth = self.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(tuple.range));
        self.emit_build(BUILD_TUPLE, tuple.elts.len())?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(otherwise);
        self.set_depth(branch_depth);
        self.compile_expression(&expression.orelse)?;
        self.assembler
            .set_location(self.source_location(tuple.range));
        self.emit_build(BUILD_TUPLE, tuple.elts.len())?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_assignment_if_expression(
        &mut self,
        assignment: &ruff_python_ast::StmtAssign,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        self.compile_terminal_assignment_if_expression_inner(assignment, expression, false)
    }

    fn compile_terminal_assignment_if_expression_inner(
        &mut self,
        assignment: &ruff_python_ast::StmtAssign,
        expression: &ruff_python_ast::ExprIf,
        preserve_fallthrough_join_nop: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.pre_register_expression_names(&expression.body)?;
        self.pre_register_expression_names(&expression.orelse)?;
        if let Expr::If(body) = expression.body.as_ref() {
            // CPython copies the terminal assignment and return into the nested branches. The
            // nested expression's fallthrough join survives that copy as a source-position NOP.
            self.compile_terminal_assignment_if_expression_inner(assignment, body, true)?;
        } else {
            self.compile_expression(&expression.body)?;
            self.compile_assignment_targets(&assignment.targets)?;
            self.assembler
                .set_location(self.source_location(assignment.targets[0].range()));
            self.emit_deferred_implicit_return()?;
        }

        self.assembler.mark(otherwise);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        if preserve_fallthrough_join_nop {
            self.assembler
                .set_location(self.source_location(expression.orelse.range()));
            self.emit(NOP, 0, 0)?;
        }
        self.compile_assignment_targets(&assignment.targets)?;
        self.assembler
            .set_location(self.source_location(assignment.targets[0].range()));
        self.emit_deferred_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_assignment_targets(&mut self, targets: &[Expr]) -> Result<(), CompileError> {
        for (index, target) in targets.iter().enumerate() {
            if index + 1 < targets.len() {
                self.emit(COPY, 1, 1)?;
            }
            self.compile_store_target(target)?;
        }
        Ok(())
    }

    fn compile_terminal_bool_expression(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
    ) -> Result<(), CompileError> {
        let Some((last, leading)) = expression.values.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler
            .set_location(self.source_location(expression.range));
        let base_depth = self.depth;
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };

        let mut short_circuits = Vec::with_capacity(leading.len());
        for value in leading {
            let short_circuit = self.assembler.label();
            let continue_outer = self.assembler.label();
            short_circuits.push(short_circuit);
            if let Expr::BoolOp(inner) = value {
                self.compile_bool_expression_short_to(
                    inner,
                    if inner.op == expression.op {
                        short_circuit
                    } else {
                        continue_outer
                    },
                )?;
            } else {
                self.compile_expression(value)?;
            }
            self.assembler
                .set_location(self.source_location(expression.range));
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            if self.generator_region_start.is_some() {
                self.generator_region_exclusions
                    .push((exclusion_start, exclusion_end));
            }
            self.emit_jump_forward(jump, short_circuit, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler.mark(continue_outer);
            self.emit(POP_TOP, 0, -1)?;
        }

        if let Expr::BoolOp(last) = last {
            self.compile_terminal_bool_expression(last)?;
        } else {
            self.compile_expression(last)?;
            let result = self.assembler.label();
            self.assembler.preserve_block_boundary(result);
            self.assembler.mark(result);
            self.assembler
                .set_location(self.source_location(last.range()));
            self.emit(POP_TOP, 0, -1)?;
            self.emit_implicit_return()?;
        }

        for short_circuit in short_circuits.into_iter().rev() {
            self.assembler.mark(short_circuit);
            self.set_depth(base_depth + 1);
            self.assembler
                .set_location(self.source_location(expression.range));
            self.emit(POP_TOP, 0, -1)?;
            self.emit_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_bool_expression_short_to(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
        short_circuit: Label,
    ) -> Result<(), CompileError> {
        let Some((last, leading)) = expression.values.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler
            .set_location(self.source_location(expression.range));
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };
        for value in leading {
            self.compile_expression(value)?;
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            self.emit_jump_forward(jump, short_circuit, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.compile_expression(last)
    }

    fn compile_terminal_compare_expression(
        &mut self,
        expression: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let cleanup = self.assembler.label();
        self.assembler
            .set_location(self.source_location(expression.range));
        self.compile_expression(&expression.left)?;
        for (operator, comparator) in expression
            .ops
            .iter()
            .zip(&expression.comparators)
            .take(expression.ops.len() - 1)
        {
            self.compile_expression(comparator)?;
            self.emit(SWAP, 2, 0)?;
            self.emit(COPY, 2, 1)?;
            let (opcode, argument) = comparison_operator(*operator);
            self.emit(opcode, argument, -1)?;
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.compile_expression(expression.comparators.last().unwrap())?;
        let (opcode, argument) = comparison_operator(*expression.ops.last().unwrap());
        self.emit(opcode, argument, -1)?;
        self.assembler
            .set_location(self.source_location(expression.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_statement(&mut self, statement: &Stmt) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(statement.range()));
        let result = self.compile_statement_inner(statement);
        self.assembler.set_location(previous);
        result
    }

    fn compile_statement_inner(&mut self, statement: &Stmt) -> Result<(), CompileError> {
        let starting_depth = self.depth;
        match statement {
            Stmt::Assign(assignment) => {
                let unpacked = if let [target] = assignment.targets.as_slice()
                    && let Expr::Tuple(value) = assignment.value.as_ref()
                {
                    let targets = match target {
                        Expr::Tuple(target) => Some(target.elts.as_slice()),
                        Expr::List(target) => Some(target.elts.as_slice()),
                        _ => None,
                    };
                    targets.filter(|targets| {
                        targets.len() == value.elts.len()
                            && targets.len() <= 3
                            && !targets.iter().any(Expr::is_starred_expr)
                            && !value.elts.iter().any(Expr::is_starred_expr)
                    })
                } else {
                    None
                };
                if let Some(targets) = unpacked {
                    let Expr::Tuple(value) = assignment.value.as_ref() else {
                        unreachable!();
                    };
                    for value in &value.elts {
                        self.compile_expression(value)?;
                    }
                    if targets.len() > 1 {
                        self.assembler
                            .set_location(self.source_location(assignment.targets[0].range()));
                        self.emit(SWAP, to_u32(targets.len(), "unpack target count")?, 0)?;
                    }
                    for target in targets {
                        self.compile_store_target(target)?;
                    }
                } else {
                    let defer_restore =
                        expression_defers_async_comprehension_restore(&assignment.value);
                    let previous_defer = std::mem::replace(
                        &mut self.defer_async_comprehension_restore,
                        defer_restore,
                    );
                    let newly_owned = if assignment
                        .targets
                        .iter()
                        .any(|target| matches!(target, Expr::Name(_)))
                        && let Expr::Name(name) = assignment.value.as_ref()
                    {
                        self.owned_load_locals.insert(name.id.to_string())
                    } else {
                        false
                    };
                    self.compile_expression(&assignment.value)?;
                    if newly_owned && let Expr::Name(name) = assignment.value.as_ref() {
                        self.owned_load_locals.remove(name.id.as_str());
                    }
                    self.defer_async_comprehension_restore = previous_defer;
                    for (index, target) in assignment.targets.iter().enumerate() {
                        if index + 1 < assignment.targets.len() {
                            self.emit(COPY, 1, 1)?;
                        }
                        self.compile_store_target(target)?;
                    }
                    if defer_restore {
                        for target in &assignment.targets {
                            let mut names = Vec::new();
                            collect_target_names(target, &mut names);
                            self.owned_load_locals.extend(names);
                        }
                    }
                    self.emit_pending_comprehension_restores()?;
                }
            }
            Stmt::AugAssign(assignment) => {
                self.compile_augmented_assignment(assignment)?;
            }
            Stmt::AnnAssign(assignment) => self.compile_annotated_assignment(assignment)?,
            Stmt::Expr(expression) => {
                let discarded_comprehension = match expression.value.as_ref() {
                    Expr::ListComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            None,
                            &comprehension.elt,
                            BUILD_LIST,
                            LIST_APPEND,
                            true,
                        )?;
                        true
                    }
                    Expr::SetComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            None,
                            &comprehension.elt,
                            BUILD_SET,
                            SET_ADD,
                            true,
                        )?;
                        true
                    }
                    Expr::DictComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            comprehension.key.as_deref(),
                            &comprehension.value,
                            BUILD_MAP,
                            MAP_ADD,
                            true,
                        )?;
                        true
                    }
                    _ => {
                        self.compile_expression(&expression.value)?;
                        false
                    }
                };
                if discarded_comprehension {
                    return Ok(());
                }
                self.mark_definitely_evaluated_locals(&expression.value);
                let preserve_no_location = matches!(
                    expression.value.as_ref(),
                    Expr::BoolOp(_) | Expr::If(_)
                ) || matches!(expression.value.as_ref(), Expr::Compare(compare) if compare.ops.len() > 1);
                let optimized_generator_call = matches!(
                    expression.value.as_ref(),
                    Expr::Call(call) if optimized_generator_callable(call).is_some()
                );
                if preserve_no_location || optimized_generator_call {
                    self.assembler.set_location(SourceLocation::NONE);
                } else {
                    let discard_range = match expression.value.as_ref() {
                        Expr::Named(named) => named.target.range(),
                        Expr::Attribute(attribute) => self.attribute_opcode_range(attribute),
                        Expr::Call(call) => self.call_opcode_range(call),
                        Expr::FString(fstring) => self.fstring_result_range(fstring),
                        expression => expression.range(),
                    };
                    if matches!(expression.value.as_ref(), Expr::BinOp(binary) if optimized_percent_format(binary).is_some())
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else {
                        self.assembler
                            .set_location(self.source_location(discard_range));
                    }
                }
                self.emit(POP_TOP, 0, -1)?;
                if preserve_no_location {
                    self.assembler.preserve_last_no_location();
                }
            }
            Stmt::Pass(_) => {}
            Stmt::If(statement) => self.compile_if(statement)?,
            Stmt::While(statement) => self.compile_while(statement)?,
            Stmt::For(statement) if statement.is_async => self.compile_async_for(statement)?,
            Stmt::For(statement) => self.compile_for(statement)?,
            Stmt::Break(_) => {
                // Loop-control cleanup runs after CPython unwinds the active exception block.
                let unreachable_depth = self.depth;
                let context = self
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
                self.emit(NOP, 0, 0)?;
                let finally_end_unwind_start =
                    self.emit_finally_end_unwind(context.finally_end_depth, false)?;
                let exclusion_start = if self.active_exception_region_exclusions.len()
                    == context.exception_region_depth
                {
                    None
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                };
                self.emit_loop_control_exception_unwind(self.loops.len())?;
                let with_unwind_starts = self.emit_active_with_unwind(context.with_depth, false)?;
                match context.iterator_cleanup {
                    IteratorCleanup::None => {}
                    IteratorCleanup::Sync => self.emit(POP_TOP, 0, -1)?,
                    IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
                }
                if context.break_returns {
                    self.emit_implicit_return()?;
                } else {
                    let jump_exclusion_start = (self.active_exception_region_exclusions.len()
                        == context.exception_region_depth
                        && context.exception_region_depth > 0
                        && context.finally_end_depth == 0
                        && self.active_exception_handlers.is_empty())
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        start
                    });
                    self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
                    if context.preserve_break_exit {
                        // CPython inlines small exit blocks before removing a jump to the next
                        // block. Keep this terminal protected-loop break through that pass so
                        // the following finally return is copied onto the edge.
                        self.assembler.defer_last_jump_removal();
                        self.assembler.preserve_last_inlined_jump_nop();
                    }
                    if let Some(jump_exclusion_start) = jump_exclusion_start {
                        let jump_exclusion_end = self.assembler.label();
                        self.assembler.mark(jump_exclusion_end);
                        for exclusions in &mut self.active_exception_region_exclusions {
                            exclusions.push((jump_exclusion_start, jump_exclusion_end));
                        }
                    }
                }
                if exclusion_start.is_some() || !with_unwind_starts.is_empty() {
                    let exclusion_end = self.assembler.label();
                    self.assembler.mark(exclusion_end);
                    if let Some(exclusion_start) = exclusion_start {
                        for exclusions in self
                            .active_exception_region_exclusions
                            .iter_mut()
                            .skip(context.exception_region_depth)
                        {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    }
                    for (index, unwind_start) in with_unwind_starts {
                        self.active_with_region_exclusions[index]
                            .push((unwind_start, exclusion_end));
                    }
                }
                self.set_depth(unreachable_depth);
            }
            Stmt::Continue(_) => {
                // Loop-control cleanup runs after CPython unwinds the active exception block.
                let context =
                    self.loops.last().copied().ok_or_else(|| {
                        CompileError::Internal("continue outside loop".to_string())
                    })?;
                self.emit(NOP, 0, 0)?;
                let finally_end_unwind_start =
                    self.emit_finally_end_unwind(context.finally_end_depth, false)?;
                let exclusion_start = if self.active_exception_region_exclusions.len()
                    == context.exception_region_depth
                {
                    None
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                };
                self.emit_loop_control_exception_unwind(self.loops.len())?;
                let with_unwind_starts = self.emit_active_with_unwind(context.with_depth, false)?;
                self.emit_jump_backward(JUMP_BACKWARD, context.continue_label, 0)?;
                // CPython emits a temporary NOP before loop control, so an incoming jump sees
                // the NOP instead of threading through this backward jump.
                self.assembler.prevent_last_jump_threading_target();
                if exclusion_start.is_some() || !with_unwind_starts.is_empty() {
                    let exclusion_end = self.assembler.label();
                    self.assembler.mark(exclusion_end);
                    if let Some(exclusion_start) = exclusion_start {
                        for exclusions in self
                            .active_exception_region_exclusions
                            .iter_mut()
                            .skip(context.exception_region_depth)
                        {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    }
                    for (index, unwind_start) in with_unwind_starts {
                        self.active_with_region_exclusions[index]
                            .push((unwind_start, exclusion_end));
                    }
                }
                self.set_depth(starting_depth);
            }
            Stmt::Return(statement) => {
                if !matches!(self.scope, Scope::Function { .. }) {
                    return Err(unsupported("return outside a function"));
                }
                let return_is_overridden = self.active_overriding_finally_returns > 0;
                let preserve_tos = statement
                    .value
                    .as_deref()
                    .is_some_and(|value| !is_literal_constant(value));
                let unwinds_pass_finally = !self.active_pass_finally_locations.is_empty();
                let overriding_unwind_start = if return_is_overridden
                    && !preserve_tos
                    && !self.active_exception_region_exclusions.is_empty()
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                if !preserve_tos {
                    let range = statement
                        .value
                        .as_deref()
                        .map_or(statement.range, Ranged::range);
                    self.assembler.set_location(self.source_location(range));
                    self.emit(NOP, 0, 0)?;
                } else if let Some(value) = &statement.value {
                    self.compile_expression(value)?;
                }
                let pass_finally_edge_start = unwinds_pass_finally
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler
                            .mark_before_trailing_converted_pop_block(start)
                            .then_some(start)
                    })
                    .flatten();
                let nested_finally_region_unwind_start = (self.active_finally_end_blocks > 0
                    && self.active_exception_region_exclusions.len()
                        > self.active_finally_end_blocks)
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        (self.active_finally_end_blocks, start)
                    });
                let finally_end_unwind_start = self.emit_finally_end_unwind(0, preserve_tos)?;
                let delay_exception_unwind_start = overriding_unwind_start.is_none()
                    && !self.active_exception_region_exclusions.is_empty()
                    && self.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                    && preserve_tos
                    && !self.active_exception_handlers.is_empty();
                let mut exception_unwind_start = if overriding_unwind_start.is_some() {
                    overriding_unwind_start
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else if !self.active_exception_region_exclusions.is_empty()
                    && self.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                    && !delay_exception_unwind_start
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                let loops = self.loops.clone();
                let handlers = if !return_is_overridden
                    && self.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                {
                    self.active_exception_handlers.clone()
                } else {
                    Vec::new()
                };
                let mut next_loop = loops.len();
                for handler in handlers.iter().rev() {
                    for context in loops[handler.loop_depth..next_loop].iter().rev() {
                        match context.iterator_cleanup {
                            IteratorCleanup::None => {}
                            IteratorCleanup::Sync | IteratorCleanup::Async => {
                                if preserve_tos {
                                    self.emit(SWAP, 2, 0)?;
                                }
                                self.emit(POP_TOP, 0, -1)?;
                            }
                        }
                    }
                    if preserve_tos {
                        self.emit(SWAP, 2, 0)?;
                    }
                    if delay_exception_unwind_start && exception_unwind_start.is_none() {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        exception_unwind_start = Some(start);
                    }
                    let protected_pop_except = (!return_is_overridden
                        && !self.active_return_finally_contexts.is_empty())
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        start
                    });
                    self.emit(POP_EXCEPT, 0, -1)?;
                    if let Some(start) = protected_pop_except {
                        let end = self.assembler.label();
                        self.assembler.mark(end);
                        for context in &self.active_return_finally_contexts {
                            self.assembler.add_exception_region(
                                start,
                                end,
                                context.handler,
                                context.depth,
                                false,
                            );
                        }
                    }
                    if let Some(name) = &handler.name {
                        let none = self.add_constant(Constant::None)?;
                        self.emit(LOAD_CONST, none, 1)?;
                        self.store_name(name)?;
                        self.delete_name(name)?;
                    }
                    next_loop = handler.loop_depth;
                }
                let return_finally_contexts = if !return_is_overridden {
                    std::mem::take(&mut self.active_return_finally_contexts)
                } else {
                    Vec::new()
                };
                for finally_context in return_finally_contexts.iter().rev() {
                    for context in loops[finally_context.loop_depth..next_loop].iter().rev() {
                        match context.iterator_cleanup {
                            IteratorCleanup::None => {}
                            IteratorCleanup::Sync | IteratorCleanup::Async => {
                                if preserve_tos {
                                    self.emit(SWAP, 2, 0)?;
                                }
                                self.emit(POP_TOP, 0, -1)?;
                            }
                        }
                    }
                    let initialized_locals = self.initialized_locals.clone();
                    let result = self.compile_suite(&finally_context.body);
                    self.initialized_locals = initialized_locals;
                    result?;
                    if let Some(location) = self.assembler.last_instruction_location() {
                        self.assembler.set_location(location);
                    }
                    next_loop = finally_context.loop_depth;
                }
                if !return_finally_contexts.is_empty() {
                    self.active_return_finally_contexts = return_finally_contexts;
                }
                for context in loops[..next_loop].iter().rev() {
                    match context.iterator_cleanup {
                        IteratorCleanup::None => {}
                        IteratorCleanup::Sync | IteratorCleanup::Async => {
                            if preserve_tos {
                                self.emit(SWAP, 2, 0)?;
                            }
                            self.emit(POP_TOP, 0, -1)?;
                        }
                    }
                }
                let with_unwind_starts = self.emit_active_with_unwind(0, preserve_tos)?;
                let finally_unwind_start = if unwinds_pass_finally {
                    let start = if let Some(start) = pass_finally_edge_start {
                        start
                    } else {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        start
                    };
                    for location in self.active_pass_finally_locations.clone().into_iter().rev() {
                        self.assembler.set_location(location);
                        self.emit(NOP, 0, 0)?;
                    }
                    self.assembler.set_location(SourceLocation::NONE);
                    Some(start)
                } else {
                    None
                };
                if !preserve_tos && !return_is_overridden {
                    if let Some(value) = &statement.value {
                        if !with_unwind_starts.is_empty() || finally_unwind_start.is_some() {
                            let constant = fold_constant(value).ok_or_else(|| {
                                CompileError::Internal(
                                    "literal return value did not fold to a constant".to_string(),
                                )
                            })?;
                            self.emit_preprocessed_constant(value, constant)?;
                        } else if {
                            let location = self.source_location(statement.range);
                            location.line != location.end_line
                        } {
                            self.assembler
                                .set_location(self.source_location(statement.range));
                            let constant = fold_constant(value).ok_or_else(|| {
                                CompileError::Internal(
                                    "literal return value did not fold to a constant".to_string(),
                                )
                            })?;
                            self.emit_preprocessed_constant(value, constant)?;
                        } else {
                            self.assembler
                                .set_location(self.source_location(value.range()));
                            self.compile_expression(value)?;
                        }
                    } else {
                        let none = self.add_constant(Constant::None)?;
                        self.emit(LOAD_CONST, none, 1)?;
                    }
                }
                let return_exclusion_start = if exception_unwind_start.is_none()
                    && finally_unwind_start.is_none()
                    && !self.active_exception_region_exclusions.is_empty()
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                if return_is_overridden {
                    if preserve_tos {
                        self.assembler
                            .set_location(self.source_location(statement.range));
                        self.emit(POP_TOP, 0, -1)?;
                    }
                } else {
                    self.emit(RETURN_VALUE, 0, -1)?;
                }
                if exception_unwind_start.is_some() || return_exclusion_start.is_some() {
                    let return_exclusion_end = self.assembler.label();
                    self.assembler.mark(return_exclusion_end);
                    let exclusion_start = exception_unwind_start
                        .or(return_exclusion_start)
                        .expect("return exclusion has a start");
                    for (index, exclusions) in self
                        .active_exception_region_exclusions
                        .iter_mut()
                        .enumerate()
                    {
                        let exclusion_start = nested_finally_region_unwind_start
                            .filter(|(cutoff, _)| index >= *cutoff)
                            .map_or(exclusion_start, |(_, start)| start);
                        exclusions.push((exclusion_start, return_exclusion_end));
                    }
                }
                if !with_unwind_starts.is_empty() || finally_unwind_start.is_some() {
                    let unwind_end = self.assembler.label();
                    self.assembler.mark(unwind_end);
                    for (index, unwind_start) in with_unwind_starts {
                        self.active_with_region_exclusions[index].push((unwind_start, unwind_end));
                    }
                    if let Some(unwind_start) = finally_unwind_start {
                        for exclusions in &mut self.active_exception_region_exclusions {
                            exclusions.push((unwind_start, unwind_end));
                        }
                    }
                }
                self.set_depth(starting_depth);
            }
            Stmt::FunctionDef(definition) => self.compile_function_definition(definition)?,
            Stmt::ClassDef(definition) => self.compile_class_definition(definition)?,
            Stmt::Delete(statement) => {
                for target in &statement.targets {
                    self.compile_delete_target(target)?;
                }
            }
            Stmt::Assert(statement) => self.compile_assert(statement)?,
            Stmt::Raise(statement) => self.compile_raise(statement)?,
            Stmt::Import(statement) => self.compile_import(statement)?,
            Stmt::ImportFrom(statement) => self.compile_from_import(statement)?,
            Stmt::Global(_) | Stmt::Nonlocal(_) => {}
            Stmt::Try(statement) => self.compile_try(statement)?,
            Stmt::Match(statement) => self.compile_match(statement)?,
            Stmt::TypeAlias(statement) => self.compile_type_alias(statement)?,
            Stmt::With(statement) if statement.is_async => self.compile_async_with(statement)?,
            Stmt::With(statement) => self.compile_with(statement)?,
            Stmt::IpyEscapeCommand(_) => {
                return Err(unsupported(statement_name(statement)));
            }
        }

        if self.depth != starting_depth {
            return Err(CompileError::Internal(format!(
                "statement changed stack depth from {starting_depth} to {}",
                self.depth
            )));
        }
        Ok(())
    }

    fn compile_if(&mut self, statement: &ruff_python_ast::StmtIf) -> Result<(), CompileError> {
        if let Some(truthiness) = early_condition_truthiness(&statement.test)
            && statement.elif_else_clauses.len() <= 1
            && statement
                .elif_else_clauses
                .first()
                .is_none_or(|clause| clause.test.is_none())
        {
            if let Some(constant) = fold_constant(&statement.test) {
                self.add_constant(constant)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            if truthiness {
                let body_start = self.assembler.instruction_count();
                self.compile_suite(&statement.body)?;
                if self.assembler.instruction_count() == body_start
                    && let Some(Stmt::Pass(statement)) = statement.body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
                for clause in &statement.elif_else_clauses {
                    self.pre_register_suite_names(&clause.body)?;
                }
            } else {
                self.pre_register_suite_names(&statement.body)?;
                self.assembler.disable_load_fast_borrowing();
                if let Some(clause) = statement.elif_else_clauses.first() {
                    let body_start = self.assembler.instruction_count();
                    self.compile_suite(&clause.body)?;
                    if self.assembler.instruction_count() == body_start
                        && let Some(Stmt::Pass(statement)) = clause.body.last()
                    {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                        self.emit(NOP, 0, 0)?;
                    }
                }
            }
            return Ok(());
        }

        let base_depth = self.depth;
        let end = self.assembler.label();
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );

        let branch_count = branches.len();
        let has_terminating_else = branches
            .last()
            .is_some_and(|(test, body)| test.is_none() && suite_terminates(body));
        // The remaining branches still compile for symbol and constant-table side effects, but
        // their exception handlers do not contribute to CPython's stack size.
        let first_branch_is_always_true = jump_constant_truthiness(&statement.test) == Some(true);
        let mut first_branch_max_depth = None;
        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if let Some(test) = test {
                let next = self.assembler.label();
                let exclude_not_taken = self.exclude_terminal_if_not_taken
                    || (!self.active_with_region_exclusions.is_empty()
                        && branch_index > 0
                        && suite_terminates(body));
                let previous_exclusion =
                    std::mem::replace(&mut self.exclude_terminal_if_not_taken, exclude_not_taken);
                let retain_folded_test_in_protected_region = branch_index == 0
                    && early_condition_truthiness(test) == Some(true)
                    && (!self.active_with_region_exclusions.is_empty()
                        || !self.active_exception_region_exclusions.is_empty()
                        || self.generator_region_start.is_some());
                let condition_result = self.compile_jump_if(test, false, next);
                self.exclude_terminal_if_not_taken = previous_exclusion;
                condition_result?;
                if retain_folded_test_in_protected_region {
                    // CPython keeps the first folded branch marker inside the surrounding
                    // protected region instead of treating it as an artificial condition NOP.
                    if self.generator_region_start.is_some() {
                        self.generator_region_exclusions.pop();
                    }
                    for exclusions in &mut self.active_with_region_exclusions {
                        exclusions.pop();
                    }
                    for exclusions in &mut self.active_exception_region_exclusions {
                        exclusions.pop();
                    }
                }
                self.mark_definitely_evaluated_locals(test);
                let body_start = self.assembler.instruction_count();
                self.compile_with_strict_owned_loads(matches!(test, Expr::If(_)), |compiler| {
                    compiler.compile_suite(body)
                })?;
                let pass_only_body = matches!(body, [Stmt::Pass(_)]);
                if self.assembler.instruction_count() == body_start
                    && let Some(statement) = body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
                if branch_index == 0 && first_branch_is_always_true {
                    first_branch_max_depth = Some(self.max_depth);
                }
                if !suite_terminates(body) && branch_index + 1 < branch_count {
                    let nested_if_body_falls_through = matches!(
                        body.last(),
                        Some(Stmt::If(statement))
                            if early_condition_truthiness(&statement.test).is_none()
                                && !suite_terminates(&statement.body)
                    );
                    let nested_if_ends_in_break = matches!(
                        body.last(),
                        Some(Stmt::If(statement))
                            if matches!(statement.body.last(), Some(Stmt::Break(_)))
                    );
                    if nested_if_ends_in_break && has_terminating_else {
                        let Some(Stmt::If(statement)) = body.last() else {
                            unreachable!();
                        };
                        self.assembler
                            .set_location(self.source_location(statement.test.range()));
                    } else if nested_if_body_falls_through {
                        self.assembler.set_location(SourceLocation::NONE);
                    } else {
                        self.set_branch_end_location(body, body_start);
                    }
                    self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                    if nested_if_ends_in_break && has_terminating_else {
                        self.assembler.preserve_last_inlined_jump_nop();
                    } else if nested_if_body_falls_through {
                        self.assembler.preserve_last_no_location();
                    } else if pass_only_body {
                        self.assembler.preserve_last_direct_inlined_jump_nop();
                    } else if matches!(body.last(), Some(Stmt::If(_))) {
                        // CPython emits this jump in the nested if's empty join block. When
                        // small-exit inlining replaces it with a NOP, no preceding instruction
                        // in that block can make the NOP redundant.
                        self.assembler.preserve_last_direct_inlined_jump_nop();
                    }
                }
                self.assembler.mark(next);
                self.set_depth(base_depth);
            } else {
                let body_start = self.assembler.instruction_count();
                self.compile_suite(body)?;
                if branch_index + 1 == branch_count
                    && self.assembler.instruction_count() == body_start
                    && let Some(statement) = body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
            }
        }

        self.assembler.mark(end);
        self.set_depth(base_depth);
        if let Some(max_depth) = first_branch_max_depth {
            self.max_depth = max_depth;
        }
        Ok(())
    }

    fn set_branch_end_location(&mut self, body: &[Stmt], body_start: usize) {
        if let Some(Stmt::If(statement)) = body.last()
            && let Some(test) = terminating_if_fallthrough_test(statement)
        {
            self.assembler
                .set_location(self.source_location(test.range()));
        } else if self.assembler.instruction_count() > body_start
            && let Some(location) = self.assembler.last_instruction_location()
        {
            self.assembler.set_location(location);
        } else if let Some(statement) = body.last() {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
    }

    fn compile_with_strict_owned_loads<T>(
        &mut self,
        strict: bool,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        let previous = strict.then(|| self.assembler.set_strict_owned_loads(true));
        let result = compile(self);
        if let Some(previous) = previous {
            self.assembler.set_strict_owned_loads(previous);
        }
        result
    }

    fn compile_loop_tail_suite(
        &mut self,
        body: &[Stmt],
        restart: Label,
    ) -> Result<bool, CompileError> {
        match body.split_last() {
            Some((Stmt::If(statement), leading)) => {
                self.compile_suite(leading)?;
                self.compile_loop_tail_if(statement, restart)?;
                Ok(true)
            }
            Some((Stmt::While(statement), leading))
                if statement.orelse.is_empty()
                    && early_condition_truthiness(&statement.test).is_none()
                    && !suite_contains_loop_break(&statement.body)
                    && !matches!(
                        statement.body.last(),
                        Some(Stmt::Try(statement)) if try_requires_loop_tail_inlining(statement)
                    ) =>
            {
                self.compile_suite(leading)?;
                self.compile_loop_tail_while(statement, restart)?;
                Ok(true)
            }
            Some((Stmt::Assert(statement), leading))
                if early_condition_truthiness(&statement.test).is_none() =>
            {
                self.compile_suite(leading)?;
                self.compile_loop_tail_assert(statement, restart)?;
                Ok(true)
            }
            _ => {
                self.compile_suite(body)?;
                Ok(false)
            }
        }
    }

    fn add_control_flow_region_exclusion(&mut self, start: Label, end: Label) {
        if self.generator_region_start.is_some() {
            self.generator_region_exclusions.push((start, end));
        }
        for exclusions in &mut self.active_with_region_exclusions {
            exclusions.push((start, end));
        }
        for exclusions in &mut self.active_exception_region_exclusions {
            exclusions.push((start, end));
        }
    }

    fn has_active_control_flow_region(&self) -> bool {
        self.generator_region_start.is_some()
            || !self.active_with_region_exclusions.is_empty()
            || !self.active_exception_region_exclusions.is_empty()
    }

    fn emit_active_with_unwind(
        &mut self,
        from_depth: usize,
        preserve_tos: bool,
    ) -> Result<Vec<(usize, Label)>, CompileError> {
        let contexts = self.active_with_exits[from_depth..].to_vec();
        let mut unwind_starts = Vec::with_capacity(contexts.len());
        for (offset, context) in contexts.into_iter().enumerate().rev() {
            let index = from_depth + offset;
            let start = self.assembler.label();
            self.assembler.mark(start);
            unwind_starts.push((index, start));
            self.assembler.set_location(context.location);
            if preserve_tos {
                self.emit(SWAP, 3, 0)?;
                self.emit(SWAP, 2, 0)?;
            }
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(CALL, 3, -4)?;
            if context.is_async {
                self.compile_awaitable_on_stack(2)?;
            }
            self.emit(POP_TOP, 0, -1)?;
        }
        Ok(unwind_starts)
    }

    fn emit_loop_control_exception_unwind(
        &mut self,
        loop_depth: usize,
    ) -> Result<(), CompileError> {
        let handlers = self
            .active_exception_handlers
            .iter()
            .rev()
            .take_while(|handler| handler.loop_depth >= loop_depth)
            .cloned()
            .collect::<Vec<_>>();
        for handler in handlers {
            self.emit(POP_EXCEPT, 0, -1)?;
            if let Some(name) = &handler.name {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name)?;
                self.delete_name(name)?;
            }
        }
        Ok(())
    }

    fn emit_finally_end_unwind(
        &mut self,
        from_depth: usize,
        preserve_tos: bool,
    ) -> Result<Option<Label>, CompileError> {
        let mut unwind_start = None;
        for _ in from_depth..self.active_finally_end_blocks {
            if preserve_tos {
                self.emit(SWAP, 2, 0)?;
            }
            self.emit(POP_TOP, 0, -1)?;
            if preserve_tos {
                self.emit(SWAP, 2, 0)?;
            }
            if unwind_start.is_none() {
                let start = self.assembler.label();
                self.assembler.mark(start);
                unwind_start = Some(start);
            }
            self.emit(POP_EXCEPT, 0, -1)?;
        }
        Ok(unwind_start)
    }

    fn compile_loop_tail_while(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let start = self.assembler.label();
        let body_label = self.assembler.label();
        let control_region = self
            .has_active_control_flow_region()
            .then(|| (self.assembler.label(), self.assembler.label()));

        self.assembler.mark(start);
        self.compile_jump_if(&statement.test, true, body_label)?;
        if let Some((control_start, _)) = control_region {
            self.assembler
                .mark_before_trailing_instructions(control_start, 2);
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if let Some((control_start, control_end)) = control_region {
            self.assembler.mark(control_end);
            self.add_control_flow_region_exclusion(control_start, control_end);
        }

        self.assembler.mark(body_label);
        self.set_depth(base_depth);
        self.loops.push(LoopContext {
            continue_label: start,
            break_label: restart,
            iterator_cleanup: IteratorCleanup::None,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: false,
            preserve_break_exit: false,
        });
        let body_start = self.assembler.instruction_count();
        let previous_loop_tail_exclusion = std::mem::replace(
            &mut self.exclude_loop_tail_not_taken_from_control_flow_regions,
            control_region.is_some(),
        );
        let tail_result = self.compile_while_tail_suite(&statement.body, start);
        self.exclude_loop_tail_not_taken_from_control_flow_regions = previous_loop_tail_exclusion;
        let tail_jumps = tail_result?;
        self.loops.pop();
        if !tail_jumps && !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }
        Ok(())
    }

    fn compile_while_tail_suite(
        &mut self,
        body: &[Stmt],
        restart: Label,
    ) -> Result<bool, CompileError> {
        if let Some((Stmt::Try(statement), leading)) = body.split_last()
            && try_requires_loop_tail_inlining(statement)
        {
            self.compile_suite(leading)?;
            self.compile_try_inner(statement, false, true, true)?;
            Ok(false)
        } else {
            self.compile_loop_tail_suite(body, restart)
        }
    }

    fn emit_while_backedge(
        &mut self,
        body: &[Stmt],
        body_start: usize,
        restart: Label,
    ) -> Result<(), CompileError> {
        let inline_try_tail = matches!(
            body.last(),
            Some(Stmt::Try(statement)) if try_requires_loop_tail_inlining(statement)
        );
        if !inline_try_tail {
            self.set_branch_end_location(body, body_start);
        }
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if inline_try_tail {
            self.assembler.prepare_last_no_location_block_for_inlining();
        }
        Ok(())
    }

    fn compile_loop_tail_assert(
        &mut self,
        statement: &ruff_python_ast::StmtAssert,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let failure = self.assembler.label();
        let previous_exclusion = std::mem::replace(
            &mut self.exclude_terminal_if_not_taken,
            !self.active_with_region_exclusions.is_empty(),
        );
        if let Expr::BoolOp(boolean) = statement.test.as_ref()
            && boolean.op == BoolOp::Or
            && let Some((last, leading)) = boolean.values.split_last()
        {
            for value in leading {
                let next = self.assembler.label();
                self.compile_jump_if(value, false, next)?;
                self.assembler
                    .set_location(self.source_location(value.range()));
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                self.assembler.mark(next);
            }
            self.compile_jump_if(last, false, failure)?;
            self.assembler
                .set_location(self.source_location(last.range()));
        } else {
            self.compile_jump_if(&statement.test, false, failure)?;
            self.assembler
                .set_location(self.source_location(statement.test.range()));
        }
        self.exclude_terminal_if_not_taken = previous_exclusion;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(failure);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(LOAD_COMMON_CONSTANT, 0, 1)?;
        if let Some(message) = &statement.msg {
            self.compile_expression(message)?;
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit(RAISE_VARARGS, 1, -1)?;
        Ok(())
    }

    fn compile_loop_tail_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        if statement.elif_else_clauses.is_empty() {
            if self.generator_region_start.is_none()
                && let Expr::Compare(comparison) = statement.test.as_ref()
                && comparison.ops.len() > 1
            {
                return self.compile_loop_tail_chained_compare(statement, comparison, restart);
            }
            let body = statement.body.as_slice();
            let body_label = self.assembler.label();
            self.compile_loop_tail_condition(&statement.test, true, body_label, restart)?;
            self.assembler.mark(body_label);
            self.set_depth(base_depth);
            let body_start = self.assembler.instruction_count();
            let nested_tail = self.compile_with_strict_owned_loads(
                matches!(statement.test.as_ref(), Expr::If(_)),
                |compiler| compiler.compile_loop_tail_suite(body, restart),
            )?;
            if !nested_tail && !suite_terminates(body) {
                self.set_branch_end_location(body, body_start);
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            }
            return Ok(());
        }
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let branch_count = branches.len();
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if !has_else && branch_index + 1 == branch_count {
                let test = test.expect("last branch without else has a test");
                let body_label = self.assembler.label();
                let control_region = (early_condition_truthiness(test).is_none()
                    && self.has_active_control_flow_region())
                .then(|| (self.assembler.label(), self.assembler.label()));
                let generator_exclusion = (control_region.is_none()
                    && self.generator_region_start.is_some())
                .then(|| self.assembler.label());
                self.compile_jump_if(test, true, body_label)?;
                if let Some((control_start, _)) = control_region {
                    self.assembler
                        .mark_before_trailing_instructions(control_start, 2);
                }
                if let Some(exclusion_start) = generator_exclusion {
                    self.assembler
                        .mark_before_trailing_instructions(exclusion_start, 2);
                }
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                if let Some((control_start, control_end)) = control_region {
                    self.assembler.mark(control_end);
                    self.add_control_flow_region_exclusion(control_start, control_end);
                }
                self.assembler.mark(body_label);
                if let Some(exclusion_start) = generator_exclusion {
                    self.generator_region_exclusions
                        .push((exclusion_start, body_label));
                }
                self.set_depth(base_depth);
                let body_start = self.assembler.instruction_count();
                let nested_tail = self
                    .compile_with_strict_owned_loads(matches!(test, Expr::If(_)), |compiler| {
                        compiler.compile_loop_tail_suite(body, restart)
                    })?;
                if !nested_tail && !suite_terminates(body) {
                    self.set_branch_end_location(body, body_start);
                    self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                }
                return Ok(());
            }
            let next = test.map(|_| self.assembler.label());
            if let (Some(test), Some(next)) = (test, next) {
                let exclude_from_control_flow_regions =
                    !self.active_exception_region_exclusions.is_empty()
                        || self.exclude_loop_tail_not_taken_from_control_flow_regions;
                let previous_exception_exclusion = std::mem::replace(
                    &mut self.exclude_condition_not_taken_from_exception,
                    exclude_from_control_flow_regions,
                );
                let previous_all_exception_exclusions = std::mem::replace(
                    &mut self.exclude_condition_not_taken_from_all_exception_regions,
                    !self.active_exception_region_exclusions.is_empty(),
                );
                let condition_result = self.compile_jump_if(test, false, next);
                self.exclude_condition_not_taken_from_exception = previous_exception_exclusion;
                self.exclude_condition_not_taken_from_all_exception_regions =
                    previous_all_exception_exclusions;
                condition_result?;
            }
            let nested_tail = self.compile_with_strict_owned_loads(
                test.is_some_and(|test| matches!(test, Expr::If(_))),
                |compiler| compiler.compile_loop_tail_suite(body, restart),
            )?;
            if !nested_tail && !suite_terminates(body) {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                } else if let Some(statement) = body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            }
            if let Some(next) = next {
                self.assembler.mark(next);
                self.set_depth(base_depth);
            }
        }
        if !has_else {
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        }
        Ok(())
    }

    fn compile_loop_tail_condition(
        &mut self,
        expression: &Expr,
        jump_on: bool,
        success: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if let Expr::UnaryOp(unary) = expression
            && unary.op == UnaryOp::Not
        {
            return self.compile_loop_tail_condition(&unary.operand, !jump_on, success, restart);
        }
        if let Expr::BoolOp(boolean) = expression {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            let short_circuit_value = boolean.op == BoolOp::Or;
            if short_circuit_value == jump_on {
                for value in leading {
                    self.compile_jump_if(value, jump_on, success)?;
                }
            } else {
                for value in leading {
                    let next = self.assembler.label();
                    self.compile_loop_tail_condition(value, !short_circuit_value, next, restart)?;
                    self.assembler.mark(next);
                }
            }
            return self.compile_loop_tail_condition(last, jump_on, success, restart);
        }

        let control_region = (early_condition_truthiness(expression).is_none()
            && self.has_active_control_flow_region())
        .then(|| (self.assembler.label(), self.assembler.label()));
        let generator_exclusion = (control_region.is_none()
            && self.generator_region_start.is_some())
        .then(|| self.assembler.label());
        self.compile_jump_if(expression, jump_on, success)?;
        if let Some((control_start, _)) = control_region {
            self.assembler
                .mark_before_trailing_instructions(control_start, 2);
        }
        if let Some(exclusion_start) = generator_exclusion {
            self.assembler
                .mark_before_trailing_instructions(exclusion_start, 2);
        }
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if let Some((control_start, control_end)) = control_region {
            self.assembler.mark(control_end);
            self.add_control_flow_region_exclusion(control_start, control_end);
        }
        if let Some(exclusion_start) = generator_exclusion {
            self.generator_region_exclusions
                .push((exclusion_start, success));
        }
        Ok(())
    }

    fn compile_loop_tail_chained_compare(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        comparison: &ruff_python_ast::ExprCompare,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let cleanup = self.assembler.label();
        let success = self.assembler.label();
        let body = self.assembler.label();

        self.compile_expression(&comparison.left)?;
        for (operator, comparator) in comparison
            .ops
            .iter()
            .zip(&comparison.comparators)
            .take(comparison.ops.len() - 1)
        {
            self.compile_expression(comparator)?;
            self.assembler
                .set_location(self.source_location(comparison.range));
            self.emit(SWAP, 2, 0)?;
            self.emit(COPY, 2, 1)?;
            let (opcode, argument) = comparison_operator_boolean(*operator);
            self.emit(opcode, argument, -1)?;
            self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
        }

        self.compile_expression(comparison.comparators.last().unwrap())?;
        self.assembler
            .set_location(self.source_location(comparison.range));
        let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
        self.emit(opcode, argument, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_TRUE, success, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(success);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit_jump_forward(JUMP_FORWARD, body, 0)?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 1);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(body);
        self.set_depth(base_depth);
        let body_start = self.assembler.instruction_count();
        let nested_tail = self.compile_loop_tail_suite(&statement.body, restart)?;
        if !nested_tail && !suite_terminates(&statement.body) {
            self.set_branch_end_location(&statement.body, body_start);
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        }
        Ok(())
    }

    fn compile_match(
        &mut self,
        statement: &ruff_python_ast::StmtMatch,
    ) -> Result<(), CompileError> {
        self.compile_match_inner(statement, false)
    }

    fn compile_match_inner(
        &mut self,
        statement: &ruff_python_ast::StmtMatch,
        terminal: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        if matches!(statement.subject.as_ref(), Expr::Tuple(_))
            && let Some(constant) = fold_constant(&statement.subject)
        {
            if self.constants.is_empty()
                && let Some(seed) = first_literal_constant(&statement.subject)
            {
                self.add_constant(seed)?;
            }
            self.assembler
                .set_location(self.source_location(statement.subject.range()));
            self.emit_deferred_constant(constant)?;
        } else {
            self.compile_expression(&statement.subject)?;
        }
        let end = self.assembler.label();
        let has_default = statement.cases.len() > 1
            && statement
                .cases
                .last()
                .is_some_and(|case| is_wildcard_pattern(&case.pattern));
        let matched_cases = statement.cases.len() - usize::from(has_default);
        let mut failure_reaches_end = false;

        for (index, case) in statement.cases[..matched_cases].iter().enumerate() {
            let copied_subject = index + 1 != matched_cases;
            if copied_subject {
                self.assembler
                    .set_location(self.source_location(case.pattern.range()));
                self.emit(COPY, 1, 1)?;
            }
            let failure_depth = base_depth + i32::from(copied_subject);
            let mut context = MatchContext {
                stores: Vec::new(),
                fail_pop: Vec::new(),
                on_top: 0,
            };
            self.compile_stack_pattern(&case.pattern, &mut context)?;
            for name in &context.stores {
                self.assembler
                    .set_location(self.source_location(case.pattern.range()));
                self.store_name(name)?;
            }
            let mut guard_always_false = false;
            let mut irrefutable_true_guard = false;
            if let Some(guard) = &case.guard {
                if let Some(truthiness) = early_condition_truthiness(guard) {
                    self.record_folded_value(guard)?;
                    irrefutable_true_guard = truthiness && context.fail_pop.is_empty();
                    if irrefutable_true_guard {
                        let location = self.source_location(guard.range());
                        if self
                            .assembler
                            .last_instruction_location()
                            .is_none_or(|previous| previous.line != location.line)
                        {
                            self.assembler.set_location(location);
                            self.emit(NOP, 0, 0)?;
                        }
                    }
                    if !truthiness || !context.fail_pop.is_empty() {
                        let range = if is_wildcard_pattern(&case.pattern) {
                            case.pattern.range()
                        } else {
                            guard.range()
                        };
                        self.assembler.set_location(self.source_location(range));
                        self.emit(NOP, 0, 0)?;
                    }
                    if !truthiness {
                        self.ensure_match_fail_pop(&mut context, 0);
                        guard_always_false = true;
                    }
                } else {
                    self.ensure_match_fail_pop(&mut context, 0);
                    self.compile_jump_if(guard, false, context.fail_pop[0])?;
                }
            }
            if !guard_always_false {
                if copied_subject {
                    self.assembler.set_location(
                        case.body.first().map_or(SourceLocation::NONE, |statement| {
                            self.source_location(statement_execution_range(statement))
                        }),
                    );
                    self.emit(POP_TOP, 0, -1)?;
                }
                let body_previous_location = self.assembler.last_instruction_location();
                let body_start = self.assembler.instruction_count();
                let emitted_fallthrough = if terminal
                    && matches!(case.body.last(), Some(Stmt::If(_)))
                {
                    let previous_fallthrough =
                        std::mem::replace(&mut self.emitted_fallthrough_return, false);
                    self.compile_suite_inner(&case.body, true)?;
                    std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough)
                } else {
                    self.compile_suite(&case.body)?;
                    false
                };
                if !suite_terminates(&case.body) && !emitted_fallthrough {
                    self.set_branch_end_location(&case.body, body_start);
                    if terminal {
                        self.emit_deferred_implicit_return()?;
                    } else {
                        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                        if index + 1 == matched_cases && !context.fail_pop.is_empty() {
                            // CPython duplicates small exits before removing this jump across
                            // the empty final pattern-failure block.
                            self.assembler.defer_last_jump_removal();
                        }
                        let subject_pop_is_optimized_away = !copied_subject
                            && context.stores.is_empty()
                            && fold_constant(&statement.subject).is_some();
                        let previous_instruction_covers_branch = context.fail_pop.is_empty()
                            && !subject_pop_is_optimized_away
                            && body_previous_location.is_some_and(|location| {
                                location.line == self.assembler.location().line
                            });
                        if irrefutable_true_guard
                            || (self.assembler.instruction_count() == body_start + 1
                                && !previous_instruction_covers_branch
                                && !matches!(self.scope, Scope::Class { .. }))
                        {
                            self.assembler.preserve_last_inlined_jump_nop();
                        }
                    }
                }
            } else {
                self.pre_register_suite_names(&case.body)?;
            }
            if !context.fail_pop.is_empty() {
                self.emit_match_fail_pop(
                    &context,
                    self.source_location(case.pattern.range()),
                    failure_depth,
                )?;
                if index + 1 != matched_cases {
                    // CPython starts every following case in a fresh CFG block. Keep the
                    // failure cleanup separate so it cannot absorb the next case as a small
                    // exit when a large OR pattern pushes its success target out of range.
                    let boundary = self.assembler.label();
                    self.assembler.preserve_block_boundary(boundary);
                    self.assembler.mark(boundary);
                }
                failure_reaches_end = true;
            } else {
                failure_reaches_end = false;
                if index + 1 != matched_cases {
                    // A constant-true guard can make a syntactically guarded,
                    // irrefutable case unconditional. The remaining cases are
                    // unreachable, but code generation still starts them with
                    // the original subject on the stack.
                    self.set_depth(failure_depth);
                }
            }
        }

        if has_default {
            let case = statement.cases.last().unwrap();
            self.assembler
                .set_location(self.source_location(case.pattern.range()));
            self.emit(NOP, 0, 0)?;
            if let Some(guard) = &case.guard {
                self.compile_jump_if(guard, false, end)?;
                failure_reaches_end = true;
            }
            let body_start = self.assembler.instruction_count();
            let emitted_fallthrough = if terminal && matches!(case.body.last(), Some(Stmt::If(_))) {
                let previous_fallthrough =
                    std::mem::replace(&mut self.emitted_fallthrough_return, false);
                self.compile_suite_inner(&case.body, true)?;
                std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough)
            } else {
                self.compile_suite(&case.body)?;
                false
            };
            if terminal && !suite_terminates(&case.body) && !emitted_fallthrough {
                self.set_branch_end_location(&case.body, body_start);
                self.emit_deferred_implicit_return()?;
                failure_reaches_end = case.guard.is_some();
            }
        }

        self.assembler.mark(end);
        self.set_depth(base_depth);
        if terminal && failure_reaches_end {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_deferred_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn ensure_match_fail_pop(&mut self, context: &mut MatchContext, pops: usize) {
        while context.fail_pop.len() <= pops {
            let label = self.assembler.label();
            context.fail_pop.push(label);
        }
    }

    fn jump_to_match_fail(
        &mut self,
        context: &mut MatchContext,
        opcode: Opcode,
        effect: i32,
    ) -> Result<(), CompileError> {
        let pops = context.on_top + context.stores.len();
        self.ensure_match_fail_pop(context, pops);
        self.emit_jump_forward(opcode, context.fail_pop[pops], effect)?;
        if matches!(opcode.code(), 100..=103) {
            self.emit(NOT_TAKEN, 0, 0)?;
        }
        Ok(())
    }

    fn emit_match_fail_pop(
        &mut self,
        context: &MatchContext,
        location: SourceLocation,
        failure_depth: i32,
    ) -> Result<(), CompileError> {
        for pops in (1..context.fail_pop.len()).rev() {
            self.assembler.mark(context.fail_pop[pops]);
            self.set_depth(failure_depth + i32::try_from(pops).unwrap());
            self.assembler.set_location(location);
            self.emit(POP_TOP, 0, -1)?;
        }
        self.assembler.mark(context.fail_pop[0]);
        self.set_depth(failure_depth);
        Ok(())
    }

    fn compile_stack_pattern(
        &mut self,
        pattern: &Pattern,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        self.assembler
            .set_location(self.source_location(pattern.range()));
        match pattern {
            Pattern::MatchValue(pattern) => {
                if let Some(constant) = fold_constant(&pattern.value) {
                    self.emit_preprocessed_constant(&pattern.value, constant)?;
                } else {
                    self.compile_expression(&pattern.value)?;
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.emit(COMPARE_OP, 88, -1)?;
                self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)
            }
            Pattern::MatchSingleton(pattern) => {
                let constant = self.add_constant(match pattern.value {
                    Singleton::None => Constant::None,
                    Singleton::True => Constant::Bool(true),
                    Singleton::False => Constant::Bool(false),
                })?;
                self.emit(LOAD_CONST, constant, 1)?;
                self.emit(IS_OP, 0, -1)?;
                self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)
            }
            Pattern::MatchAs(pattern) => {
                if let Some(inner) = &pattern.pattern {
                    if !is_wildcard_pattern(inner) {
                        context.on_top += 1;
                        self.emit(COPY, 1, 1)?;
                        self.compile_stack_pattern(inner, context)?;
                        context.on_top -= 1;
                    }
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.capture_match_name(pattern.name.as_ref().map(|name| name.as_str()), context)
            }
            Pattern::MatchStar(pattern) => {
                self.capture_match_name(pattern.name.as_ref().map(|name| name.as_str()), context)
            }
            Pattern::MatchOr(pattern) => self.compile_stack_or_pattern(pattern, context),
            Pattern::MatchSequence(pattern) => {
                self.compile_stack_sequence_pattern(pattern, context)
            }
            Pattern::MatchMapping(pattern) => self.compile_stack_mapping_pattern(pattern, context),
            Pattern::MatchClass(pattern) => self.compile_stack_class_pattern(pattern, context),
        }
    }

    fn capture_match_name(
        &mut self,
        name: Option<&str>,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let Some(name) = name else {
            return self.emit(POP_TOP, 0, -1);
        };
        let mut rotations = context.on_top + context.stores.len() + 1;
        while rotations > 1 {
            self.emit(SWAP, to_u32(rotations, "pattern capture rotation")?, 0)?;
            rotations -= 1;
        }
        context.stores.push(name.to_string());
        Ok(())
    }

    fn rotate_match_stack(&mut self, mut count: usize) -> Result<(), CompileError> {
        while count > 1 {
            self.emit(SWAP, to_u32(count, "pattern capture rotation")?, 0)?;
            count -= 1;
        }
        Ok(())
    }

    fn compile_stack_or_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchOr,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let entry_depth = self.depth;
        let end = self.assembler.label();
        let mut control: Option<Vec<String>> = None;
        for (index, alternative) in pattern.patterns.iter().enumerate() {
            self.assembler
                .set_location(self.source_location(alternative.range()));
            self.emit(COPY, 1, 1)?;
            let mut alternative_context = MatchContext {
                stores: Vec::new(),
                fail_pop: Vec::new(),
                on_top: 0,
            };
            self.compile_stack_pattern(alternative, &mut alternative_context)?;

            if index == 0 {
                control = Some(alternative_context.stores.clone());
            } else {
                let control = control.as_ref().unwrap();
                if alternative_context.stores.len() != control.len() {
                    return Err(CompileError::Parse(
                        "alternative patterns bind different names".to_string(),
                    ));
                }
                for control_index in (0..control.len()).rev() {
                    let Some(store_index) = alternative_context
                        .stores
                        .iter()
                        .position(|name| name == &control[control_index])
                    else {
                        return Err(CompileError::Parse(
                            "alternative patterns bind different names".to_string(),
                        ));
                    };
                    if control_index != store_index {
                        debug_assert!(store_index < control_index);
                        let rotations = store_index + 1;
                        let rotated: Vec<_> =
                            alternative_context.stores.drain(..rotations).collect();
                        alternative_context.stores.splice(
                            control_index - store_index..control_index - store_index,
                            rotated,
                        );
                        for _ in 0..rotations {
                            self.rotate_match_stack(control_index + 1)?;
                        }
                    }
                }
            }

            self.assembler
                .set_location(self.source_location(alternative.range()));
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if alternative_context.fail_pop.is_empty() {
                self.set_depth(entry_depth);
            } else {
                self.emit_match_fail_pop(
                    &alternative_context,
                    self.source_location(alternative.range()),
                    entry_depth,
                )?;
            }
        }
        self.assembler
            .set_location(self.source_location(pattern.range));
        self.emit(POP_TOP, 0, -1)?;
        self.jump_to_match_fail(context, JUMP_FORWARD, 0)?;
        self.assembler.mark(end);
        let control = control.unwrap();
        self.set_depth(entry_depth + i32::try_from(control.len()).unwrap());
        self.assembler
            .set_location(self.source_location(pattern.range));
        let rotations = control.len() + 1 + context.on_top + context.stores.len();
        for name in control {
            self.rotate_match_stack(rotations)?;
            if context.stores.contains(&name) {
                return Err(CompileError::Parse(format!(
                    "multiple assignments to name '{name}' in pattern"
                )));
            }
            context.stores.push(name);
        }
        self.emit(POP_TOP, 0, -1)
    }

    fn compile_stack_sequence_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchSequence,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let starred = pattern
            .patterns
            .iter()
            .position(|pattern| matches!(pattern, Pattern::MatchStar(_)));
        let minimum = pattern.patterns.len() - usize::from(starred.is_some());
        let only_wildcards = pattern.patterns.iter().all(|pattern| {
            matches!(pattern, Pattern::MatchAs(pattern) if pattern.pattern.is_none() && pattern.name.is_none())
                || matches!(pattern, Pattern::MatchStar(pattern) if pattern.name.is_none())
        });
        context.on_top += 1;
        self.emit(MATCH_SEQUENCE, 0, 1)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        if starred.is_none() || pattern.patterns.len() > 1 {
            self.emit(GET_LEN, 0, 1)?;
            if self.constants.is_empty() {
                self.add_constant(Constant::Int(u64::try_from(minimum).unwrap_or(u64::MAX)))?;
            }
            self.emit(
                LOAD_SMALL_INT,
                to_u32(minimum, "sequence pattern length")?,
                1,
            )?;
            self.emit(COMPARE_OP, if starred.is_some() { 172 } else { 72 }, -1)?;
            self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        }
        context.on_top -= 1;
        if only_wildcards {
            return self.emit(POP_TOP, 0, -1);
        }
        if let Some(starred) = starred
            && matches!(&pattern.patterns[starred], Pattern::MatchStar(pattern) if pattern.name.is_none())
        {
            context.on_top += 1;
            for (index, subpattern) in pattern.patterns.iter().enumerate() {
                if index == starred || is_wildcard_pattern(subpattern) {
                    continue;
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.emit(COPY, 1, 1)?;
                if index < starred {
                    self.emit(LOAD_SMALL_INT, to_u32(index, "sequence pattern index")?, 1)?;
                } else {
                    self.emit(GET_LEN, 0, 1)?;
                    self.emit(
                        LOAD_SMALL_INT,
                        to_u32(
                            pattern.patterns.len() - index,
                            "sequence pattern suffix index",
                        )?,
                        1,
                    )?;
                    self.emit(BINARY_OP, binary_operator(Operator::Sub, false), -1)?;
                }
                self.emit(BINARY_OP, 26, -1)?;
                self.compile_stack_pattern(subpattern, context)?;
            }
            context.on_top -= 1;
            self.assembler
                .set_location(self.source_location(pattern.range));
            return self.emit(POP_TOP, 0, -1);
        }
        if let Some(starred) = starred {
            let before = to_u32(starred, "sequence pattern prefix length")?;
            let after = to_u32(
                pattern.patterns.len() - starred - 1,
                "sequence pattern suffix length",
            )?;
            self.emit(
                UNPACK_EX,
                before | (after << 8),
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        } else {
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.patterns.len(), "sequence pattern length")?,
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        }
        context.on_top += pattern.patterns.len();
        for subpattern in &pattern.patterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        Ok(())
    }

    fn compile_stack_mapping_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchMapping,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        context.on_top += 1;
        self.emit(MATCH_MAPPING, 0, 1)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        if pattern.keys.is_empty() && pattern.rest.is_none() {
            context.on_top -= 1;
            return self.emit(POP_TOP, 0, -1);
        }
        if !pattern.keys.is_empty() {
            self.emit(GET_LEN, 0, 1)?;
            self.add_constant(Constant::Int(
                u64::try_from(pattern.keys.len()).unwrap_or(u64::MAX),
            ))?;
            self.emit(
                LOAD_SMALL_INT,
                to_u32(pattern.keys.len(), "mapping pattern key count")?,
                1,
            )?;
            self.emit(COMPARE_OP, 172, -1)?;
            self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        }
        if let Some(keys) = pattern
            .keys
            .iter()
            .map(fold_constant)
            .collect::<Option<Vec<_>>>()
        {
            self.emit_deferred_constant(Constant::Tuple(keys))?;
        } else {
            for key in &pattern.keys {
                self.compile_expression(key)?;
            }
            self.emit_build(BUILD_TUPLE, pattern.keys.len())?;
        }
        self.emit(MATCH_KEYS, 0, 1)?;
        context.on_top += 2;
        self.emit(COPY, 1, 1)?;
        self.add_constant(Constant::None)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_NONE, -1)?;
        self.emit(
            UNPACK_SEQUENCE,
            to_u32(pattern.patterns.len(), "mapping pattern value count")?,
            i32::try_from(pattern.patterns.len()).unwrap() - 1,
        )?;
        context.on_top += pattern.patterns.len();
        context.on_top -= 1;
        for subpattern in &pattern.patterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        context.on_top -= 2;
        if let Some(rest) = &pattern.rest {
            self.assembler
                .set_location(self.source_location(pattern.range));
            self.emit(BUILD_MAP, 0, 1)?;
            self.emit(SWAP, 3, 0)?;
            self.emit(DICT_UPDATE, 2, -1)?;
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.keys.len(), "mapping pattern key count")?,
                i32::try_from(pattern.keys.len()).unwrap() - 1,
            )?;
            let mut size = pattern.keys.len();
            while size > 0 {
                self.emit(COPY, to_u32(1 + size, "mapping rest copy depth")?, 1)?;
                self.emit(SWAP, 2, 0)?;
                self.emit(DELETE_SUBSCR, 0, -2)?;
                size -= 1;
            }
            self.capture_match_name(Some(rest.as_str()), context)
        } else {
            self.assembler
                .set_location(self.source_location(pattern.range));
            self.emit(POP_TOP, 0, -1)?;
            self.emit(POP_TOP, 0, -1)
        }
    }

    fn compile_stack_class_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchClass,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        self.compile_expression(&pattern.cls)?;
        let keyword_names = pattern
            .arguments
            .keywords
            .iter()
            .map(|keyword| Constant::String(keyword.attr.as_str().to_string()))
            .collect();
        let keyword_names = self.add_interned_string_tuple(keyword_names)?;
        self.emit(LOAD_CONST, keyword_names, 1)?;
        self.emit(
            MATCH_CLASS,
            to_u32(
                pattern.arguments.patterns.len(),
                "class positional pattern count",
            )?,
            -2,
        )?;
        self.emit(COPY, 1, 1)?;
        self.add_constant(Constant::None)?;
        context.on_top += 1;
        self.jump_to_match_fail(context, POP_JUMP_IF_NONE, -1)?;
        let subpatterns = pattern
            .arguments
            .patterns
            .iter()
            .chain(
                pattern
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| &keyword.pattern),
            )
            .collect::<Vec<_>>();
        self.emit(
            UNPACK_SEQUENCE,
            to_u32(subpatterns.len(), "class pattern count")?,
            i32::try_from(subpatterns.len()).unwrap() - 1,
        )?;
        context.on_top += subpatterns.len();
        context.on_top -= 1;
        for subpattern in subpatterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        Ok(())
    }

    fn compile_pattern(
        &mut self,
        pattern: &Pattern,
        subject: u32,
        failure: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        self.assembler
            .set_location(self.source_location(pattern.range()));
        match pattern {
            Pattern::MatchValue(pattern) => {
                self.emit(LOAD_FAST, subject, 1)?;
                self.compile_expression(&pattern.value)?;
                self.emit(COMPARE_OP, 88, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, failure, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }
            Pattern::MatchSingleton(pattern) => {
                self.emit(LOAD_FAST, subject, 1)?;
                let constant = self.add_constant(match pattern.value {
                    Singleton::None => Constant::None,
                    Singleton::True => Constant::Bool(true),
                    Singleton::False => Constant::Bool(false),
                })?;
                self.emit(LOAD_CONST, constant, 1)?;
                self.emit(IS_OP, 0, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, failure, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }
            Pattern::MatchAs(pattern) => {
                if let Some(inner) = &pattern.pattern {
                    self.compile_pattern(inner, subject, failure)?;
                }
                if let Some(name) = &pattern.name {
                    self.emit(LOAD_FAST, subject, 1)?;
                    self.store_name(name.as_str())?;
                }
            }
            Pattern::MatchStar(pattern) => {
                if let Some(name) = &pattern.name {
                    self.emit(LOAD_FAST, subject, 1)?;
                    self.store_name(name.as_str())?;
                }
            }
            Pattern::MatchOr(pattern) => {
                let success = self.assembler.label();
                for (index, alternative) in pattern.patterns.iter().enumerate() {
                    if index + 1 == pattern.patterns.len() {
                        self.compile_pattern(alternative, subject, failure)?;
                    } else {
                        let next = self.assembler.label();
                        self.compile_pattern(alternative, subject, next)?;
                        self.emit_jump_forward(JUMP_FORWARD, success, 0)?;
                        self.assembler.mark(next);
                        self.set_depth(base_depth);
                    }
                }
                self.assembler.mark(success);
                self.set_depth(base_depth);
            }
            Pattern::MatchSequence(pattern) => {
                self.compile_sequence_pattern(pattern, subject, failure)?;
            }
            Pattern::MatchMapping(pattern) => {
                self.compile_mapping_pattern(pattern, subject, failure)?;
            }
            Pattern::MatchClass(pattern) => {
                self.compile_class_pattern(pattern, subject, failure)?;
            }
        }
        if self.depth != base_depth {
            return Err(CompileError::Internal(format!(
                "pattern changed stack depth from {base_depth} to {}",
                self.depth
            )));
        }
        Ok(())
    }

    fn compile_sequence_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchSequence,
        subject: u32,
        failure: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let structural_failure = self.assembler.label();
        let success = self.assembler.label();
        let starred = pattern
            .patterns
            .iter()
            .position(|pattern| matches!(pattern, Pattern::MatchStar(_)));
        let minimum = pattern.patterns.len() - usize::from(starred.is_some());

        self.emit(LOAD_FAST, subject, 1)?;
        self.emit(MATCH_SEQUENCE, 0, 1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, structural_failure, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(GET_LEN, 0, 1)?;
        self.emit(
            LOAD_SMALL_INT,
            to_u32(minimum, "sequence pattern length")?,
            1,
        )?;
        self.emit(COMPARE_OP, if starred.is_some() { 172 } else { 72 }, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, structural_failure, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;

        if let Some(starred) = starred {
            let before = to_u32(starred, "sequence pattern prefix length")?;
            let after = to_u32(
                pattern.patterns.len() - starred - 1,
                "sequence pattern suffix length",
            )?;
            self.emit(
                UNPACK_EX,
                before | (after << 8),
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        } else {
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.patterns.len(), "sequence pattern length")?,
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        }
        let mut elements = Vec::with_capacity(pattern.patterns.len());
        for _ in &pattern.patterns {
            let temporary = self.allocate_match_temporary()?;
            self.emit(STORE_FAST, temporary, -1)?;
            elements.push(temporary);
        }
        for (element, pattern) in elements.into_iter().zip(&pattern.patterns) {
            self.compile_pattern(pattern, element, failure)?;
        }
        self.emit_jump_forward(JUMP_FORWARD, success, 0)?;

        self.assembler.mark(structural_failure);
        self.set_depth(base_depth + 1);
        self.emit(POP_TOP, 0, -1)?;
        self.emit_jump_forward(JUMP_FORWARD, failure, 0)?;
        self.assembler.mark(success);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_mapping_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchMapping,
        subject: u32,
        failure: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let structural_failure = self.assembler.label();
        let keys_failure = self.assembler.label();
        let success = self.assembler.label();

        self.emit(LOAD_FAST, subject, 1)?;
        self.emit(MATCH_MAPPING, 0, 1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, structural_failure, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(GET_LEN, 0, 1)?;
        self.emit(
            LOAD_SMALL_INT,
            to_u32(pattern.keys.len(), "mapping pattern key count")?,
            1,
        )?;
        self.emit(COMPARE_OP, 172, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, structural_failure, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;

        let keys_temporary = if pattern.keys.is_empty() {
            None
        } else {
            for key in &pattern.keys {
                self.compile_expression(key)?;
            }
            self.emit_build(BUILD_TUPLE, pattern.keys.len())?;
            let temporary = self.allocate_match_temporary()?;
            self.emit(COPY, 1, 1)?;
            self.emit(STORE_FAST, temporary, -1)?;
            self.emit(MATCH_KEYS, 0, 1)?;
            self.emit(COPY, 1, 1)?;
            self.emit_jump_forward(POP_JUMP_IF_NONE, keys_failure, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.patterns.len(), "mapping pattern value count")?,
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
            Some(temporary)
        };

        let mut values = Vec::with_capacity(pattern.patterns.len());
        for _ in &pattern.patterns {
            let temporary = self.allocate_match_temporary()?;
            self.emit(STORE_FAST, temporary, -1)?;
            values.push(temporary);
        }
        if keys_temporary.is_some() {
            self.emit(POP_TOP, 0, -1)?;
        }
        self.emit(POP_TOP, 0, -1)?;

        if let Some(rest) = &pattern.rest {
            self.emit(BUILD_MAP, 0, 1)?;
            self.emit(LOAD_FAST, subject, 1)?;
            self.emit(DICT_UPDATE, 1, -1)?;
            if let Some(keys) = keys_temporary {
                for index in 0..pattern.keys.len() {
                    self.emit(COPY, 1, 1)?;
                    self.emit(LOAD_FAST, keys, 1)?;
                    self.emit(
                        LOAD_SMALL_INT,
                        to_u32(index, "mapping pattern key index")?,
                        1,
                    )?;
                    self.emit(BINARY_OP, 26, -1)?;
                    self.emit(DELETE_SUBSCR, 0, -2)?;
                }
            }
            self.store_name(rest.as_str())?;
        }
        for (value, pattern) in values.into_iter().zip(&pattern.patterns) {
            self.compile_pattern(pattern, value, failure)?;
        }
        self.emit_jump_forward(JUMP_FORWARD, success, 0)?;

        if !pattern.keys.is_empty() {
            self.assembler.mark(keys_failure);
            self.set_depth(base_depth + 3);
            self.emit(POP_TOP, 0, -1)?;
            self.emit(POP_TOP, 0, -1)?;
            self.emit(POP_TOP, 0, -1)?;
            self.emit_jump_forward(JUMP_FORWARD, failure, 0)?;
        }
        self.assembler.mark(structural_failure);
        self.set_depth(base_depth + 1);
        self.emit(POP_TOP, 0, -1)?;
        self.emit_jump_forward(JUMP_FORWARD, failure, 0)?;
        self.assembler.mark(success);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_class_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchClass,
        subject: u32,
        failure: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let structural_failure = self.assembler.label();
        let success = self.assembler.label();
        let count = pattern.arguments.patterns.len() + pattern.arguments.keywords.len();

        self.emit(LOAD_FAST, subject, 1)?;
        self.compile_expression(&pattern.cls)?;
        let keywords = pattern
            .arguments
            .keywords
            .iter()
            .map(|keyword| Constant::String(keyword.attr.as_str().to_string()))
            .collect();
        let keywords = self.add_interned_string_tuple(keywords)?;
        self.emit(LOAD_CONST, keywords, 1)?;
        self.emit(
            MATCH_CLASS,
            to_u32(
                pattern.arguments.patterns.len(),
                "class positional pattern count",
            )?,
            -2,
        )?;
        self.emit(COPY, 1, 1)?;
        self.emit_jump_forward(POP_JUMP_IF_NONE, structural_failure, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        if count == 0 {
            self.emit(POP_TOP, 0, -1)?;
        } else {
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(count, "class pattern argument count")?,
                i32::try_from(count).unwrap() - 1,
            )?;
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let temporary = self.allocate_match_temporary()?;
            self.emit(STORE_FAST, temporary, -1)?;
            values.push(temporary);
        }
        for (value, argument) in values.into_iter().zip(
            pattern.arguments.patterns.iter().chain(
                pattern
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| &keyword.pattern),
            ),
        ) {
            self.compile_pattern(argument, value, failure)?;
        }
        self.emit_jump_forward(JUMP_FORWARD, success, 0)?;

        self.assembler.mark(structural_failure);
        self.set_depth(base_depth + 1);
        self.emit(POP_TOP, 0, -1)?;
        self.emit_jump_forward(JUMP_FORWARD, failure, 0)?;
        self.assembler.mark(success);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_while(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        if let Some(truthiness) = early_condition_truthiness(&statement.test) {
            if self.constants.is_empty()
                && let Some(constant) = fold_constant(&statement.test)
            {
                self.add_constant(constant)?;
            }
            if !truthiness {
                self.pre_register_suite_names(&statement.body)?;
                // The eliminated loop body leaves a CFG boundary that CPython's borrowed-load
                // optimizer does not traverse into the following block.
                self.assembler.set_strict_owned_loads(true);
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
                self.emit(NOP, 0, 0)?;
                self.compile_suite(&statement.orelse)?;
                return Ok(());
            }

            let start = self.assembler.label();
            let end = self.assembler.label();
            self.assembler.mark(start);
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            self.loops.push(LoopContext {
                continue_label: start,
                break_label: end,
                iterator_cleanup: IteratorCleanup::None,
                with_depth: self.active_with_exits.len(),
                finally_end_depth: self.active_finally_end_blocks,
                exception_region_depth: self.active_exception_region_exclusions.len(),
                break_returns: false,
                preserve_break_exit: self.preserve_finally_break_exit_loop_range
                    == Some(statement.range),
            });
            let body_start = self.assembler.instruction_count();
            let tail_jumps = self.compile_while_tail_suite(&statement.body, start)?;
            self.loops.pop();
            if !tail_jumps && !suite_terminates(&statement.body) {
                self.emit_while_backedge(&statement.body, body_start, start)?;
            }
            self.pre_register_suite_names(&statement.orelse)?;
            self.assembler.mark(end);
            if suite_contains_loop_break(&statement.body) {
                // A constant-true loop can reach its exit only through `break`. CPython removes
                // a redundant break jump before borrowed-load analysis but does not reconnect
                // the large exit block; a copied small exit remains independently reachable.
                self.assembler.prevent_borrow_reachability(end);
            }
            self.set_depth(base_depth);
            if statement.orelse.is_empty() {
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
            }
            return Ok(());
        }
        let start = self.assembler.label();
        let else_label = self.assembler.label();
        let end = self.assembler.label();

        self.assembler.mark(start);
        let previous_exception_exclusion = std::mem::replace(
            &mut self.exclude_condition_not_taken_from_exception,
            !self.active_exception_region_exclusions.is_empty()
                || self.active_terminal_withs > 0
                || self.generator_region_start.is_some(),
        );
        let condition_result = self.compile_jump_if(&statement.test, false, else_label);
        self.exclude_condition_not_taken_from_exception = previous_exception_exclusion;
        condition_result?;
        self.loops.push(LoopContext {
            continue_label: start,
            break_label: end,
            iterator_cleanup: IteratorCleanup::None,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: false,
            preserve_break_exit: self.preserve_finally_break_exit_loop_range
                == Some(statement.range),
        });
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.compile_while_tail_suite(&statement.body, start)?;
        self.loops.pop();
        if !tail_jumps && !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }

        self.assembler.mark(else_label);
        self.set_depth(base_depth);
        if statement.orelse.is_empty() && !self.active_with_region_exclusions.is_empty() {
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            if self.generator_region_start.is_some() {
                self.generator_region_exclusions
                    .push((exclusion_start, exclusion_end));
            }
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
        }
        self.compile_suite(&statement.orelse)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        if statement.orelse.is_empty() {
            self.assembler
                .set_location(self.source_location(statement.test.range()));
        }
        Ok(())
    }

    fn compile_for(&mut self, statement: &ruff_python_ast::StmtFor) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let has_break = suite_contains_loop_break(&statement.body);
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        let mut newly_initialized_targets = Vec::new();
        collect_target_names(&statement.target, &mut newly_initialized_targets);
        newly_initialized_targets.retain(|name| !self.initialized_locals.contains(name));

        self.compile_iterable_expression(&statement.iter)?;
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(GET_ITER, 0, 0)?;
        self.assembler.mark(start);
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&statement.target)?;

        self.loops.push(LoopContext {
            continue_label: start,
            break_label: end,
            iterator_cleanup: IteratorCleanup::Sync,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: false,
            preserve_break_exit: self.preserve_finally_break_exit_loop_range
                == Some(statement.range),
        });
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.compile_loop_tail_suite(&statement.body, start)?;
        self.loops.pop();
        if !tail_jumps && !suite_terminates(&statement.body) {
            if self.assembler.instruction_count() > body_start
                && let Some(location) = self.assembler.last_instruction_location()
            {
                self.assembler.set_location(location);
            } else if let Some(statement) = statement.body.last() {
                self.assembler
                    .set_location(self.source_location(statement.range()));
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        for name in &newly_initialized_targets {
            self.initialized_locals.remove(name);
        }
        let orelse_start = self.assembler.instruction_count();
        self.compile_suite(&statement.orelse)?;
        if self.assembler.instruction_count() == orelse_start
            && let [Stmt::Pass(statement)] = statement.orelse.as_slice()
            && self.active_exception_region_exclusions.is_empty()
            && !self.active_with_region_exclusions.is_empty()
        {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }
        if has_break {
            self.assembler.mark(end);
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_async_for(
        &mut self,
        statement: &ruff_python_ast::StmtFor,
    ) -> Result<(), CompileError> {
        if self.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("async for outside coroutine code"));
        }

        let base_depth = self.depth;
        let has_break = suite_contains_loop_break(&statement.body);
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let async_cleanup = self.assembler.label();
        let end = self.assembler.label();
        let mut newly_initialized_targets = Vec::new();
        collect_target_names(&statement.target, &mut newly_initialized_targets);
        newly_initialized_targets.retain(|name| !self.initialized_locals.contains(name));

        self.compile_expression(&statement.iter)?;
        let iterator_location = self.source_location(statement.iter.range());
        self.assembler.set_location(iterator_location);
        self.emit(GET_AITER, 0, 0)?;
        self.assembler
            .set_location(self.source_location(statement.range));
        self.assembler.mark(start);
        self.assembler.mark(protected_start);
        self.emit(GET_ANEXT, 0, 1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, send_end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit(RESUME, 3, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
        self.assembler.mark(send_end);
        self.set_depth(base_depth + 3);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.emit(NOT_TAKEN, 0, 0)?;
        self.compile_store_target(&statement.target)?;

        self.loops.push(LoopContext {
            continue_label: start,
            break_label: end,
            iterator_cleanup: IteratorCleanup::Async,
            with_depth: self.active_with_exits.len(),
            finally_end_depth: self.active_finally_end_blocks,
            exception_region_depth: self.active_exception_region_exclusions.len(),
            break_returns: false,
            preserve_break_exit: self.preserve_finally_break_exit_loop_range
                == Some(statement.range),
        });
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.compile_loop_tail_suite(&statement.body, start)?;
        self.loops.pop();
        if !tail_jumps && !suite_terminates(&statement.body) {
            if self.assembler.instruction_count() > body_start
                && let Some(location) = self.assembler.last_instruction_location()
            {
                self.assembler.set_location(location);
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(cleanup_throw);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(CLEANUP_THROW, 0, -1)?;
        let cleanup_throw_end = self.assembler.label();
        self.assembler.mark(cleanup_throw_end);
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (base_depth + 3).cast_unsigned(),
            false,
        );

        self.assembler.mark(async_cleanup);
        self.generator_region_exclusions
            .push((cleanup_throw, async_cleanup));
        for exclusions in &mut self.active_with_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        for exclusions in &mut self.active_exception_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        self.set_depth(base_depth + 2);
        self.assembler.set_location(iterator_location);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(END_ASYNC_FOR, send_end);
        self.assembler.add_exception_region(
            protected_start,
            protected_end,
            async_cleanup,
            (base_depth + 1).cast_unsigned(),
            false,
        );
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            (base_depth + 1).cast_unsigned(),
            false,
        );
        for name in &newly_initialized_targets {
            self.initialized_locals.remove(name);
        }
        self.compile_suite(&statement.orelse)?;
        if has_break {
            self.assembler.mark(end);
        }
        if self.emit_protected_async_for_end_nop && statement.orelse.is_empty() && !has_break {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler
                .set_location(self.source_location(statement.iter.range()));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            // CPython's coroutine stop handler excludes both the synthetic end block and the
            // preceding END_ASYNC_FOR, while the surrounding protected region still owns the
            // END_ASYNC_FOR itself.
            self.generator_region_exclusions
                .push((async_cleanup, noop_end));
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_augmented_assignment(
        &mut self,
        assignment: &ruff_python_ast::StmtAugAssign,
    ) -> Result<(), CompileError> {
        match assignment.target.as_ref() {
            Expr::Name(target) => {
                let target_location = self.source_location(target.range);
                self.assembler.set_location(target_location);
                self.load_name(target.id.as_str())?;
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit(BINARY_OP, binary_operator(assignment.op, true), -1)?;
                self.assembler.set_location(target_location);
                self.store_name(target.id.as_str())?;
            }
            Expr::Attribute(target) => {
                self.compile_expression(&target.value)?;
                self.assembler
                    .set_location(self.source_location(target.range));
                self.emit(COPY, 1, 1)?;
                let index = self.name_index(target.attr.as_str())?;
                let attribute_location = self.source_location(self.attribute_opcode_range(target));
                self.assembler.set_location(attribute_location);
                self.emit(LOAD_ATTR, index << 1, 0)?;
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit(BINARY_OP, binary_operator(assignment.op, true), -1)?;
                self.assembler.set_location(attribute_location);
                self.emit(Opcode::new(117, 0), 2, 0)?;
                self.emit(STORE_ATTR, index, -2)?;
            }
            Expr::Subscript(target) => {
                self.compile_expression(&target.value)?;
                let optimized_slice = if let Expr::Slice(slice) = target.slice.as_ref()
                    && two_element_slice_optimization(slice)
                {
                    let slice_location = self.source_location(slice.range);
                    self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                    self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                    true
                } else {
                    self.compile_expression(&target.slice)?;
                    false
                };
                let target_location = self.source_location(target.range);
                self.assembler.set_location(target_location);
                if optimized_slice {
                    self.emit(COPY, 3, 1)?;
                    self.emit(COPY, 3, 1)?;
                    self.emit(COPY, 3, 1)?;
                    self.emit(BINARY_SLICE, 0, -2)?;
                } else {
                    self.emit(COPY, 2, 1)?;
                    self.emit(COPY, 2, 1)?;
                    self.emit(BINARY_OP, 26, -1)?;
                }
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit(BINARY_OP, binary_operator(assignment.op, true), -1)?;
                self.assembler.set_location(target_location);
                if optimized_slice {
                    self.emit(SWAP, 4, 0)?;
                    self.emit(SWAP, 3, 0)?;
                    self.emit(SWAP, 2, 0)?;
                    self.emit(STORE_SLICE, 0, -4)?;
                } else {
                    self.emit(SWAP, 3, 0)?;
                    self.emit(SWAP, 2, 0)?;
                    self.emit(STORE_SUBSCR, 0, -3)?;
                }
            }
            _ => return Err(unsupported("augmented assignment target")),
        }
        Ok(())
    }

    fn compile_annotated_assignment(
        &mut self,
        assignment: &ruff_python_ast::StmtAnnAssign,
    ) -> Result<(), CompileError> {
        if assignment.simple
            && (matches!(self.scope, Scope::Module)
                || matches!(self.scope, Scope::Class { .. })
                    && self.flags & CO_FUTURE_ANNOTATIONS != 0)
        {
            if let Some(value) = &assignment.value {
                self.compile_expression(value)?;
                self.compile_store_target(&assignment.target)?;
            }
            let Expr::Name(target) = assignment.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            if self.flags & CO_FUTURE_ANNOTATIONS != 0 {
                self.assembler
                    .set_location(self.source_location(assignment.annotation.range()));
                let annotation = unparse_annotation(&assignment.annotation);
                self.load_string_constant(&annotation)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.load_name("__annotations__")?;
                let target = self.mangled_name(target.id.as_str());
                self.load_string_constant(&target)?;
                self.emit(STORE_SUBSCR, 0, -3)?;
            } else {
                let index = self.module_annotation_index;
                self.module_annotation_index += 1;
                if self.constants.is_empty() {
                    self.add_constant(Constant::Int(u64::from(index)))?;
                }
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.load_name("__conditional_annotations__")?;
                self.emit(LOAD_SMALL_INT, index, 1)?;
                self.emit(SET_ADD, 1, -1)?;
                self.emit(POP_TOP, 0, -1)?;
            }
            return Ok(());
        }
        if let Some(value) = &assignment.value {
            self.compile_expression(value)?;
            self.compile_store_target(&assignment.target)?;
            return Ok(());
        }

        match assignment.target.as_ref() {
            Expr::Name(_) => {}
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                self.assembler
                    .set_location(self.source_location(attribute.value.range()));
                self.emit(POP_TOP, 0, -1)?;
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                self.assembler
                    .set_location(self.source_location(subscript.value.range()));
                self.emit(POP_TOP, 0, -1)?;
                self.compile_discarded_annotation_slice(&subscript.slice)?;
            }
            _ => return Err(unsupported("annotated assignment target")),
        }
        Ok(())
    }

    fn compile_discarded_annotation_slice(
        &mut self,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        match expression {
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.compile_discarded_annotation_slice(element)?;
                }
            }
            Expr::Slice(slice) => {
                for element in [
                    slice.lower.as_deref(),
                    slice.upper.as_deref(),
                    slice.step.as_deref(),
                ]
                .into_iter()
                .flatten()
                {
                    self.compile_discarded_annotation_slice(element)?;
                }
            }
            _ if fold_constant(expression).is_some() => {
                if let Some(constant) = first_literal_constant(expression) {
                    self.add_constant(constant)?;
                }
            }
            _ => {
                self.compile_expression(expression)?;
                self.assembler
                    .set_location(self.source_location(expression.range()));
                self.emit(POP_TOP, 0, -1)?;
            }
        }
        Ok(())
    }

    fn compile_assert(
        &mut self,
        statement: &ruff_python_ast::StmtAssert,
    ) -> Result<(), CompileError> {
        if early_condition_truthiness(&statement.test) == Some(true) {
            if let Some(constant) = fold_constant(&statement.test) {
                self.add_constant(constant)?;
            }
            if let Some(message) = &statement.msg {
                self.pre_register_expression_names(message)?;
                self.pre_register_expression_constants(message)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            return self.emit(NOP, 0, 0);
        }
        let base_depth = self.depth;
        let end = self.assembler.label();
        let previous_exclusion = std::mem::replace(
            &mut self.exclude_terminal_if_not_taken,
            !self.active_with_region_exclusions.is_empty(),
        );
        self.compile_jump_if(&statement.test, true, end)?;
        self.exclude_terminal_if_not_taken = previous_exclusion;
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(LOAD_COMMON_CONSTANT, 0, 1)?;
        if let Some(message) = &statement.msg {
            self.compile_expression(message)?;
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit(RAISE_VARARGS, 1, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_raise(
        &mut self,
        statement: &ruff_python_ast::StmtRaise,
    ) -> Result<(), CompileError> {
        let mut argument_count = 0;
        if let Some(exception) = &statement.exc {
            self.compile_expression(exception)?;
            argument_count += 1;
            if let Some(cause) = &statement.cause {
                self.compile_expression(cause)?;
                argument_count += 1;
            }
        }
        self.emit(RAISE_VARARGS, argument_count, -argument_count.cast_signed())
    }

    fn compile_try(&mut self, statement: &ruff_python_ast::StmtTry) -> Result<(), CompileError> {
        self.compile_try_inner(statement, false, true, false)
    }

    fn compile_try_inner(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
        emit_statement_nop: bool,
        exclude_terminal_body_not_taken: bool,
    ) -> Result<(), CompileError> {
        if !statement.finalbody.is_empty() {
            return self.compile_try_finally(statement, terminal);
        }
        if statement.is_star {
            return self.compile_try_star_except(statement, terminal, emit_statement_nop);
        }
        if statement.handlers.is_empty() {
            return Err(unsupported("try statement without handlers"));
        }

        let base_depth = self.depth;
        let try_start = self.assembler.label();
        let try_end = self.assembler.label();
        let handler_start = self.assembler.label();
        let common_cleanup = self.assembler.label();
        let end = self.assembler.label();
        let body_ends_with_break_loop = statement.orelse.is_empty()
            && matches!(
                statement.body.last(),
                Some(Stmt::For(statement))
                    if statement.orelse.is_empty()
                        && suite_contains_loop_break(&statement.body)
            );

        if emit_statement_nop {
            let statement_nop_start = self.assembler.label();
            self.assembler.mark(statement_nop_start);
            self.assembler
                .set_location(self.source_location_including_trailing_semicolon(statement.range));
            self.emit(NOP, 0, 0)?;
            let statement_nop_end = self.assembler.label();
            self.assembler.mark(statement_nop_end);
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
            if self.generator_region_start.is_some() {
                self.generator_region_exclusions
                    .push((statement_nop_start, statement_nop_end));
            }
        }
        self.assembler.mark(try_start);
        self.active_exception_region_exclusions.push(Vec::new());
        let direct_continue =
            statement.orelse.is_empty() && matches!(statement.body.as_slice(), [Stmt::Continue(_)]);
        let body_noop_location = if direct_continue {
            self.assembler.mark(try_end);
            self.compile_statement(&statement.body[0])?;
            None
        } else {
            let body_instruction_count = self.assembler.instruction_count();
            if statement.body.len() > 1
                && let Some(Stmt::Pass(pass)) = statement.body.first()
            {
                self.assembler
                    .set_location(self.source_location(pass.range));
                self.emit(NOP, 0, 0)?;
            }
            if exclude_terminal_body_not_taken
                && let Some((last @ Stmt::If(_), leading)) = statement.body.split_last()
            {
                self.compile_suite(leading)?;
                let previous = std::mem::replace(&mut self.exclude_terminal_if_not_taken, true);
                let previous_exception =
                    std::mem::replace(&mut self.exclude_condition_not_taken_from_exception, true);
                let result = self.compile_statement(last);
                self.exclude_terminal_if_not_taken = previous;
                self.exclude_condition_not_taken_from_exception = previous_exception;
                result?;
            } else {
                self.compile_suite(&statement.body)?;
            }
            let location = (self.assembler.instruction_count() == body_instruction_count)
                .then(|| {
                    statement
                        .body
                        .last()
                        .map(|statement| self.source_location(statement.range()))
                })
                .flatten();
            self.assembler.mark(try_end);
            if !statement.orelse.is_empty()
                && let Some(location) = location
            {
                self.assembler.set_location(location);
                self.emit(NOP, 0, 0)?;
            }
            location
        };
        let try_exclusions = self
            .active_exception_region_exclusions
            .pop()
            .expect("active try region has exclusion collector");
        let orelse_instruction_count = self.assembler.instruction_count();
        self.compile_suite(&statement.orelse)?;
        let orelse_noop_location = (self.assembler.instruction_count() == orelse_instruction_count)
            .then(|| {
                statement
                    .orelse
                    .last()
                    .map(|statement| self.source_location(statement.range()))
            })
            .flatten();
        if direct_continue {
        } else if terminal {
            if let Some(location) = orelse_noop_location {
                self.assembler.set_location(location);
            } else if matches!(statement.body.last(), Some(Stmt::Try(_))) {
                self.assembler.set_location(SourceLocation::NONE);
            } else if statement.orelse.is_empty()
                && let Some(location) = body_noop_location
            {
                self.assembler.set_location(location);
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_deferred_implicit_return()?;
        } else {
            if let Some(location) = orelse_noop_location {
                self.assembler.set_location(location);
            } else if statement.orelse.is_empty()
                && let Some(Stmt::If(statement)) = statement.body.last()
                && let Some(test) = terminating_if_fallthrough_test(statement)
            {
                self.assembler
                    .set_location(self.source_location(test.range()));
            } else if statement.orelse.is_empty()
                && let Some(location) = body_noop_location
            {
                self.assembler.set_location(location);
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            let pass_only_handlers = statement.handlers.iter().all(|handler| {
                handler
                    .as_except_handler()
                    .is_some_and(|handler| matches!(handler.body.as_slice(), [Stmt::Pass(_)]))
            });
            let body_ends_with_overridden_return = self.active_overriding_finally_returns > 0
                && matches!(statement.body.last(), Some(Stmt::Return(_)));
            let exit_exclusion_start = if !self.active_exception_region_exclusions.is_empty()
                && (body_ends_with_overridden_return
                    || (body_noop_location.is_some()
                        && (self.active_overriding_finally_returns > 0
                            || (pass_only_handlers && !self.prevent_try_exit_inlining))))
            {
                let start = self.assembler.label();
                self.assembler.mark(start);
                Some(start)
            } else {
                None
            };
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if body_ends_with_break_loop {
                // CPython's `POP_BLOCK` initially prevents the loop break from threading
                // through this normal try exit. Once `POP_BLOCK` disappears, the exit jump
                // is retained as the loop cleanup's source-position NOP.
                self.assembler
                    .preserve_last_redundant_jump_nop_after_threading();
            }
            if statement.orelse.is_empty() && body_noop_location.is_some() {
                // Keep the pass line when direct exit duplication replaces this jump.
                self.assembler.preserve_last_direct_inlined_jump_nop();
            }
            if let Some(exit_exclusion_start) = exit_exclusion_start {
                let exit_exclusion_end = self.assembler.label();
                self.assembler.mark(exit_exclusion_end);
                for exclusions in &mut self.active_exception_region_exclusions {
                    exclusions.push((exit_exclusion_start, exit_exclusion_end));
                }
            }
            if self.prevent_try_exit_inlining {
                self.assembler.prevent_last_jump_inlining();
            }
        }
        self.add_exception_regions_with_exclusions(
            try_start,
            try_end,
            &try_exclusions,
            handler_start,
            base_depth.cast_unsigned(),
            false,
        );

        self.assembler.mark(handler_start);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let mut dispatch_start = handler_start;
        let mut final_handler_location = SourceLocation::NONE;

        for (handler_index, handler) in statement.handlers.iter().enumerate() {
            let handler = handler.as_except_handler().ok_or_else(|| {
                CompileError::Internal("invalid exception handler node".to_string())
            })?;
            let next_handler = self.assembler.label();
            let handler_location = self.source_location_including_trailing_semicolon(handler.range);
            final_handler_location = handler_location;
            self.set_depth(base_depth + 2);
            let mut not_taken_exclusion = None;
            if let Some(exception_type) = &handler.type_ {
                let mut references = ReferenceCollector::default();
                references.visit_expr(exception_type);
                let newly_owned: Vec<_> = references
                    .references
                    .into_iter()
                    .filter(|name| self.owned_load_locals.insert(name.clone()))
                    .collect();
                self.compile_expression(exception_type)?;
                for name in newly_owned {
                    self.owned_load_locals.remove(&name);
                }
                self.assembler.set_location(handler_location);
                self.emit(CHECK_EXC_MATCH, 0, 0)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, next_handler, -1)?;
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOT_TAKEN, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if handler_index > 0 || matches!(exception_type.as_ref(), Expr::If(_)) {
                    for exclusions in &mut self.active_exception_region_exclusions {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                    not_taken_exclusion = Some((exclusion_start, exclusion_end));
                }
            } else if handler_index + 1 != statement.handlers.len() {
                return Err(CompileError::Internal(
                    "bare except handler must be last".to_string(),
                ));
            }

            self.assembler.set_location(handler_location);
            if let Some(name) = &handler.name {
                self.store_name(name.as_str())?;
            } else {
                self.emit(POP_TOP, 0, -1)?;
            }

            let body_start = self.assembler.label();
            let body_end = self.assembler.label();
            let name_cleanup = self.assembler.label();
            self.assembler.mark(body_start);
            if matches!(handler.body.as_slice(), [Stmt::Continue(_)]) {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                self.assembler
                    .set_location(self.source_location(handler.body[0].range()));
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &handler.name {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                }
                let continue_label = self
                    .loops
                    .last()
                    .map(|context| context.continue_label)
                    .ok_or_else(|| CompileError::Internal("continue outside loop".to_string()))?;
                self.emit_jump_backward(JUMP_BACKWARD, continue_label, 0)?;
                if let Some(name) = &handler.name {
                    self.assembler.add_exception_region(
                        body_start,
                        body_end,
                        name_cleanup,
                        (base_depth + 1).cast_unsigned(),
                        true,
                    );
                    self.assembler.mark(name_cleanup);
                    self.set_depth(base_depth + 3);
                    self.assembler.set_location(SourceLocation::NONE);
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                    self.emit(RERAISE, 1, -3)?;
                }
                self.assembler.mark(next_handler);
                dispatch_start = if handler.name.is_some() {
                    name_cleanup
                } else {
                    next_handler
                };
                continue;
            }
            if handler.name.is_none() && matches!(handler.body.as_slice(), [Stmt::Break(_)]) {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                self.assembler
                    .set_location(self.source_location(handler.body[0].range()));
                self.emit(POP_EXCEPT, 0, -1)?;
                let unreachable_depth = self.depth;
                let context = self
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
                match context.iterator_cleanup {
                    IteratorCleanup::None => {}
                    IteratorCleanup::Sync | IteratorCleanup::Async => {
                        self.emit(POP_TOP, 0, -1)?;
                    }
                }
                if context.break_returns {
                    self.emit_implicit_return()?;
                } else {
                    self.emit_jump_forward(JUMP_BACKWARD, context.break_label, 0)?;
                }
                self.set_depth(unreachable_depth);
                self.assembler.mark(next_handler);
                dispatch_start = next_handler;
                continue;
            }
            if handler.name.is_none()
                && !self.active_pass_finally_locations.is_empty()
                && let [Stmt::Return(return_statement)] = handler.body.as_slice()
                && return_statement
                    .value
                    .as_deref()
                    .is_none_or(|value| matches!(value, Expr::NoneLiteral(_)))
            {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                let return_range = return_statement
                    .value
                    .as_deref()
                    .map_or(return_statement.range, Ranged::range);
                self.assembler
                    .set_location(self.source_location(return_range));
                self.emit(POP_EXCEPT, 0, -1)?;
                let unwind_start = self.assembler.label();
                self.assembler.mark(unwind_start);
                for location in self.active_pass_finally_locations.clone().into_iter().rev() {
                    self.assembler.set_location(location);
                    self.emit(NOP, 0, 0)?;
                }
                self.assembler.set_location(SourceLocation::NONE);
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.emit(RETURN_VALUE, 0, -1)?;
                let unwind_end = self.assembler.label();
                self.assembler.mark(unwind_end);
                for exclusions in &mut self.active_exception_region_exclusions {
                    exclusions.push((unwind_start, unwind_end));
                }
                self.assembler.mark(next_handler);
                dispatch_start = next_handler;
                continue;
            }
            let handler_body_start = self.assembler.instruction_count();
            let newly_owned_handler_locals = self
                .initialized_locals
                .clone()
                .into_iter()
                .filter(|name| self.owned_load_locals.insert(name.clone()))
                .collect::<Vec<_>>();
            let previously_owned = handler
                .name
                .as_ref()
                .is_some_and(|name| !self.owned_load_locals.insert(name.to_string()));
            let terminal_handler_if = if terminal
                && let Some(name) = &handler.name
                && let [Stmt::If(statement)] = handler.body.as_slice()
                && terminal_exception_handler_if_supported(statement)
            {
                Some((statement, name.as_str()))
            } else {
                None
            };
            // CPython terminal-compiles a final nested try while the outer handler fblock is
            // still active, so each implicit return carries the outer handler cleanup.
            let terminal_handler_try = terminal
                && terminal_handler_if.is_none()
                && matches!(handler.body.last(), Some(Stmt::Try(_)));
            let mut terminal_handler_try_return = false;
            let (terminal_handler_exclusions, mut handler_region_exclusions) =
                if let Some((statement, name)) = terminal_handler_if {
                    (
                        Some(self.compile_terminal_exception_handler_if(
                            statement,
                            name,
                            base_depth + 1,
                        )?),
                        Vec::new(),
                    )
                } else {
                    self.active_exception_region_exclusions.push(Vec::new());
                    self.active_exception_handlers
                        .push(ExceptionHandlerContext {
                            name: handler.name.as_ref().map(ToString::to_string),
                            loop_depth: self.loops.len(),
                        });
                    let previous_unwind = std::mem::replace(
                        &mut self.unwind_exception_handlers_for_implicit_return,
                        terminal_handler_try,
                    );
                    let previous_fallthrough = terminal_handler_try
                        .then(|| std::mem::replace(&mut self.emitted_fallthrough_return, false));
                    let strict_owned_loads = matches!(
                        handler.body.last(),
                        Some(Stmt::Break(_) | Stmt::Continue(_))
                    );
                    let result =
                        self.compile_with_strict_owned_loads(strict_owned_loads, |compiler| {
                            if terminal_handler_try {
                                compiler.compile_suite_inner(&handler.body, true)
                            } else {
                                compiler.compile_suite(&handler.body)
                            }
                        });
                    self.unwind_exception_handlers_for_implicit_return = previous_unwind;
                    if let Some(previous_fallthrough) = previous_fallthrough {
                        terminal_handler_try_return = std::mem::replace(
                            &mut self.emitted_fallthrough_return,
                            previous_fallthrough,
                        );
                    }
                    self.active_exception_handlers.pop();
                    result?;
                    (
                        None,
                        self.active_exception_region_exclusions
                            .pop()
                            .expect("exception handler has an exclusion collector"),
                    )
                };
            if let Some(exclusions) = &terminal_handler_exclusions {
                handler_region_exclusions.extend_from_slice(exclusions);
            }
            if let Some(name) = &handler.name
                && !previously_owned
            {
                self.owned_load_locals.remove(name.as_str());
            }
            for name in newly_owned_handler_locals {
                self.owned_load_locals.remove(&name);
            }
            let handler_body_has_instructions =
                self.assembler.instruction_count() > handler_body_start;
            self.assembler.mark(body_end);
            if handler.name.is_some() {
                self.add_exception_regions_with_exclusions(
                    body_start,
                    body_end,
                    &handler_region_exclusions,
                    name_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
            }
            let mut dispatch_exclusions = Vec::new();
            if let Some(exclusion) = not_taken_exclusion {
                dispatch_exclusions.push(exclusion);
            }
            dispatch_exclusions.extend_from_slice(&handler_region_exclusions);
            self.add_exception_regions_with_exclusions(
                dispatch_start,
                body_end,
                &dispatch_exclusions,
                common_cleanup,
                (base_depth + 1).cast_unsigned(),
                true,
            );
            if terminal_handler_exclusions.is_none() && !terminal_handler_try_return {
                if !handler_body_has_instructions && let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                } else if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &handler.name {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                }
                if terminal {
                    self.emit_deferred_implicit_return()?;
                } else {
                    self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                    if !handler_body_has_instructions
                        && !self.active_exception_region_exclusions.is_empty()
                    {
                        self.assembler
                            .exclude_last_instruction_from_exception_if_extended();
                    }
                    let return_is_overridden = self.active_overriding_finally_returns > 0
                        && matches!(handler.body.last(), Some(Stmt::Return(_)));
                    if self.prevent_try_exit_inlining && !return_is_overridden {
                        self.assembler.prevent_last_jump_inlining();
                    }
                }
            }

            if let Some(name) = &handler.name {
                self.assembler.mark(name_cleanup);
                self.set_depth(base_depth + 3);
                self.assembler.set_location(SourceLocation::NONE);
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
                self.emit(RERAISE, 1, -3)?;
            }

            self.assembler.mark(next_handler);
            dispatch_start = if handler.name.is_some() {
                name_cleanup
            } else {
                next_handler
            };
        }

        if statement.handlers.last().is_some_and(|handler| {
            handler
                .as_except_handler()
                .is_some_and(|handler| handler.type_.is_some())
        }) {
            self.set_depth(base_depth + 2);
            self.assembler.set_location(final_handler_location);
            self.emit(RERAISE, 0, -2)?;
            let dispatch_end = self.assembler.label();
            self.assembler.mark(dispatch_end);
            self.assembler.add_exception_region(
                dispatch_start,
                dispatch_end,
                common_cleanup,
                (base_depth + 1).cast_unsigned(),
                true,
            );
        }

        self.assembler.mark(common_cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;

        if statement.handlers.iter().all(|handler| {
            handler.as_except_handler().is_some_and(|handler| {
                matches!(
                    handler.body.last(),
                    Some(Stmt::Break(_) | Stmt::Continue(_))
                )
            })
        }) {
            self.assembler.prevent_borrow_reachability(end);
        }
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_try_star_except(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
        emit_statement_nop: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let try_start = self.assembler.label();
        let try_end = self.assembler.label();
        let handler_start = self.assembler.label();
        let orelse = self.assembler.label();
        let end = self.assembler.label();
        let common_cleanup = self.assembler.label();
        let reraise_star = self.assembler.label();
        let reraise = self.assembler.label();

        if emit_statement_nop {
            let statement_nop_start = self.assembler.label();
            self.assembler.mark(statement_nop_start);
            self.assembler
                .set_location(self.source_location_including_trailing_semicolon(statement.range));
            self.emit(NOP, 0, 0)?;
            let statement_nop_end = self.assembler.label();
            self.assembler.mark(statement_nop_end);
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
        }
        self.assembler.mark(try_start);
        self.active_exception_region_exclusions.push(Vec::new());
        let body_instruction_start = self.assembler.instruction_count();
        self.compile_suite(&statement.body)?;
        let body_noop_location = (self.assembler.instruction_count() == body_instruction_start)
            .then(|| {
                statement
                    .body
                    .last()
                    .map(|statement| self.source_location(statement.range()))
            })
            .flatten();
        let body_fallthrough_location =
            body_noop_location.or_else(|| self.assembler.last_instruction_location());
        let exit_exclusion_start = if body_noop_location.is_some()
            && statement.orelse.is_empty()
            && !self.prevent_try_exit_inlining
            && !self.active_exception_region_exclusions.is_empty()
        {
            let start = self.assembler.label();
            self.assembler.mark(start);
            Some(start)
        } else {
            None
        };
        if !terminal && let Some(location) = body_noop_location {
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
        }
        self.assembler.mark(try_end);
        let try_exclusions = self
            .active_exception_region_exclusions
            .pop()
            .expect("active try-star region has exclusion collector");
        self.assembler.set_location(if terminal {
            body_fallthrough_location.unwrap_or(SourceLocation::NONE)
        } else {
            SourceLocation::NONE
        });
        self.emit_jump_forward(JUMP_FORWARD, orelse, 0)?;
        if let Some(exit_exclusion_start) = exit_exclusion_start {
            let exit_exclusion_end = self.assembler.label();
            self.assembler.mark(exit_exclusion_end);
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((exit_exclusion_start, exit_exclusion_end));
            }
        }
        self.add_exception_regions_with_exclusions(
            try_start,
            try_end,
            &try_exclusions,
            handler_start,
            base_depth.cast_unsigned(),
            false,
        );

        self.assembler.mark(handler_start);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let handler_region_start = handler_start;
        let mut handler_region_exclusions = Vec::new();

        for (handler_index, handler) in statement.handlers.iter().enumerate() {
            let handler = handler.as_except_handler().ok_or_else(|| {
                CompileError::Internal("invalid exception-group handler node".to_string())
            })?;
            let handler_location = self.source_location_including_trailing_semicolon(handler.range);
            let next_handler = self.assembler.label();
            let handler_error = self.assembler.label();
            let no_match = self.assembler.label();
            let body_cleanup = self.assembler.label();

            self.set_depth(base_depth + 2);
            if handler_index == 0 {
                self.assembler.set_location(handler_location);
                self.emit(BUILD_LIST, 0, 1)?;
                self.emit(COPY, 2, 1)?;
            }
            let exception_type = handler.type_.as_ref().ok_or_else(|| {
                CompileError::Internal("exception-group handler has no type".to_string())
            })?;
            self.compile_expression(exception_type)?;
            self.assembler.set_location(handler_location);
            self.emit(CHECK_EG_MATCH, 0, 0)?;
            self.emit(COPY, 1, 1)?;
            self.emit_jump_forward(POP_JUMP_IF_NONE, no_match, -1)?;
            let not_taken_exclusion_start =
                (handler_index > 0 || matches!(exception_type.as_ref(), Expr::If(_))).then(|| {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    start
                });
            self.emit(NOT_TAKEN, 0, 0)?;
            if let Some(start) = not_taken_exclusion_start {
                let end = self.assembler.label();
                self.assembler.mark(end);
                handler_region_exclusions.push((start, end));
            }

            if let Some(name) = &handler.name {
                self.store_name(name.as_str())?;
            } else {
                self.emit(POP_TOP, 0, -1)?;
            }
            let body_start = self.assembler.label();
            self.assembler.mark(body_start);
            let body_instruction_start = self.assembler.instruction_count();
            self.compile_suite(&handler.body)?;
            let body_has_instructions = self.assembler.instruction_count() > body_instruction_start;
            let body_end = self.assembler.label();
            self.assembler.mark(body_end);
            if let Some(name) = &handler.name {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
            }
            let is_last = handler_index + 1 == statement.handlers.len();
            if is_last {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit_jump_forward(JUMP_FORWARD, next_handler, 0)?;
            }

            self.assembler.add_exception_region(
                body_start,
                body_end,
                body_cleanup,
                (base_depth + 4).cast_unsigned(),
                true,
            );
            self.assembler.mark(body_cleanup);
            self.set_depth(base_depth + 6);
            self.assembler.set_location(SourceLocation::NONE);
            if let Some(name) = &handler.name {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
            }
            self.emit(LIST_APPEND, 3, -1)?;
            self.emit(POP_TOP, 0, -1)?;
            if is_last {
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                self.emit_jump_forward(JUMP_FORWARD, handler_error, 0)?;
            }

            if !is_last {
                self.assembler.mark(next_handler);
                self.set_depth(base_depth + 4);
                self.emit(NOP, 0, 0)?;
                self.emit_jump_forward(JUMP_FORWARD, handler_error, 0)?;
            }

            self.assembler.mark(no_match);
            self.set_depth(base_depth + 5);
            self.assembler.set_location(handler_location);
            self.emit(POP_TOP, 0, -1)?;

            self.set_depth(base_depth + 4);
            if is_last {
                self.assembler.set_location(SourceLocation::NONE);
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                self.assembler.mark(handler_error);
            }
        }

        self.assembler.mark(reraise_star);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(CALL_INTRINSIC_2, 1, -1)?;
        self.emit(COPY, 1, 1)?;
        self.emit_jump_forward(POP_JUMP_IF_NOT_NONE, reraise, -1)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.emit(NOT_TAKEN, 0, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        handler_region_exclusions.push((exclusion_start, exclusion_end));
        for exclusions in &mut self.active_exception_region_exclusions {
            exclusions.push((exclusion_start, exclusion_end));
        }
        self.emit(POP_TOP, 0, -1)?;
        let handler_region_end = self.assembler.label();
        self.assembler.mark(handler_region_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(reraise);
        self.set_depth(base_depth + 2);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 0, -1)?;
        self.add_exception_regions_with_exclusions(
            handler_region_start,
            handler_region_end,
            &handler_region_exclusions,
            common_cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(common_cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;

        self.assembler.mark(orelse);
        self.set_depth(base_depth);
        self.compile_suite(&statement.orelse)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        if terminal {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_implicit_return()?;
        }
        Ok(())
    }

    fn compile_terminal_exception_handler_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        name: &str,
        handler_depth: i32,
    ) -> Result<Vec<(Label, Label)>, CompileError> {
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());
        let branch_count = branches.len();
        let mut exclusions = Vec::new();
        let mut last_test: &Expr = statement.test.as_ref();

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            let next = if let Some(test) = test {
                last_test = test;
                let next = self.assembler.label();
                if branch_index > 0 && branch_index + 1 == branch_count && !has_else {
                    self.active_with_region_exclusions.push(Vec::new());
                    self.exclude_terminal_if_not_taken = true;
                    self.compile_jump_if(test, false, next)?;
                    self.exclude_terminal_if_not_taken = false;
                    exclusions.extend(
                        self.active_with_region_exclusions
                            .pop()
                            .expect("terminal handler has exclusion collector"),
                    );
                } else {
                    self.compile_jump_if(test, false, next)?;
                }
                Some(next)
            } else {
                None
            };

            let action_location = if matches!(body, [Stmt::Return(statement)] if statement.value.is_none())
            {
                self.source_location(body[0].range())
            } else {
                let instruction_count = self.assembler.instruction_count();
                self.compile_suite(body)?;
                if self.assembler.instruction_count() > instruction_count {
                    self.assembler
                        .last_instruction_location()
                        .unwrap_or_else(|| self.source_location(last_test.range()))
                } else {
                    body.last().map_or_else(
                        || self.source_location(last_test.range()),
                        |statement| self.source_location(statement.range()),
                    )
                }
            };
            exclusions.push(self.emit_terminal_exception_handler_action(
                name,
                handler_depth,
                action_location,
            )?);

            if let Some(next) = next {
                self.assembler.mark(next);
                self.set_depth(handler_depth);
            }
        }
        if !has_else {
            exclusions.push(self.emit_terminal_exception_handler_action(
                name,
                handler_depth,
                self.source_location(last_test.range()),
            )?);
        }
        Ok(exclusions)
    }

    fn emit_terminal_exception_handler_action(
        &mut self,
        name: &str,
        handler_depth: i32,
        location: SourceLocation,
    ) -> Result<(Label, Label), CompileError> {
        let start = self.assembler.label();
        self.assembler.mark(start);
        self.assembler.set_location(location);
        self.emit(POP_EXCEPT, 0, -1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.store_name(name)?;
        self.delete_name(name)?;
        self.emit_implicit_return()?;
        let end = self.assembler.label();
        self.assembler.mark(end);
        self.set_depth(handler_depth);
        Ok((start, end))
    }

    fn add_exception_regions_with_exclusions(
        &mut self,
        start: Label,
        end: Label,
        exclusions: &[(Label, Label)],
        target: Label,
        depth: u32,
        preserve_lasti: bool,
    ) {
        let mut region_start = start;
        for (exclusion_start, exclusion_end) in exclusions {
            self.assembler.add_exception_region(
                region_start,
                *exclusion_start,
                target,
                depth,
                preserve_lasti,
            );
            region_start = *exclusion_end;
        }
        self.assembler
            .add_exception_region(region_start, end, target, depth, preserve_lasti);
    }

    fn compile_try_finally(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
    ) -> Result<(), CompileError> {
        if statement.handlers.is_empty()
            && statement.orelse.is_empty()
            && matches!(statement.body.as_slice(), [Stmt::Continue(_)])
            && matches!(statement.finalbody.as_slice(), [Stmt::Break(_)])
        {
            return self.compile_try_finally_continue_break(statement);
        }
        let base_depth = self.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let finally_handler = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        if !statement.handlers.is_empty() {
            // Removing the inner try's normal exit leaves an empty CPython CFG block before the
            // normal finally body. Its borrow traversal stops at that empty block.
            self.assembler.prevent_borrow_reachability(protected_end);
        }

        let statement_nop_start = self.assembler.label();
        self.assembler.mark(statement_nop_start);
        self.emit(NOP, 0, 0)?;
        let statement_nop_end = self.assembler.label();
        self.assembler.mark(statement_nop_end);
        for exclusions in &mut self.active_exception_region_exclusions {
            exclusions.push((statement_nop_start, statement_nop_end));
        }
        for exclusions in &mut self.active_with_region_exclusions {
            exclusions.push((statement_nop_start, statement_nop_end));
        }
        if self.generator_region_start.is_some() {
            self.generator_region_exclusions
                .push((statement_nop_start, statement_nop_end));
        }
        self.assembler.mark(protected_start);
        self.active_exception_region_exclusions.push(Vec::new());
        let pass_finally_location = match statement.finalbody.as_slice() {
            [Stmt::Pass(statement)] => Some(self.source_location(statement.range)),
            _ => None,
        };
        if let Some(location) = pass_finally_location {
            self.active_pass_finally_locations.push(location);
        }
        let previous_overriding_finally_returns = self.active_overriding_finally_returns;
        if matches!(statement.finalbody.last(), Some(Stmt::Return(_))) {
            self.active_overriding_finally_returns += 1;
        }
        let previous_break_exit_loop_range = self.preserve_finally_break_exit_loop_range;
        self.preserve_finally_break_exit_loop_range =
            if matches!(statement.finalbody.last(), Some(Stmt::Return(_))) {
                match statement.body.last() {
                    Some(Stmt::While(loop_statement))
                        if loop_statement.orelse.is_empty()
                            && suite_contains_loop_break(&loop_statement.body) =>
                    {
                        Some(loop_statement.range)
                    }
                    Some(Stmt::For(loop_statement))
                        if loop_statement.orelse.is_empty()
                            && suite_contains_loop_break(&loop_statement.body) =>
                    {
                        Some(loop_statement.range)
                    }
                    _ => previous_break_exit_loop_range,
                }
            } else {
                previous_break_exit_loop_range
            };
        let copy_finally_on_return = pass_finally_location.is_none()
            && !suite_terminates(&statement.finalbody)
            && self.active_exception_handlers.is_empty()
            && self.active_return_finally_contexts.is_empty();
        if copy_finally_on_return {
            self.active_return_finally_contexts
                .push(ReturnFinallyContext {
                    body: statement.finalbody.to_vec(),
                    loop_depth: self.loops.len(),
                    handler: finally_handler,
                    depth: base_depth.cast_unsigned(),
                });
        }
        self.active_finally_try_bodies += 1;
        let protected_result = (|| -> Result<bool, CompileError> {
            if statement.handlers.is_empty() {
                let instruction_count = self.assembler.instruction_count();
                if statement.body.len() > 1
                    && let Some(Stmt::Pass(pass)) = statement.body.first()
                {
                    self.assembler
                        .set_location(self.source_location(pass.range));
                    self.emit(NOP, 0, 0)?;
                }
                self.compile_suite(&statement.body)?;
                let has_instructions = self.assembler.instruction_count() > instruction_count;
                if !has_instructions {
                    if let Some(last) = statement.body.last() {
                        self.assembler
                            .set_location(self.source_location(last.range()));
                    }
                    self.emit(NOP, 0, 0)?;
                }
                Ok(has_instructions)
            } else {
                let mut inner = statement.clone();
                inner.finalbody.clear();
                let previous = std::mem::replace(&mut self.prevent_try_exit_inlining, true);
                let result = self.compile_try_inner(&inner, false, false, false);
                self.prevent_try_exit_inlining = previous;
                result?;
                Ok(true)
            }
        })();
        self.active_finally_try_bodies -= 1;
        if copy_finally_on_return {
            self.active_return_finally_contexts.pop();
        }
        self.preserve_finally_break_exit_loop_range = previous_break_exit_loop_range;
        self.active_overriding_finally_returns = previous_overriding_finally_returns;
        let protected_has_instructions = protected_result?;
        if pass_finally_location.is_some() {
            self.active_pass_finally_locations.pop();
        }
        let protected_exclusions = self
            .active_exception_region_exclusions
            .pop()
            .expect("active protected region has exclusion collector");
        self.assembler.mark(protected_end);
        if terminal
            && pass_finally_location.is_some()
            && !statement.handlers.is_empty()
            && suite_terminates(&statement.body)
            && statement.handlers.iter().any(|handler| {
                handler
                    .as_except_handler()
                    .is_some_and(|handler| !suite_terminates(&handler.body))
            })
            && let Some(location) = statement.handlers.iter().rev().find_map(|handler| {
                handler
                    .as_except_handler()
                    .and_then(|handler| handler.body.last())
                    .map(|statement| self.source_location(statement.range()))
            })
        {
            let exit_nop_start = self.assembler.label();
            self.assembler.mark(exit_nop_start);
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
            let exit_nop_end = self.assembler.label();
            self.assembler.mark(exit_nop_end);
            if self.generator_region_start.is_some() {
                self.generator_region_exclusions
                    .push((exit_nop_start, exit_nop_end));
            }
        }
        // `POP_BLOCK` is converted to a NOP before CPython's CFG optimizer runs. It normally
        // disappears, but can retain the sole predecessor's line when a small finally exit is
        // copied onto another edge.
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(NOP, 0, 0)?;
        self.assembler.mark_last_as_converted_pop_block();
        let previous_fallthrough = std::mem::replace(&mut self.emitted_fallthrough_return, false);
        if let Some(location) = pass_finally_location {
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
        } else {
            self.active_normal_finally_bodies += 1;
            let result = self.compile_suite_inner(&statement.finalbody, terminal);
            self.active_normal_finally_bodies -= 1;
            result?;
        }
        let finalbody_emitted_fallthrough =
            std::mem::replace(&mut self.emitted_fallthrough_return, previous_fallthrough);
        if terminal && !finalbody_emitted_fallthrough && !suite_terminates(&statement.finalbody) {
            if let Some(statement) = statement
                .finalbody
                .last()
                .filter(|statement| statement_uses_implicit_return_location(statement))
            {
                self.assembler
                    .set_location(self.source_location(implicit_return_range(self, statement)));
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_implicit_return()?;
        } else if !terminal {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }
        if protected_has_instructions {
            self.add_exception_regions_with_exclusions(
                protected_start,
                protected_end,
                &protected_exclusions,
                finally_handler,
                base_depth.cast_unsigned(),
                false,
            );
        } else {
            self.assembler.add_exception_region(
                protected_end,
                protected_end,
                finally_handler,
                base_depth.cast_unsigned(),
                false,
            );
        }

        self.assembler.mark(finally_handler);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let handler_start = finally_handler;
        self.active_exception_region_exclusions.push(Vec::new());
        let mut explicit_handler_end = None;
        if let Some((Stmt::Return(return_statement), leading)) = statement.finalbody.split_last() {
            self.compile_suite(leading)?;
            explicit_handler_end =
                Some(self.compile_return_from_exception_handler(return_statement)?);
        } else if let [Stmt::If(final_if)] = statement.finalbody.as_slice()
            && final_if.elif_else_clauses.is_empty()
        {
            let condition_false = self.assembler.label();
            self.compile_jump_if(&final_if.test, false, condition_false)?;
            self.active_finally_end_blocks += 1;
            let body_result = self.compile_suite(&final_if.body);
            self.active_finally_end_blocks -= 1;
            body_result?;
            if !suite_terminates(&final_if.body) {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(RERAISE, 0, -2)?;
            }
            self.assembler.mark(condition_false);
            self.set_depth(base_depth + 2);
            self.assembler
                .set_location(self.source_location(final_if.test.range()));
            self.emit(RERAISE, 0, -2)?;
        } else {
            if let Some(location) = pass_finally_location {
                self.assembler.set_location(location);
                self.emit(NOP, 0, 0)?;
            } else {
                self.active_finally_end_blocks += 1;
                let finalbody_result = self.compile_suite(&statement.finalbody);
                self.active_finally_end_blocks -= 1;
                finalbody_result?;
            }
            if !suite_terminates(&statement.finalbody) {
                if let Some(statement) = statement
                    .finalbody
                    .last()
                    .filter(|statement| statement_uses_implicit_return_location(statement))
                {
                    self.assembler
                        .set_location(self.source_location(implicit_return_range(self, statement)));
                } else if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(RERAISE, 0, -2)?;
            }
        }
        let handler_end = if let Some(handler_end) = explicit_handler_end {
            handler_end
        } else {
            let handler_end = self.assembler.label();
            self.assembler.mark(handler_end);
            handler_end
        };
        let handler_exclusions = self
            .active_exception_region_exclusions
            .pop()
            .expect("active finally handler has exclusion collector");
        self.add_exception_regions_with_exclusions(
            handler_start,
            handler_end,
            &handler_exclusions,
            cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_try_finally_continue_break(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth;
        let context = self
            .loops
            .last()
            .copied()
            .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
        let continue_statement = &statement.body[0];
        let break_statement = &statement.finalbody[0];
        let handler = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let empty_protected = self.assembler.label();

        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(NOP, 0, 0)?;
        self.assembler.mark(empty_protected);
        self.assembler.add_exception_region(
            empty_protected,
            empty_protected,
            handler,
            base_depth.cast_unsigned(),
            false,
        );
        self.assembler
            .set_location(self.source_location(continue_statement.range()));
        self.emit(NOP, 0, 0)?;
        self.assembler
            .set_location(self.source_location(break_statement.range()));
        match context.iterator_cleanup {
            IteratorCleanup::None => {}
            IteratorCleanup::Sync | IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
        }
        if context.break_returns {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
        }

        self.assembler.mark(handler);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.assembler
            .set_location(self.source_location(break_statement.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        match context.iterator_cleanup {
            IteratorCleanup::None => {}
            IteratorCleanup::Sync | IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
        }
        if context.break_returns {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
        }
        self.assembler.add_exception_region(
            handler,
            handler_end,
            cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_return_from_exception_handler(
        &mut self,
        statement: &ruff_python_ast::StmtReturn,
    ) -> Result<Label, CompileError> {
        let preserve_value = statement
            .value
            .as_deref()
            .is_some_and(|value| !is_literal_constant(value));
        if preserve_value {
            self.compile_expression(
                statement
                    .value
                    .as_deref()
                    .expect("preserved return has a value"),
            )?;
            self.emit(SWAP, 2, 0)?;
        } else {
            let range = statement
                .value
                .as_deref()
                .map_or(statement.range, Ranged::range);
            self.assembler.set_location(self.source_location(range));
        }

        self.emit(POP_TOP, 0, -1)?;
        if preserve_value {
            self.emit(SWAP, 2, 0)?;
        }
        let handler_end = self.assembler.label();
        self.assembler.mark(handler_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        for context in self.loops.clone().iter().rev() {
            match context.iterator_cleanup {
                IteratorCleanup::None => {}
                IteratorCleanup::Sync | IteratorCleanup::Async => {
                    if preserve_value {
                        self.emit(SWAP, 2, 0)?;
                    }
                    self.emit(POP_TOP, 0, -1)?;
                }
            }
        }

        let pass_finally_unwind_start =
            (!self.active_pass_finally_locations.is_empty()).then(|| {
                let start = self.assembler.label();
                self.assembler.mark(start);
                start
            });
        for location in self.active_pass_finally_locations.clone().into_iter().rev() {
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
        }

        if !preserve_value {
            if let Some(value) = &statement.value {
                self.compile_expression(value)?;
            } else {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
            }
        }
        self.emit(RETURN_VALUE, 0, -1)?;
        if let Some(pass_finally_unwind_start) = pass_finally_unwind_start {
            let unwind_end = self.assembler.label();
            self.assembler.mark(unwind_end);
            let outer_region_count = self
                .active_exception_region_exclusions
                .len()
                .saturating_sub(1);
            for exclusions in self
                .active_exception_region_exclusions
                .iter_mut()
                .take(outer_region_count)
            {
                exclusions.push((pass_finally_unwind_start, unwind_end));
            }
        }
        Ok(handler_end)
    }

    fn compile_with(&mut self, statement: &ruff_python_ast::StmtWith) -> Result<(), CompileError> {
        self.compile_with_items(&statement.items, &statement.body, false)
    }

    fn compile_with_items(
        &mut self,
        items: &[ruff_python_ast::WithItem],
        body: &[Stmt],
        terminal: bool,
    ) -> Result<(), CompileError> {
        let Some((item, remaining)) = items.split_first() else {
            return self.compile_suite(body);
        };
        let base_depth = self.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let handler = self.assembler.label();
        let suppress = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        self.max_depth = self.max_depth.max((base_depth + 7).cast_unsigned());
        let newly_initialized_targets =
            item.optional_vars
                .as_deref()
                .map_or_else(Vec::new, |target| {
                    let mut names = Vec::new();
                    collect_target_names(target, &mut names);
                    names.retain(|name| !self.initialized_locals.contains(name));
                    names
                });

        self.compile_expression(&item.context_expr)?;
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(COPY, 1, 1)?;
        self.emit(LOAD_SPECIAL, 1, 1)?;
        self.emit(SWAP, 2, 0)?;
        self.emit(SWAP, 3, 0)?;
        self.emit(LOAD_SPECIAL, 0, 1)?;
        self.emit(CALL, 0, -1)?;
        self.assembler.mark(protected_start);
        if let Some(target) = &item.optional_vars {
            self.compile_store_target(target)?;
        } else {
            self.emit(POP_TOP, 0, -1)?;
        }
        self.active_with_exits.push(WithExitContext {
            location: self.source_location(item.context_expr.range()),
            is_async: false,
        });
        self.active_with_region_exclusions.push(Vec::new());
        self.active_terminal_withs += usize::from(terminal);
        let mut body_noop = None;
        if remaining.is_empty() {
            let body_start = self.assembler.instruction_count();
            let body_previous_location = self.assembler.last_instruction_location();
            let terminal_if = matches!(
                body.last(),
                Some(Stmt::If(statement))
                    if statement.elif_else_clauses.is_empty()
                        && suite_terminates(&statement.body)
            );
            let terminal_branching = matches!(
                body.last(),
                Some(Stmt::If(statement))
                    if statement
                        .elif_else_clauses
                        .iter()
                        .any(|clause| clause.test.is_some())
            );
            if terminal_if || terminal_branching {
                let (last, leading) = body.split_last().expect("with body has a final if");
                self.compile_suite(leading)?;
                self.exclude_terminal_if_not_taken = true;
                self.compile_statement(last)?;
                self.exclude_terminal_if_not_taken = false;
            } else if !matches!(body, [Stmt::Pass(_)]) {
                // Keep direct pass-only bodies on the with compiler's fallback path. CPython
                // retains that NOP inside the protected range, unlike a terminal pass in a
                // nested suite such as a loop `else`.
                self.compile_suite(body)?;
            }
            let trailing_nop_is_covered = body_previous_location.is_some_and(|previous| {
                previous.line >= 0
                    && self
                        .assembler
                        .last_instruction_location()
                        .is_some_and(|last| last.line == previous.line)
            }) && self.take_trailing_nop_location().is_some();
            if !trailing_nop_is_covered
                && self.assembler.instruction_count() == body_start
                && let Some(statement) = body.last()
            {
                body_noop = Some(statement.range());
            } else if terminal_if && let Some(Stmt::If(statement)) = body.last() {
                body_noop = Some(statement.test.range());
            }
        } else {
            self.compile_with_items(remaining, body, false)?;
        }
        self.active_terminal_withs -= usize::from(terminal);
        let region_exclusions = self
            .active_with_region_exclusions
            .pop()
            .expect("active with statement has region exclusions");
        self.active_with_exits.pop();
        self.assembler.mark(protected_end);
        if !remaining.is_empty() && suite_terminates(body) {
            // A nested manager can suppress the terminal body's exception. CPython keeps the
            // nested item's line marker before running this outer manager's normal exit.
            self.assembler
                .set_location(self.source_location(remaining[0].context_expr.range()));
            self.emit(NOP, 0, 0)?;
        }
        if let Some(range) = body_noop {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler.set_location(self.source_location(range));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            self.generator_region_exclusions
                .push((noop_start, noop_end));
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }

        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        let none = self.add_constant(Constant::None)?;
        if !suite_terminates(body) || !remaining.is_empty() {
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(CALL, 3, -4)?;
            self.emit(POP_TOP, 0, -1)?;
            if terminal {
                self.emit_implicit_return()?;
            } else {
                self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                if self.active_finally_try_bodies > 0 {
                    self.assembler.prevent_last_jump_inlining();
                }
            }
        }
        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                handler,
                (base_depth + 2).cast_unsigned(),
                true,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            handler,
            (base_depth + 2).cast_unsigned(),
            true,
        );

        self.assembler.mark(handler);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.emit(WITH_EXCEPT_START, 0, 1)?;
        self.emit(TO_BOOL, 0, 0)?;
        self.emit_jump_forward(POP_JUMP_IF_TRUE, suppress, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(RERAISE, 2, -5)?;
        self.assembler.mark(suppress);
        self.set_depth(base_depth + 5);
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.assembler.add_exception_region(
            handler,
            handler_end,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if self.active_finally_try_bodies > 0 {
                self.assembler.prevent_last_jump_inlining();
            }
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        for name in newly_initialized_targets {
            self.initialized_locals.remove(&name);
        }
        Ok(())
    }

    fn compile_async_with(
        &mut self,
        statement: &ruff_python_ast::StmtWith,
    ) -> Result<(), CompileError> {
        if self.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("async with outside coroutine code"));
        }
        self.compile_async_with_items(&statement.items, &statement.body, false)
    }

    fn compile_async_with_items(
        &mut self,
        items: &[ruff_python_ast::WithItem],
        body: &[Stmt],
        terminal: bool,
    ) -> Result<(), CompileError> {
        let Some((item, remaining)) = items.split_first() else {
            return self.compile_suite(body);
        };
        let base_depth = self.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let handler = self.assembler.label();
        let suppress = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        let newly_initialized_targets =
            item.optional_vars
                .as_deref()
                .map_or_else(Vec::new, |target| {
                    let mut names = Vec::new();
                    collect_target_names(target, &mut names);
                    names.retain(|name| !self.initialized_locals.contains(name));
                    names
                });

        self.compile_expression(&item.context_expr)?;
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(COPY, 1, 1)?;
        self.emit(LOAD_SPECIAL, 3, 1)?;
        self.emit(SWAP, 2, 0)?;
        self.emit(SWAP, 3, 0)?;
        self.emit(LOAD_SPECIAL, 2, 1)?;
        self.emit(CALL, 0, -1)?;
        self.compile_awaitable_on_stack(1)?;
        self.assembler.mark(protected_start);
        self.active_with_region_exclusions.push(Vec::new());
        if let Some(target) = &item.optional_vars {
            self.compile_store_target(target)?;
        } else {
            self.emit(POP_TOP, 0, -1)?;
        }
        self.active_with_exits.push(WithExitContext {
            location: self.source_location(item.context_expr.range()),
            is_async: true,
        });
        let body_start = self.assembler.instruction_count();
        if remaining.is_empty() {
            if !matches!(body, [Stmt::Pass(_)]) {
                self.compile_suite(body)?;
            }
        } else {
            self.compile_async_with_items(remaining, body, false)?;
        }
        if remaining.is_empty()
            && self.assembler.instruction_count() == body_start
            && let Some(statement) = body.last()
        {
            self.assembler
                .set_location(self.source_location(statement.range()));
            self.emit(NOP, 0, 0)?;
        }
        let mut region_exclusions = self
            .active_with_region_exclusions
            .pop()
            .expect("active async with statement has region exclusions");
        self.active_with_exits.pop();
        if remaining.is_empty()
            && matches!(body, [Stmt::Expr(expression)] if fold_constant(&expression.value).is_some())
        {
            region_exclusions.clear();
        }
        self.assembler.mark(protected_end);

        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(CALL, 3, -4)?;
        self.compile_awaitable_on_stack(2)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }
        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                handler,
                (base_depth + 2).cast_unsigned(),
                true,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            handler,
            (base_depth + 2).cast_unsigned(),
            true,
        );

        self.assembler.mark(handler);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.emit(WITH_EXCEPT_START, 0, 1)?;
        self.compile_awaitable_on_stack(2)?;
        self.emit(TO_BOOL, 0, 0)?;
        self.emit_jump_forward(POP_JUMP_IF_TRUE, suppress, -1)?;
        let not_taken_start = self.assembler.label();
        self.assembler.mark(not_taken_start);
        self.emit(NOT_TAKEN, 0, 0)?;
        let not_taken_end = self.assembler.label();
        self.assembler.mark(not_taken_end);
        self.generator_region_exclusions
            .push((not_taken_start, not_taken_end));
        for exclusions in &mut self.active_with_region_exclusions {
            exclusions.push((not_taken_start, not_taken_end));
        }
        self.emit(RERAISE, 2, -5)?;
        self.assembler.mark(suppress);
        self.set_depth(base_depth + 5);
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.assembler.add_exception_region(
            handler,
            not_taken_start,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.assembler.add_exception_region(
            not_taken_end,
            handler_end,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        for name in newly_initialized_targets {
            self.initialized_locals.remove(&name);
        }
        Ok(())
    }

    fn compile_import(
        &mut self,
        statement: &ruff_python_ast::StmtImport,
    ) -> Result<(), CompileError> {
        for alias in &statement.names {
            if self.constants.is_empty() {
                self.add_constant(Constant::Int(0))?;
            }
            self.emit(LOAD_SMALL_INT, 0, 1)?;
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            let module = alias.name.as_str();
            let module_index = self.name_index(module)?;
            self.emit(IMPORT_NAME, module_index, -1)?;

            if let Some(as_name) = &alias.asname {
                self.compile_import_as(module, as_name.as_str())?;
            } else {
                let bound_name = module.split('.').next().unwrap_or(module);
                self.store_name(bound_name)?;
            }
        }
        Ok(())
    }

    fn compile_import_as(&mut self, module: &str, as_name: &str) -> Result<(), CompileError> {
        let mut parts = module.split('.');
        let _ = parts.next();
        let remaining: Vec<_> = parts.collect();
        if remaining.is_empty() {
            return self.store_name(as_name);
        }

        for (position, part) in remaining.iter().enumerate() {
            let index = self.name_index(part)?;
            self.emit(IMPORT_FROM, index, 1)?;
            if position + 1 < remaining.len() {
                self.emit(Opcode::new(117, 0), 2, 0)?;
                self.emit(POP_TOP, 0, -1)?;
            }
        }
        self.store_name(as_name)?;
        self.emit(POP_TOP, 0, -1)
    }

    fn compile_from_import(
        &mut self,
        statement: &ruff_python_ast::StmtImportFrom,
    ) -> Result<(), CompileError> {
        if self.constants.is_empty() {
            self.add_constant(Constant::Int(u64::from(statement.level)))?;
        }
        self.emit(LOAD_SMALL_INT, statement.level, 1)?;
        let imported_names = statement
            .names
            .iter()
            .map(|alias| Constant::String(alias.name.as_str().to_string()))
            .collect();
        let names = self.add_constant(Constant::Tuple(imported_names))?;
        self.emit(LOAD_CONST, names, 1)?;
        let module = statement.module.as_ref().map_or("", |name| name.as_str());
        let module_index = self.name_index(module)?;
        self.emit(IMPORT_NAME, module_index, -1)?;

        if statement.names.len() == 1 && statement.names[0].name.as_str() == "*" {
            self.emit(CALL_INTRINSIC_1, 2, 0)?;
            self.emit(POP_TOP, 0, -1)?;
            return Ok(());
        }

        for alias in &statement.names {
            let index = self.name_index(alias.name.as_str())?;
            self.emit(IMPORT_FROM, index, 1)?;
            let bound_name = alias
                .asname
                .as_ref()
                .map_or(alias.name.as_str(), |name| name.as_str());
            self.store_name(bound_name)?;
        }
        self.emit(POP_TOP, 0, -1)
    }

    fn compile_type_alias(
        &mut self,
        statement: &ruff_python_ast::StmtTypeAlias,
    ) -> Result<(), CompileError> {
        let Expr::Name(name) = statement.name.as_ref() else {
            return Err(CompileError::Internal(
                "type alias name is not an identifier".to_string(),
            ));
        };
        if let Some(type_params) = statement.type_params.as_deref() {
            return self.compile_generic_type_alias(
                name.id.as_str(),
                &statement.value,
                type_params,
                statement,
            );
        }

        self.assembler
            .set_location(self.source_location(statement.range));
        self.load_string_constant(name.id.as_str())?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.compile_type_parameter_thunk(
            name.id.as_str(),
            &statement.value,
            statement.range,
            self.child_function_is_nested(),
        )?;
        self.emit_build(BUILD_TUPLE, 3)?;
        self.emit(CALL_INTRINSIC_1, 11, 0)?;
        self.store_name(name.id.as_str())
    }

    fn compile_generic_type_alias(
        &mut self,
        name: &str,
        value: &Expr,
        type_params: &ruff_python_ast::TypeParams,
        statement: &ruff_python_ast::StmtTypeAlias,
    ) -> Result<(), CompileError> {
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let locals = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let mut cellvars = type_parameter_dependency_names(type_params, &type_names);
        let mut value_references = ReferenceCollector::default();
        value_references.visit_expr(value);
        cellvars.extend(
            value_references
                .references
                .into_iter()
                .filter(|name| type_names.contains(name)),
        );
        cellvars.extend(
            nested_lambda_required_names_in_expression(value)
                .into_iter()
                .filter(|name| type_names.contains(name)),
        );
        let mut freevars = BTreeSet::new();
        if matches!(self.scope, Scope::Class { .. }) {
            freevars.insert("__classdict__".to_string());
        }
        let plan = FunctionPlan {
            key: (0, 0),
            locals,
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars,
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let wrapper_name = format!("<generic parameters of {name}>");
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.flags & CO_FUTURE_MASK);
        let mut wrapper = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(statement.range.start())),
            plan,
            0,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper.private_name.clone_from(&self.private_name);
        wrapper
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        wrapper.type_parameter_names = type_names;
        wrapper.generic_target_qualified_name = Some(self.child_qualified_name(name));
        wrapper.emit_function_prologue()?;
        wrapper
            .assembler
            .set_location(wrapper.source_location(statement.range));
        wrapper.load_string_constant(name)?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.compile_type_parameter_thunk(name, value, statement.range, true)?;
        wrapper.emit_build(BUILD_TUPLE, 3)?;
        wrapper.emit(CALL_INTRINSIC_1, 11, 0)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;

        let wrapper_closure_names: Vec<_> = wrapper
            .locals
            .iter()
            .zip(&wrapper.local_kinds)
            .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
            .map(|(name, _)| name.clone())
            .collect();
        if !wrapper_closure_names.is_empty() {
            self.emit_closure_tuple(&wrapper_closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !wrapper_closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        self.emit(PUSH_NULL, 0, 1)?;
        self.emit(CALL, 0, -1)?;
        self.store_name(name)
    }

    fn compile_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
    ) -> Result<(), CompileError> {
        if let Some(type_params) = definition.type_params.as_deref() {
            return self.compile_generic_function_definition(definition, type_params);
        }
        self.compile_plain_function_definition(definition, None)
    }

    fn compile_generic_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let definition_location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );
        let defaults = self.compile_function_defaults(definition, definition_location)?;
        let default_argument_count = usize::from(defaults.0) + usize::from(defaults.1);
        if default_argument_count == 2 {
            self.assembler.set_location(definition_location);
            self.emit(SWAP, 2, 0)?;
        }
        let mut inner_definition = definition.clone();
        inner_definition.decorator_list.clear();
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let mut child_plan =
            FunctionPlan::build(definition, self.flags & CO_FUTURE_ANNOTATIONS != 0);
        for name in child_plan.resolve() {
            if type_names.contains(&name) {
                child_plan.freevars.insert(name);
            } else {
                child_plan.mark_global(&name);
            }
        }

        let mut locals = vec![".defaults".to_string()];
        if defaults.1 {
            locals.push(".kwdefaults".to_string());
        }
        locals.extend(
            type_params
                .iter()
                .map(|parameter| parameter.name().as_str().to_string()),
        );
        let mut wrapper_plan = FunctionPlan {
            key: (0, 0),
            locals,
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars: type_parameter_dependency_names(type_params, &type_names),
            freevars: BTreeSet::new(),
            annotation_freevars: BTreeSet::new(),
            children: vec![child_plan],
        };
        for name in wrapper_plan.resolve() {
            wrapper_plan.mark_global(&name);
        }
        if matches!(self.scope, Scope::Class { .. }) && function_has_annotations(definition) {
            wrapper_plan.freevars.insert("__classdict__".to_string());
        }
        let wrapper_name = format!("<generic parameters of {}>", definition.name);
        let target_qualified_name = self.child_qualified_name(definition.name.as_str());
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.flags & CO_FUTURE_MASK);
        let mut wrapper = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    }),
            )),
            wrapper_plan,
            to_u32(
                default_argument_count,
                "generic function default argument count",
            )?,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper.private_name.clone_from(&self.private_name);
        wrapper
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        wrapper.type_parameter_names = type_names;
        wrapper.generic_target_qualified_name = Some(target_qualified_name);
        wrapper.emit_function_prologue()?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.assembler.set_location(definition_location);
        for index in 0..default_argument_count {
            wrapper.emit(
                LOAD_FAST,
                to_u32(index, "generic function default argument index")?,
                1,
            )?;
        }
        if default_argument_count > 0 {
            // CPython inserts closure loads after fusing the synthetic default
            // argument loads, so the last default does not fuse with a cell.
            wrapper.assembler.prevent_last_instruction_fusion();
        }
        wrapper.compile_plain_function_definition(&inner_definition, Some(defaults))?;
        wrapper.emit(SWAP, 2, 0)?;
        wrapper.emit(CALL_INTRINSIC_2, 4, -1)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;

        let wrapper_closure_names: Vec<_> = wrapper
            .locals
            .iter()
            .zip(&wrapper.local_kinds)
            .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
            .map(|(name, _)| name.clone())
            .collect();
        if !wrapper_closure_names.is_empty() {
            self.emit_closure_tuple(&wrapper_closure_names)?;
        }
        self.assembler.set_location(definition_location);
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !wrapper_closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        if default_argument_count == 0 {
            self.emit(PUSH_NULL, 0, 1)?;
            self.emit(CALL, 0, -1)?;
        } else {
            self.emit(
                SWAP,
                to_u32(
                    default_argument_count + 1,
                    "generic function wrapper call depth",
                )?,
                0,
            )?;
            self.emit(
                CALL,
                to_u32(
                    default_argument_count - 1,
                    "generic function wrapper call argument count",
                )?,
                -i32::try_from(default_argument_count).unwrap(),
            )?;
        }
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        self.store_name(definition.name.as_str())
    }

    fn compile_plain_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
        preloaded_defaults: Option<(bool, bool)>,
    ) -> Result<(), CompileError> {
        let parameters = &definition.parameters;
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let definition_location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );

        let (has_positional_defaults, has_keyword_defaults) =
            if let Some(defaults) = preloaded_defaults {
                defaults
            } else {
                self.compile_function_defaults(definition, definition_location)?
            };

        let plan = if let Some(parent) = &self.function_plan {
            parent.child(definition).ok_or_else(|| {
                CompileError::Internal(format!(
                    "missing scope plan for nested function `{}`",
                    definition.name
                ))
            })?
        } else if matches!(self.scope, Scope::Class { .. }) {
            let class_freevars = self.free_names.iter().map(String::as_str).collect();
            FunctionPlan::analyze_in_class(
                definition,
                self.flags & CO_FUTURE_ANNOTATIONS != 0,
                &class_freevars,
            )
        } else {
            FunctionPlan::analyze(definition, self.flags & CO_FUTURE_ANNOTATIONS != 0)
        };
        let closure_names: Vec<_> = plan.freevars.iter().cloned().collect();
        let annotation_child = self.compile_function_annotations(definition, &plan)?;

        let arg_count = to_u32(
            parameters.posonlyargs.len() + parameters.args.len(),
            "function argument count",
        )?;
        let positional_only_arg_count = to_u32(
            parameters.posonlyargs.len(),
            "positional-only argument count",
        )?;
        let keyword_only_arg_count =
            to_u32(parameters.kwonlyargs.len(), "keyword-only argument count")?;
        let mut parameter_flags = (if parameters.vararg.is_some() {
            CO_VARARGS
        } else {
            0
        } | if parameters.kwarg.is_some() {
            CO_VARKEYWORDS
        } else {
            0
        } | if matches!(self.scope, Scope::Function { .. }) {
            CO_NESTED
        } else if matches!(self.scope, Scope::Class { .. }) {
            CO_METHOD
                | if self.class_scope_is_nested {
                    CO_NESTED
                } else {
                    0
                }
        } else {
            0
        }) | (self.flags & CO_FUTURE_MASK);
        if definition.is_async && suite_contains_yield(&definition.body) {
            parameter_flags |= CO_ASYNC_GENERATOR;
        } else if definition.is_async {
            parameter_flags |= CO_COROUTINE;
        } else if suite_contains_yield(&definition.body) {
            parameter_flags |= CO_GENERATOR;
        }
        let qualified_name = self
            .generic_target_qualified_name
            .clone()
            .unwrap_or_else(|| self.child_qualified_name(definition.name.as_str()));
        let first_line_number = self.line_number(u32::from(
            definition
                .decorator_list
                .first()
                .map_or(definition.range.start(), |decorator| {
                    decorator.expression.range().start()
                }),
        ));
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            definition.name.as_str(),
            qualified_name,
            first_line_number,
            plan,
            arg_count,
            positional_only_arg_count,
            keyword_only_arg_count,
            parameter_flags,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        let child = child.compile_function_body(&definition.body)?;

        self.assembler.set_location(definition_location);
        if let Some(annotation_child) = annotation_child {
            let annotation_closure_names: Vec<_> = annotation_child
                .locals
                .iter()
                .zip(&annotation_child.local_kinds)
                .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
                .map(|(name, _)| name.clone())
                .collect();
            if !annotation_closure_names.is_empty() {
                self.emit_closure_tuple(&annotation_closure_names)?;
            }
            let annotation = self.add_constant(Constant::Code(Box::new(annotation_child)))?;
            self.emit(LOAD_CONST, annotation, 1)?;
            self.emit(MAKE_FUNCTION, 0, 0)?;
            if !annotation_closure_names.is_empty() {
                self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
            }
        }
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let constant = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, constant, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        if function_has_annotations(definition) {
            self.emit(SET_FUNCTION_ATTRIBUTE, 16, -1)?;
        }
        if has_keyword_defaults {
            self.emit(SET_FUNCTION_ATTRIBUTE, 2, -1)?;
        }
        if has_positional_defaults {
            self.emit(SET_FUNCTION_ATTRIBUTE, 1, -1)?;
        }
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        if self.generic_target_qualified_name.is_some() {
            Ok(())
        } else {
            self.store_name(definition.name.as_str())
        }
    }

    fn compile_function_defaults(
        &mut self,
        definition: &StmtFunctionDef,
        definition_location: SourceLocation,
    ) -> Result<(bool, bool), CompileError> {
        let parameters = &definition.parameters;
        let positional_defaults: Vec<_> = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .filter_map(|parameter| parameter.default.as_deref())
            .collect();
        if !positional_defaults.is_empty() {
            if let Some(defaults) = positional_defaults
                .iter()
                .map(|default| fold_constant(default))
                .collect::<Option<Vec<_>>>()
            {
                for default in &positional_defaults {
                    self.record_folded_value(default)?;
                }
                self.assembler.set_location(definition_location);
                self.emit_deferred_constant(Constant::Tuple(defaults))?;
            } else {
                for default in &positional_defaults {
                    self.compile_expression(default)?;
                }
                self.assembler.set_location(definition_location);
                self.emit_build(BUILD_TUPLE, positional_defaults.len())?;
            }
        }

        let keyword_defaults: Vec<_> = parameters
            .kwonlyargs
            .iter()
            .filter_map(|parameter| {
                parameter
                    .default
                    .as_deref()
                    .map(|default| (parameter.name().as_str(), default))
            })
            .collect();
        if !keyword_defaults.is_empty() {
            for (name, default) in &keyword_defaults {
                self.assembler.set_location(definition_location);
                let name = self.add_constant(Constant::String(self.mangled_name(name)))?;
                self.emit(LOAD_CONST, name, 1)?;
                self.compile_expression(default)?;
            }
            let count = to_u32(keyword_defaults.len(), "keyword default count")?;
            self.assembler.set_location(definition_location);
            self.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        }

        Ok((
            !positional_defaults.is_empty(),
            !keyword_defaults.is_empty(),
        ))
    }

    fn compile_class_definition(&mut self, definition: &StmtClassDef) -> Result<(), CompileError> {
        if let Some(type_params) = definition.type_params.as_deref() {
            return self.compile_generic_class_definition(definition, type_params);
        }
        self.compile_plain_class_definition(definition)
    }

    fn compile_generic_class_definition(
        &mut self,
        definition: &StmtClassDef,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let mut inner_definition = definition.clone();
        inner_definition.decorator_list.clear();
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let mut locals: Vec<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        locals.push(".generic_base".to_string());
        locals.push(".type_params".to_string());
        let required_names =
            class_required_names(definition, self.flags & CO_FUTURE_ANNOTATIONS != 0);
        let mut cellvars: HashSet<_> = type_names.intersection(&required_names).cloned().collect();
        cellvars.insert(".type_params".to_string());
        let plan = FunctionPlan {
            key: (0, 0),
            locals,
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars,
            freevars: BTreeSet::new(),
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let wrapper_name = format!("<generic parameters of {}>", definition.name);
        let target_qualified_name = self.child_qualified_name(definition.name.as_str());
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.flags & CO_FUTURE_MASK);
        let mut wrapper = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    }),
            )),
            plan,
            0,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper.private_name.clone_from(&self.private_name);
        wrapper
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        wrapper.type_parameter_names = type_names;
        wrapper.generic_target_qualified_name = Some(target_qualified_name);
        wrapper.emit_function_prologue()?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.assembler.set_location(wrapper.definition_location(
            definition.range,
            definition.name.range(),
            b"class",
            false,
        ));
        wrapper.store_name(".type_params")?;
        wrapper.compile_plain_class_definition(&inner_definition)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;

        let definition_location =
            self.definition_location(definition.range, definition.name.range(), b"class", false);
        self.assembler.set_location(definition_location);
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        self.emit(PUSH_NULL, 0, 1)?;
        self.emit(CALL, 0, -1)?;
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        self.store_name(definition.name.as_str())
    }

    fn compile_plain_class_definition(
        &mut self,
        definition: &StmtClassDef,
    ) -> Result<(), CompileError> {
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let is_generic = self.generic_target_qualified_name.is_some();

        let mut globals = LocalCollector::default();
        globals.collect_globals(&definition.body);
        let mut freevars =
            class_required_names(definition, self.flags & CO_FUTURE_ANNOTATIONS != 0)
                .into_iter()
                .filter(|name| match &self.scope {
                    Scope::Module => false,
                    Scope::Class { free_indices, .. } => {
                        self.cell_names.contains(name) || free_indices.contains_key(name)
                    }
                    Scope::Function {
                        indices,
                        free_indices,
                        cells,
                        ..
                    } => {
                        indices.contains_key(name) && cells.contains(name)
                            || free_indices.contains_key(name)
                    }
                })
                .collect::<BTreeSet<_>>();
        if is_generic {
            freevars.insert(".type_params".to_string());
        }
        let closure_names = freevars.iter().cloned().collect::<Vec<_>>();
        let qualified_name = self
            .generic_target_qualified_name
            .clone()
            .unwrap_or_else(|| self.child_qualified_name(definition.name.as_str()));
        let first_line_number = self.line_number(u32::from(
            definition
                .decorator_list
                .first()
                .map_or(definition.range.start(), |decorator| {
                    decorator.expression.range().start()
                }),
        ));
        let needs_class_closure =
            class_needs_class_closure(&definition.body, self.flags & CO_FUTURE_ANNOTATIONS != 0);
        let needs_classdict = contains_type_alias(&definition.body)
            || self.flags & CO_FUTURE_ANNOTATIONS == 0
                && (contains_function_definition(&definition.body)
                    || has_simple_annotations(&definition.body));
        let class_scope_is_nested = self.child_function_is_nested();
        let mut child = Self::class(
            &self.filename,
            Arc::clone(&self.source),
            definition.name.as_str(),
            qualified_name,
            first_line_number,
            globals.globals,
            globals.nonlocals,
            freevars,
            needs_class_closure,
            needs_classdict,
            self.flags & CO_FUTURE_MASK,
        );
        child.class_scope_is_nested = class_scope_is_nested;
        child
            .type_parameter_names
            .clone_from(&self.type_parameter_names);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        let child = child.compile_class_body(&definition.body)?;

        let definition_location =
            self.definition_location(definition.range, definition.name.range(), b"class", false);
        self.assembler.set_location(definition_location);
        self.emit(LOAD_BUILD_CLASS, 0, 1)?;
        self.emit(PUSH_NULL, 0, 1)?;
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        let name = self.add_constant(Constant::String(definition.name.as_str().to_string()))?;
        self.emit(LOAD_CONST, name, 1)?;
        if is_generic {
            self.load_name(".type_params")?;
            self.emit(CALL_INTRINSIC_1, 10, 0)?;
            self.store_name(".generic_base")?;
        }

        let arguments = definition.arguments.as_deref();
        let has_unpacking = arguments.is_some_and(|arguments| {
            arguments.args.iter().any(Expr::is_starred_expr)
                || arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.is_none())
        });
        let has_starred_arguments =
            arguments.is_some_and(|arguments| arguments.args.iter().any(Expr::is_starred_expr));
        let argument_count = arguments.map_or(0, |arguments| {
            arguments.args.len() + arguments.keywords.len()
        }) + usize::from(is_generic);
        if has_unpacking {
            // Extended class calls include the body function and class name in their positional
            // tuple. CPython packs those together with every base before the first starred base.
            if has_starred_arguments {
                let leading_count = arguments.map_or(0, |arguments| {
                    arguments
                        .args
                        .iter()
                        .take_while(|argument| !argument.is_starred_expr())
                        .count()
                });
                if let Some(arguments) = arguments {
                    for argument in arguments.args.iter().take(leading_count) {
                        self.compile_expression(argument)?;
                    }
                }
                self.emit_build(BUILD_LIST, leading_count + 2)?;
                for argument in arguments
                    .expect("starred class arguments require an argument list")
                    .args
                    .iter()
                    .skip(leading_count)
                {
                    if let Expr::Starred(starred) = argument {
                        self.compile_expression(&starred.value)?;
                        self.emit(LIST_EXTEND, 1, -1)?;
                    } else {
                        self.compile_expression(argument)?;
                        self.emit(LIST_APPEND, 1, -1)?;
                    }
                }
                if is_generic {
                    self.load_name(".generic_base")?;
                    self.emit(LIST_APPEND, 1, -1)?;
                }
                self.emit(CALL_INTRINSIC_1, 6, 0)?;
            } else {
                if let Some(arguments) = arguments {
                    for argument in &arguments.args {
                        self.compile_expression(argument)?;
                    }
                }
                if is_generic {
                    self.load_name(".generic_base")?;
                }
                let positional_count =
                    arguments.map_or(0, |arguments| arguments.args.len()) + usize::from(is_generic);
                self.emit_build(BUILD_TUPLE, positional_count + 2)?;
            }
            if let Some(arguments) = arguments.filter(|arguments| !arguments.keywords.is_empty()) {
                let mut have_dict = false;
                let mut named_start = 0;
                for (index, keyword) in arguments.keywords.iter().enumerate() {
                    if keyword.arg.is_some() {
                        continue;
                    }
                    if named_start < index {
                        self.compile_class_keyword_map(
                            &arguments.keywords[named_start..index],
                            definition_location,
                        )?;
                        if have_dict {
                            self.emit(DICT_MERGE, 1, -1)?;
                        }
                        have_dict = true;
                    }
                    if !have_dict {
                        self.emit(BUILD_MAP, 0, 1)?;
                        have_dict = true;
                    }
                    self.compile_expression(&keyword.value)?;
                    self.emit(DICT_MERGE, 1, -1)?;
                    named_start = index + 1;
                }
                if named_start < arguments.keywords.len() {
                    self.compile_class_keyword_map(
                        &arguments.keywords[named_start..],
                        definition_location,
                    )?;
                    if have_dict {
                        self.emit(DICT_MERGE, 1, -1)?;
                    }
                }
            } else {
                self.emit(PUSH_NULL, 0, 1)?;
            }
            self.assembler.set_location(definition_location);
            self.emit(CALL_FUNCTION_EX, 0, -3)?;
        } else {
            if let Some(arguments) = arguments {
                for argument in &arguments.args {
                    self.compile_expression(argument)?;
                }
            }
            if is_generic {
                self.load_name(".generic_base")?;
            }
            if let Some(arguments) = arguments {
                for keyword in &arguments.keywords {
                    self.compile_expression(&keyword.value)?;
                }
            }
            self.assembler.set_location(definition_location);
            if let Some(arguments) = arguments.filter(|arguments| !arguments.keywords.is_empty()) {
                let keyword_names = arguments
                    .keywords
                    .iter()
                    .map(|keyword| {
                        Constant::String(keyword.arg.as_ref().unwrap().as_str().to_string())
                    })
                    .collect();
                let names = self.add_interned_string_tuple(keyword_names)?;
                self.emit(LOAD_CONST, names, 1)?;
                let count = to_u32(argument_count + 2, "class argument count")?;
                self.emit(CALL_KW, count, -i32::try_from(argument_count).unwrap() - 4)?;
            } else {
                let count = to_u32(argument_count + 2, "class argument count")?;
                self.emit(CALL, count, -i32::try_from(argument_count).unwrap() - 3)?;
            }
        }
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        if self.generic_target_qualified_name.is_some() {
            Ok(())
        } else {
            self.store_name(definition.name.as_str())
        }
    }

    fn compile_class_keyword_map(
        &mut self,
        keywords: &[Keyword],
        definition_location: SourceLocation,
    ) -> Result<(), CompileError> {
        debug_assert!(!keywords.is_empty() && keywords.iter().all(|keyword| keyword.arg.is_some()));
        for keyword in keywords {
            let name = keyword.arg.as_ref().unwrap();
            let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
            self.emit(LOAD_CONST, key, 1)?;
            self.compile_expression(&keyword.value)?;
        }
        self.assembler.set_location(definition_location);
        self.emit_build_map(keywords.len())
    }

    fn compile_type_parameters(
        &mut self,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for parameter in type_params {
            let name = parameter.name().as_str();
            self.assembler
                .set_location(self.source_location(parameter.range()));
            self.load_string_constant(name)?;
            match parameter {
                TypeParam::TypeVar(parameter) => {
                    if let Some(bound) = &parameter.bound {
                        self.compile_type_parameter_thunk(name, bound, bound.range(), true)?;
                        self.assembler
                            .set_location(self.source_location(parameter.range()));
                        self.emit(
                            CALL_INTRINSIC_2,
                            if matches!(bound.as_ref(), Expr::Tuple(_)) {
                                3
                            } else {
                                2
                            },
                            -1,
                        )?;
                    } else {
                        self.emit(CALL_INTRINSIC_1, 7, 0)?;
                    }
                }
                TypeParam::ParamSpec(_) => self.emit(CALL_INTRINSIC_1, 8, 0)?,
                TypeParam::TypeVarTuple(_) => self.emit(CALL_INTRINSIC_1, 9, 0)?,
            }
            if let Some(default) = parameter.default() {
                self.compile_type_parameter_thunk(name, default, default.range(), true)?;
                self.assembler
                    .set_location(self.source_location(parameter.range()));
                self.emit(CALL_INTRINSIC_2, 5, -1)?;
            }
            self.emit(COPY, 1, 1)?;
            self.store_name(name)?;
        }
        if let Some(first) = type_params.first() {
            self.assembler
                .set_location(self.source_location(first.range()));
        }
        self.emit_build(BUILD_TUPLE, type_params.len())
    }

    fn compile_type_parameter_thunk(
        &mut self,
        name: &str,
        expression: &Expr,
        setup_range: ruff_text_size::TextRange,
        nested: bool,
    ) -> Result<(), CompileError> {
        let mut references = ReferenceCollector::default();
        references.visit_expr(expression);
        let mut freevars: BTreeSet<_> = references
            .references
            .iter()
            .filter(|name| self.type_parameter_names.contains(*name))
            .cloned()
            .collect();
        freevars.extend(
            nested_lambda_required_names_in_expression(expression)
                .into_iter()
                .filter(|name| self.type_parameter_names.contains(name)),
        );
        let can_see_class_scope = matches!(self.scope, Scope::Class { .. })
            || self.free_names.iter().any(|name| name == "__classdict__");
        if can_see_class_scope {
            freevars.insert("__classdict__".to_string());
        }
        let globals = references
            .references
            .difference(&freevars.iter().cloned().collect())
            .cloned()
            .collect();
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let plan = FunctionPlan {
            key: (0, 0),
            locals: vec![".format".to_string()],
            globals,
            nonlocals: HashSet::new(),
            references: references.references,
            annotation_references: HashSet::new(),
            cellvars: HashSet::new(),
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        // Definitions inside an annotation scope are qualified against that
        // scope's parent. Generic-parameter wrappers are themselves annotation
        // scopes even though this compiler represents them as functions.
        let child_qualified_name_parent = if self.generic_target_qualified_name.is_some() {
            self.qualified_name.clone()
        } else {
            match self.scope {
                Scope::Module => String::new(),
                Scope::Class { .. } => self.qualified_name.clone(),
                Scope::Function { .. } => format!("{}.<locals>", self.qualified_name),
            }
        };
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            name,
            self.generic_target_qualified_name.as_ref().map_or_else(
                || self.child_qualified_name(name),
                |qualified_name| {
                    qualified_name.rsplit_once('.').map_or_else(
                        || name.to_string(),
                        |(parent, _)| format!("{parent}.{name}"),
                    )
                },
            ),
            self.line_number(u32::from(setup_range.start())),
            plan,
            1,
            1,
            0,
            (if nested { CO_NESTED } else { 0 }) | (self.flags & CO_FUTURE_MASK),
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        child.child_qualified_name_parent = Some(child_qualified_name_parent);
        if can_see_class_scope {
            child.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        }
        child.emit_function_prologue()?;
        child
            .assembler
            .set_location(child.source_location(setup_range));
        child.emit_annotation_format_guard()?;
        child.compile_expression(expression)?;
        child
            .assembler
            .set_location(child.source_location(setup_range));
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;

        self.assembler
            .set_location(self.source_location(setup_range));
        let defaults = self.add_constant(Constant::Tuple(vec![Constant::Int(1)]))?;
        self.emit(LOAD_CONST, defaults, 1)?;
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        self.emit(SET_FUNCTION_ATTRIBUTE, 1, -1)
    }

    fn emit_annotation_format_guard(&mut self) -> Result<(), CompileError> {
        let parameter = self
            .locals
            .first()
            .map_or_else(|| "format".to_string(), Clone::clone);
        self.load_name(&parameter)?;
        self.add_constant(Constant::Int(2))?;
        self.emit(LOAD_SMALL_INT, 2, 1)?;
        self.emit(COMPARE_OP, 132, -1)?;
        let supported = self.assembler.label();
        self.emit_jump_forward(POP_JUMP_IF_FALSE, supported, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(LOAD_COMMON_CONSTANT, 1, 1)?;
        self.emit(RAISE_VARARGS, 1, -1)?;
        self.assembler.mark(supported);
        self.set_depth(0);
        Ok(())
    }

    fn compile_function_annotations(
        &self,
        definition: &StmtFunctionDef,
        function_plan: &FunctionPlan,
    ) -> Result<Option<CodeObject>, CompileError> {
        let parameters = &definition.parameters;
        let mut annotations = Vec::<(String, &Expr)>::new();
        for parameter in parameters.args.iter().chain(&parameters.posonlyargs) {
            if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                annotations.push((parameter.name().as_str().to_string(), annotation));
            }
        }
        if let Some(parameter) = &parameters.vararg
            && let Some(annotation) = parameter.annotation.as_deref()
        {
            annotations.push((parameter.name.as_str().to_string(), annotation));
        }
        for parameter in &parameters.kwonlyargs {
            if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                annotations.push((parameter.name().as_str().to_string(), annotation));
            }
        }
        if let Some(parameter) = &parameters.kwarg
            && let Some(annotation) = parameter.annotation.as_deref()
        {
            annotations.push((parameter.name.as_str().to_string(), annotation));
        }
        if let Some(annotation) = definition.returns.as_deref() {
            annotations.push(("return".to_string(), annotation));
        }
        if annotations.is_empty() {
            return Ok(None);
        }

        let mut freevars = function_plan.annotation_freevars.clone();
        if self.flags & CO_FUTURE_ANNOTATIONS == 0 {
            freevars.extend(
                self.type_parameter_names
                    .iter()
                    .filter(|name| function_plan.annotation_references.contains(*name))
                    .cloned(),
            );
        }
        let can_see_class_scope = self.flags & CO_FUTURE_ANNOTATIONS == 0
            && (matches!(self.scope, Scope::Class { .. })
                || self.free_names.iter().any(|name| name == "__classdict__"));
        if can_see_class_scope {
            freevars.insert("__classdict__".to_string());
        }
        let plan = FunctionPlan {
            key: (0, 0),
            locals: vec!["format".to_string()],
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars: HashSet::new(),
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let parameter_flags = (if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        }) | (self.flags & CO_FUTURE_MASK);
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            "__annotate__",
            self.generic_target_qualified_name.as_ref().map_or_else(
                || self.child_qualified_name("__annotate__"),
                |qualified_name| {
                    qualified_name.rsplit_once('.').map_or_else(
                        || "__annotate__".to_string(),
                        |(parent, _)| format!("{parent}.__annotate__"),
                    )
                },
            ),
            self.line_number(u32::from(definition.name.range().start())),
            plan,
            1,
            1,
            0,
            parameter_flags,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        child.annotation_thunk = true;
        if can_see_class_scope {
            child.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        }
        child.emit_function_prologue()?;
        let location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );
        child.assembler.set_location(location);

        child.emit_annotation_format_guard()?;

        let future_annotations = self.flags & CO_FUTURE_ANNOTATIONS != 0;
        for (name, annotation) in &annotations {
            child.assembler.set_location(location);
            let name = child.mangled_name(name);
            child.load_string_constant(&name)?;
            if future_annotations {
                child
                    .assembler
                    .set_location(child.source_location(annotation.range()));
                child.load_string_constant(&unparse_annotation(annotation))?;
            } else if let Expr::Starred(starred) = annotation {
                child.compile_expression(&starred.value)?;
                child.assembler.set_location(location);
                child.emit(UNPACK_SEQUENCE, 1, 0)?;
            } else {
                child.compile_expression(annotation)?;
            }
        }
        child.assembler.set_location(location);
        let count = to_u32(annotations.len(), "annotation count")?;
        child.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        child.emit(RETURN_VALUE, 0, -1)?;
        child.finish_inner(false).map(Some)
    }

    fn compile_class_annotations(
        &self,
        body: &[Stmt],
    ) -> Result<Option<(CodeObject, Vec<String>)>, CompileError> {
        let mut annotations = Vec::new();
        collect_simple_annotations(body, &mut annotations);
        if annotations.is_empty() {
            return Ok(None);
        }

        let mut references = ReferenceCollector::default();
        for annotation in &annotations {
            references.visit_expr(&annotation.annotation);
        }
        let available_freevars: HashSet<_> = self.free_names.iter().cloned().collect();
        let mut freevars: BTreeSet<_> = references
            .references
            .iter()
            .filter(|name| available_freevars.contains(*name))
            .cloned()
            .collect();
        freevars.insert("__classdict__".to_string());
        let globals = references
            .references
            .iter()
            .filter(|name| !freevars.contains(*name))
            .cloned()
            .collect();
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let plan = FunctionPlan {
            key: (0, 0),
            locals: vec!["format".to_string()],
            globals,
            nonlocals: HashSet::new(),
            references: references.references,
            annotation_references: HashSet::new(),
            cellvars: HashSet::new(),
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let parameter_flags = if self.child_function_is_nested() || !self.free_names.is_empty() {
            CO_NESTED
        } else {
            0
        };
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            "__annotate__",
            self.child_qualified_name("__annotate__"),
            self.first_line_number,
            plan,
            1,
            1,
            0,
            parameter_flags,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        child.annotation_thunk = true;
        child.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        child.emit_function_prologue()?;
        let line = i32::try_from(self.first_line_number).unwrap_or(i32::MAX);
        let setup_location = SourceLocation::new(line, line, 0, 0);
        child.assembler.set_location(setup_location);
        child.emit_annotation_format_guard()?;
        child.emit(BUILD_MAP, 0, 1)?;

        for annotation in annotations {
            let Expr::Name(target) = annotation.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            child
                .assembler
                .set_location(child.source_location(annotation.annotation.range()));
            child.compile_expression(&annotation.annotation)?;
            child
                .assembler
                .set_location(child.source_location(annotation.range));
            child.emit(COPY, 2, 1)?;
            let target = child.mangled_name(target.id.as_str());
            child.load_string_constant(&target)?;
            child.assembler.set_location(setup_location);
            child.emit(STORE_SUBSCR, 0, -3)?;
        }
        child.assembler.set_location(setup_location);
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;
        Ok(Some((child, closure_names)))
    }

    fn compile_module_annotations(&self, body: &[Stmt]) -> Result<CodeObject, CompileError> {
        let mut annotations = Vec::new();
        collect_simple_annotations(body, &mut annotations);
        let first = annotations.first().ok_or_else(|| {
            CompileError::Internal("module annotation thunk has no annotations".to_string())
        })?;
        let plan = FunctionPlan {
            key: (0, 0),
            locals: vec!["format".to_string()],
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars: HashSet::new(),
            freevars: BTreeSet::new(),
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            "__annotate__",
            "__annotate__".to_string(),
            self.line_number(u32::from(
                body.first().map_or(first.range, Ranged::range).start(),
            )),
            plan,
            1,
            1,
            0,
            0,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        child.annotation_thunk = true;
        child.emit_function_prologue()?;
        let setup_location = self.source_location(body.first().map_or(first.range, Ranged::range));
        child.assembler.set_location(setup_location);
        child.load_name("format")?;
        child.add_constant(Constant::Int(2))?;
        child.emit(LOAD_SMALL_INT, 2, 1)?;
        child.emit(COMPARE_OP, 132, -1)?;
        let supported = child.assembler.label();
        child.emit_jump_forward(POP_JUMP_IF_FALSE, supported, -1)?;
        child.emit(NOT_TAKEN, 0, 0)?;
        child.emit(LOAD_COMMON_CONSTANT, 1, 1)?;
        child.emit(RAISE_VARARGS, 1, -1)?;
        child.assembler.mark(supported);
        child.set_depth(0);
        child.emit(BUILD_MAP, 0, 1)?;

        for (index, annotation) in annotations.iter().enumerate() {
            let Expr::Name(target) = annotation.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            let location = self.source_location(annotation.range);
            child.assembler.set_location(location);
            child.emit(LOAD_SMALL_INT, to_u32(index, "module annotation index")?, 1)?;
            child.load_name("__conditional_annotations__")?;
            child.emit(CONTAINS_OP, 0, -1)?;
            let skip = child.assembler.label();
            child.emit_jump_forward(POP_JUMP_IF_FALSE, skip, -1)?;
            child.emit(NOT_TAKEN, 0, 0)?;
            child.compile_expression(&annotation.annotation)?;
            child.assembler.set_location(location);
            child.emit(COPY, 2, 1)?;
            child.load_string_constant(target.id.as_str())?;
            child.assembler.set_location(setup_location);
            child.emit(STORE_SUBSCR, 0, -3)?;
            child.assembler.mark(skip);
            child.set_depth(1);
        }
        child.assembler.set_location(setup_location);
        child.emit(RETURN_VALUE, 0, -1)?;
        child.finish_inner(false)
    }

    fn compile_expression(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = self.compile_expression_inner(expression);
        self.assembler.set_location(previous);
        result
    }

    fn compile_expression_inner(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let starting_depth = self.depth;
        if matches!(expression, Expr::BoolOp(_))
            && let Some((origin, constant)) = folded_bool_operand(expression)
        {
            self.pre_register_expression_names(expression)?;
            let expression_location = self.source_location(expression.range());
            let origin_location = self.source_location(origin.range());
            if expression_location.line != origin_location.line {
                self.assembler.set_location(expression_location);
                self.emit(NOP, 0, 0)?;
            }
            self.assembler.set_location(origin_location);
            self.emit_folded_constant(origin, constant)?;
            return Ok(());
        }
        if matches!(
            expression,
            Expr::BinOp(_) | Expr::Name(_) | Expr::Subscript(_) | Expr::UnaryOp(_) | Expr::Tuple(_)
        ) && let Some(constant) = fold_constant(expression)
        {
            self.emit_folded_constant(expression, constant)?;
            return Ok(());
        }
        match expression {
            Expr::Name(name) if name.ctx == ExprContext::Load => {
                self.load_name(name.id.as_str())?;
            }
            Expr::NoneLiteral(_) => {
                let index = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BooleanLiteral(boolean) => {
                let index = self.add_constant(Constant::Bool(boolean.value))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::EllipsisLiteral(_) => {
                let index = self.add_constant(Constant::Ellipsis)?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::NumberLiteral(number) => {
                let constant = match &number.value {
                    Number::Int(value) => value.as_u64().map_or_else(
                        || Constant::BigInt {
                            negative: false,
                            digits: big_integer_digits(&value.to_string()),
                        },
                        Constant::Int,
                    ),
                    Number::Float(value) => Constant::Float(*value),
                    Number::Complex { real, imag } => Constant::Complex {
                        real: *real,
                        imag: *imag,
                    },
                };
                if let Number::Int(value) = &number.value
                    && let Some(value) = value.as_u8()
                {
                    if self.constants.is_empty() {
                        self.add_constant(constant)?;
                    }
                    self.emit(LOAD_SMALL_INT, u32::from(value), 1)?;
                } else {
                    let index = self.add_constant(constant)?;
                    self.emit(LOAD_CONST, index, 1)?;
                }
            }
            Expr::StringLiteral(string) => {
                let index = self.add_constant(self.string_literal_constant(string))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BytesLiteral(bytes) => {
                let index = self.add_constant(Constant::Bytes(bytes.value.bytes().collect()))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BinOp(binary) => {
                if !self.compile_optimized_percent_format(binary)? {
                    self.compile_expression(&binary.left)?;
                    self.compile_expression(&binary.right)?;
                    self.emit(BINARY_OP, binary_operator(binary.op, false), -1)?;
                }
            }
            Expr::UnaryOp(unary) => {
                self.compile_expression(&unary.operand)?;
                match unary.op {
                    UnaryOp::Invert => self.emit(UNARY_INVERT, 0, 0)?,
                    UnaryOp::Not => {
                        self.emit(TO_BOOL, 0, 0)?;
                        self.emit(UNARY_NOT, 0, 0)?;
                    }
                    UnaryOp::UAdd => self.emit(CALL_INTRINSIC_1, 5, 0)?,
                    UnaryOp::USub => self.emit(UNARY_NEGATIVE, 0, 0)?,
                }
            }
            Expr::Lambda(lambda) => self.compile_lambda(lambda)?,
            Expr::Call(call) => {
                self.compile_call(call)?;
            }
            Expr::Attribute(attribute) if attribute.ctx == ExprContext::Load => {
                if !self.compile_super_attribute(attribute, false)? {
                    self.compile_expression(&attribute.value)?;
                    let index = self.name_index(attribute.attr.as_str())?;
                    self.assembler
                        .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                    self.emit(LOAD_ATTR, index << 1, 0)?;
                }
            }
            Expr::List(list) if list.ctx == ExprContext::Load => {
                self.compile_collection(&list.elts, CollectionKind::List)?;
            }
            Expr::Tuple(tuple) if tuple.ctx == ExprContext::Load => {
                self.compile_collection(&tuple.elts, CollectionKind::Tuple)?;
            }
            Expr::Set(set) => {
                self.compile_collection(&set.elts, CollectionKind::Set)?;
            }
            Expr::Dict(dict) => {
                self.compile_dict(&dict.items)?;
            }
            Expr::Subscript(subscript) if subscript.ctx == ExprContext::Load => {
                self.compile_expression(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref() {
                    let slice_location = self.source_location(slice.range);
                    let subscript_location = self.source_location(subscript.range);
                    if let Some(constant) = constant_slice(slice) {
                        let index = self.add_constant(constant)?;
                        self.assembler.set_location(slice_location);
                        self.emit(LOAD_CONST, index, 1)?;
                        self.assembler.set_location(subscript_location);
                        self.emit(BINARY_OP, 26, -1)?;
                    } else if slice.step.is_none() {
                        self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                        self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                        self.assembler.set_location(subscript_location);
                        self.emit(BINARY_SLICE, 0, -2)?;
                    } else {
                        self.compile_expression(&subscript.slice)?;
                        self.emit(BINARY_OP, 26, -1)?;
                    }
                } else {
                    self.compile_expression(&subscript.slice)?;
                    self.emit(BINARY_OP, 26, -1)?;
                }
            }
            Expr::If(expression) => self.compile_if_expression(expression)?,
            Expr::BoolOp(expression) => self.compile_bool_expression(expression)?,
            Expr::Compare(expression) => self.compile_compare_expression(expression)?,
            Expr::Named(expression) => {
                self.compile_expression(&expression.value)?;
                self.emit(COPY, 1, 1)?;
                self.compile_store_target(&expression.target)?;
            }
            Expr::Slice(slice) => self.compile_slice(slice)?,
            Expr::FString(fstring) => self.compile_fstring(fstring)?,
            Expr::ListComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                None,
                &comprehension.elt,
                BUILD_LIST,
                LIST_APPEND,
                false,
            )?,
            Expr::SetComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                None,
                &comprehension.elt,
                BUILD_SET,
                SET_ADD,
                false,
            )?,
            Expr::DictComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                comprehension.key.as_deref(),
                &comprehension.value,
                BUILD_MAP,
                MAP_ADD,
                false,
            )?,
            Expr::Generator(generator) => self.compile_generator_expression(generator)?,
            Expr::Yield(expression) => {
                if self.flags & (CO_GENERATOR | CO_ASYNC_GENERATOR) == 0 {
                    return Err(unsupported("yield outside generator code"));
                }
                if let Some(value) = &expression.value {
                    self.compile_expression(value)?;
                } else {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                }
                self.assembler
                    .set_location(self.source_location(expression.range));
                if self.flags & CO_ASYNC_GENERATOR != 0 {
                    self.emit(CALL_INTRINSIC_1, 4, 0)?;
                }
                self.emit(YIELD_VALUE, 0, 0)?;
                let wrapper_is_only_exception_region = self.generator_region_start.is_some()
                    && self.active_exception_region_exclusions.is_empty()
                    && self.active_with_region_exclusions.is_empty();
                self.emit(
                    RESUME,
                    if wrapper_is_only_exception_region {
                        5
                    } else {
                        1
                    },
                    0,
                )?;
            }
            Expr::YieldFrom(expression) => {
                if self.flags & CO_GENERATOR == 0 {
                    return Err(unsupported("yield from outside generator code"));
                }
                self.compile_expression(&expression.value)?;
                self.emit(GET_YIELD_FROM_ITER, 0, 0)?;
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                let send = self.assembler.label();
                let end = self.assembler.label();
                let yielded = self.assembler.label();
                let yielded_end = self.assembler.label();
                let cleanup = self.assembler.label();
                self.assembler.mark(send);
                self.emit_jump_forward(SEND, end, 0)?;
                self.assembler.mark(yielded);
                self.emit(YIELD_VALUE, 1, 0)?;
                self.assembler.mark(yielded_end);
                self.emit(
                    RESUME,
                    if self.generator_region_start.is_some() {
                        2
                    } else {
                        6
                    },
                    0,
                )?;
                self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
                self.assembler.mark(cleanup);
                self.assembler.add_exception_region(
                    yielded,
                    yielded_end,
                    cleanup,
                    (starting_depth + 2).cast_unsigned(),
                    false,
                );
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.set_depth(starting_depth + 3);
                self.emit(CLEANUP_THROW, 0, -1)?;
                self.assembler.mark(end);
                self.set_depth(starting_depth + 2);
                self.emit(END_SEND, 0, -1)?;
            }
            Expr::Await(expression) => self.compile_await(expression)?,
            Expr::TString(tstring) => self.compile_tstring(tstring)?,
            Expr::Starred(starred) => {
                self.compile_expression(&starred.value)?;
                self.emit(UNPACK_SEQUENCE, 1, 0)?;
            }
            Expr::IpyEscapeCommand(_)
            | Expr::List(_)
            | Expr::Name(_)
            | Expr::Subscript(_)
            | Expr::Tuple(_)
            | Expr::Attribute(_) => return Err(unsupported(expression_name(expression))),
        }

        let deferred_restores = self
            .pending_comprehension_restores
            .iter()
            .map(|(indices, _)| indices.len())
            .sum::<usize>();
        let expected_depth = starting_depth
            + 1
            + if self.defer_async_comprehension_restore {
                i32::try_from(deferred_restores).unwrap()
            } else {
                0
            };
        if self.depth != expected_depth {
            return Err(CompileError::Internal(format!(
                "expression changed stack depth from {starting_depth} to {}",
                self.depth
            )));
        }
        Ok(())
    }

    fn emit_folded_constant(
        &mut self,
        expression: &Expr,
        constant: Constant,
    ) -> Result<(), CompileError> {
        if matches!(expression, Expr::Name(name) if name.id.as_str() == "__debug__") {
            return self.emit_preprocessed_constant(expression, constant);
        }
        if let Some(literal) = literal_constant(expression) {
            self.add_constant(literal)?;
        } else {
            self.record_folded_operands(expression)?;
        }
        if matches!(expression, Expr::Tuple(_))
            || matches!(expression, Expr::UnaryOp(unary) if unary.op == UnaryOp::Not)
        {
            self.emit_folded_tuple_not_nops(expression)?;
            self.assembler
                .set_location(self.source_location(expression.range()));
        }
        if let Constant::Int(value) = constant
            && let Ok(value) = u8::try_from(value)
        {
            self.emit(LOAD_SMALL_INT, u32::from(value), 1)
        } else {
            self.emit_deferred_constant(constant)
        }
    }

    fn record_folded_operands(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::BinOp(binary) => {
                self.record_folded_value(&binary.left)?;
                self.record_folded_value(&binary.right)?;
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.record_folded_value(element)?;
                }
            }
            Expr::Subscript(subscript) => {
                self.record_folded_value(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref() {
                    if let Some(constant) = constant_slice(slice) {
                        self.add_constant(constant)?;
                    } else {
                        for bound in [slice.lower.as_deref(), slice.upper.as_deref()]
                            .into_iter()
                            .flatten()
                        {
                            self.record_folded_value(bound)?;
                        }
                    }
                } else if !matches!(
                    fold_constant(&subscript.slice),
                    Some(Constant::Int(value)) if u8::try_from(value).is_ok()
                ) {
                    self.record_folded_value(&subscript.slice)?;
                }
            }
            Expr::UnaryOp(unary) => self.record_folded_value(&unary.operand)?,
            _ => {}
        }
        Ok(())
    }

    /// Emits operand-load residue retained when CPython's flowgraph folds a tuple containing
    /// constant boolean negations.
    fn emit_folded_tuple_not_nops(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.emit_folded_tuple_not_nops(element)?;
                }
            }
            Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => {
                self.assembler
                    .set_location(self.source_location(unary.operand.range()));
                self.emit(NOP, 0, 0)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn record_folded_value(&mut self, expression: &Expr) -> Result<(), CompileError> {
        if let Some(constant) = literal_constant(expression) {
            self.add_constant(constant)?;
            return Ok(());
        }
        if matches!(expression, Expr::Slice(_))
            && let Some(constant) = fold_constant(expression)
        {
            self.add_constant(constant)?;
            return Ok(());
        }
        if matches!(expression, Expr::Name(name) if name.id.as_str() == "__debug__") {
            self.add_constant(Constant::Bool(true))?;
            return Ok(());
        }
        if matches!(expression, Expr::FString(_))
            && let Some(constant) = fold_constant(expression)
        {
            self.add_constant(constant)?;
            return Ok(());
        }

        self.record_folded_operands(expression)?;
        if let Some(constant) = fold_constant(expression)
            && !matches!(constant, Constant::Int(value) if u8::try_from(value).is_ok())
        {
            self.deferred_constants.push((None, constant));
        }
        Ok(())
    }

    fn emit_preprocessed_constant(
        &mut self,
        expression: &Expr,
        constant: Constant,
    ) -> Result<(), CompileError> {
        if let Constant::Int(value) = constant
            && let Ok(value) = u8::try_from(value)
        {
            if self.constants.is_empty()
                && let Some(seed) = first_literal_constant(expression)
            {
                self.add_constant(seed)?;
            }
            self.emit(LOAD_SMALL_INT, u32::from(value), 1)
        } else {
            let index = self.add_constant(constant)?;
            self.emit(LOAD_CONST, index, 1)
        }
    }

    fn compile_fstring(
        &mut self,
        fstring: &ruff_python_ast::ExprFString,
    ) -> Result<(), CompileError> {
        fn append_literal(
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value: &str,
            range: ruff_text_size::TextRange,
        ) {
            if let Some((pending_value, pending_range)) = pending {
                pending_value.push_str(value);
                *pending_range = ruff_text_size::TextRange::new(pending_range.start(), range.end());
            } else {
                *pending = Some((value.to_string(), range));
            }
        }

        fn flush_literal(
            compiler: &mut Compiler,
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value_count: &mut usize,
            large: bool,
            outer_location: SourceLocation,
        ) -> Result<(), CompileError> {
            let Some((value, range)) = pending.take() else {
                return Ok(());
            };
            if value.is_empty() {
                return Ok(());
            }
            compiler
                .assembler
                .set_location(compiler.source_location(range));
            compiler.load_string_constant(&value)?;
            *value_count += 1;
            if large {
                compiler.assembler.set_location(outer_location);
                compiler.emit(LIST_APPEND, 1, -1)?;
            }
            Ok(())
        }

        let mut component_count = 0_usize;
        let mut has_pending_literal = false;
        let mut has_interpolation = false;
        for part in &fstring.value {
            match part {
                FStringPart::Literal(literal) => {
                    has_pending_literal |= !literal.value.is_empty();
                }
                FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                has_pending_literal |= !literal.value.is_empty();
                            }
                            InterpolatedStringElement::Interpolation(interpolation) => {
                                has_interpolation = true;
                                if interpolation.debug_text.is_some() {
                                    has_pending_literal = true;
                                }
                                if std::mem::take(&mut has_pending_literal) {
                                    component_count += 1;
                                }
                                component_count += 1;
                            }
                        }
                    }
                }
            }
        }
        component_count += usize::from(has_pending_literal);
        let large = component_count > 30;
        let outer_location = self.source_location(fstring.range);
        let single_literal_range = if component_count == 1 && !has_interpolation {
            Some(self.fstring_result_range(fstring))
        } else {
            None
        };
        if large {
            self.assembler.set_location(outer_location);
            self.load_string_constant("")?;
            let join = self.name_index("join")?;
            self.emit(LOAD_ATTR, (join << 1) | 1, 1)?;
            self.emit(BUILD_LIST, 0, 1)?;
        }

        let mut value_count = 0_usize;
        let mut pending_literal = None;
        for part in &fstring.value {
            match part {
                FStringPart::Literal(literal) => {
                    append_literal(
                        &mut pending_literal,
                        &literal.value,
                        single_literal_range.unwrap_or(literal.range),
                    );
                }
                FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                append_literal(
                                    &mut pending_literal,
                                    &literal.value,
                                    single_literal_range.unwrap_or(literal.range),
                                );
                            }
                            InterpolatedStringElement::Interpolation(interpolation) => {
                                if let Some(debug) = &interpolation.debug_text {
                                    let debug_text = strip_expression_comments(&format!(
                                        "{}{}{}",
                                        debug.leading(),
                                        debug.expression(),
                                        debug.trailing()
                                    ));
                                    append_literal(
                                        &mut pending_literal,
                                        &debug_text,
                                        debug_text_range(interpolation),
                                    );
                                }
                                flush_literal(
                                    self,
                                    &mut pending_literal,
                                    &mut value_count,
                                    large,
                                    outer_location,
                                )?;
                                self.compile_interpolation(interpolation)?;
                                value_count += 1;
                                if large {
                                    self.assembler.set_location(outer_location);
                                    self.emit(LIST_APPEND, 1, -1)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        flush_literal(
            self,
            &mut pending_literal,
            &mut value_count,
            large,
            outer_location,
        )?;
        self.assembler.set_location(outer_location);
        if large {
            self.emit(CALL, 1, -2)
        } else {
            self.finish_interpolated_string(value_count, "f-string component count")
        }
    }

    fn compile_tstring(
        &mut self,
        tstring: &ruff_python_ast::ExprTString,
    ) -> Result<(), CompileError> {
        let mut strings = vec![String::new()];
        let mut interpolations = Vec::new();
        for element in tstring.value.elements() {
            match element {
                InterpolatedStringElement::Literal(literal) => {
                    strings.last_mut().unwrap().push_str(&literal.value);
                }
                InterpolatedStringElement::Interpolation(interpolation) => {
                    if let Some(debug) = &interpolation.debug_text {
                        strings
                            .last_mut()
                            .unwrap()
                            .push_str(&strip_expression_comments(&format!(
                                "{}{}{}",
                                debug.leading(),
                                debug.expression(),
                                debug.trailing()
                            )));
                    }
                    interpolations.push(interpolation);
                    strings.push(String::new());
                }
            }
        }

        let strings = strings
            .into_iter()
            .map(Constant::String)
            .collect::<Vec<_>>();
        for string in &strings {
            self.add_constant(string.clone())?;
        }
        self.emit_deferred_constant(Constant::Tuple(strings))?;
        for interpolation in &interpolations {
            self.compile_expression(&interpolation.expression)?;
            self.assembler
                .set_location(self.source_location(interpolation.range));
            let expression_text = self.interpolation_expression_text(interpolation);
            self.load_string_constant(&expression_text)?;

            let conversion = if interpolation.debug_text.is_some()
                && interpolation.format_spec.is_none()
                && interpolation.conversion == ConversionFlag::None
            {
                ConversionFlag::Repr
            } else {
                interpolation.conversion
            };
            let mut argument = 2 | match conversion {
                ConversionFlag::None => 0,
                ConversionFlag::Str => 4,
                ConversionFlag::Repr => 8,
                ConversionFlag::Ascii => 12,
            };
            let effect = if let Some(format_spec) = &interpolation.format_spec {
                self.compile_interpolated_elements(&format_spec.elements, format_spec.range)?;
                argument |= 1;
                -2
            } else {
                -1
            };
            self.assembler
                .set_location(self.source_location(interpolation.range));
            self.emit(BUILD_INTERPOLATION, argument, effect)?;
        }
        self.assembler
            .set_location(self.source_location(tstring.range));
        if interpolations.is_empty() {
            self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
        } else {
            self.emit_build(BUILD_TUPLE, interpolations.len())?;
        }
        self.emit(BUILD_TEMPLATE, 0, -1)
    }

    fn interpolation_expression_text(
        &self,
        interpolation: &ruff_python_ast::InterpolatedElement,
    ) -> String {
        let start = usize::from(interpolation.range.start()) + 1;
        let expression_end = usize::from(interpolation.expression.range().end());
        // CPython truncates the stored expression text at a top-level `!=`
        // when the interpolation has a format specifier.
        if interpolation.format_spec.is_some()
            && interpolation.debug_text.is_none()
            && interpolation.conversion == ConversionFlag::None
            && let Expr::Compare(comparison) = interpolation.expression.as_ref()
            && comparison.ops.contains(&CmpOp::NotEq)
            && let Some(offset) = self.source[start..expression_end].find("!=")
        {
            return strip_expression_comments(self.source[start..start + offset].trim_end());
        }
        let interpolation_end = usize::from(interpolation.range.end()).saturating_sub(1);
        let trailing = &self.source[expression_end..interpolation_end];
        let delimiter = if interpolation.debug_text.is_some() {
            trailing.find('=')
        } else if interpolation.conversion != ConversionFlag::None {
            trailing.find('!')
        } else if interpolation.format_spec.is_some() {
            trailing.find(':')
        } else {
            None
        };
        let end = delimiter.map_or(interpolation_end, |offset| expression_end + offset);
        strip_expression_comments(&self.source[start..end])
            .trim_end()
            .to_string()
    }

    fn compile_interpolated_elements(
        &mut self,
        elements: &ruff_python_ast::InterpolatedStringElements,
        range: ruff_text_size::TextRange,
    ) -> Result<(), CompileError> {
        fn append_literal(
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value: &str,
            range: ruff_text_size::TextRange,
        ) {
            if let Some((pending_value, pending_range)) = pending {
                pending_value.push_str(value);
                *pending_range = ruff_text_size::TextRange::new(pending_range.start(), range.end());
            } else {
                *pending = Some((value.to_string(), range));
            }
        }

        fn flush_literal(
            compiler: &mut Compiler,
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value_count: &mut usize,
        ) -> Result<(), CompileError> {
            let Some((value, range)) = pending.take() else {
                return Ok(());
            };
            compiler
                .assembler
                .set_location(compiler.source_location(range));
            compiler.load_string_constant(&value)?;
            *value_count += 1;
            Ok(())
        }

        let mut value_count = 0_usize;
        let mut pending_literal = None;
        for element in elements {
            match element {
                InterpolatedStringElement::Literal(literal) => {
                    append_literal(&mut pending_literal, &literal.value, literal.range);
                }
                InterpolatedStringElement::Interpolation(interpolation) => {
                    if let Some(debug) = &interpolation.debug_text {
                        let debug_text = strip_expression_comments(&format!(
                            "{}{}{}",
                            debug.leading(),
                            debug.expression(),
                            debug.trailing()
                        ));
                        append_literal(
                            &mut pending_literal,
                            &debug_text,
                            debug_text_range(interpolation),
                        );
                    }
                    flush_literal(self, &mut pending_literal, &mut value_count)?;
                    self.compile_interpolation(interpolation)?;
                    value_count += 1;
                }
            }
        }
        flush_literal(self, &mut pending_literal, &mut value_count)?;
        let range = ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(u32::from(range.start()).saturating_sub(1)),
            range.end(),
        );
        self.assembler.set_location(self.source_location(range));
        self.finish_interpolated_string(value_count, "format specification component count")
    }

    fn compile_interpolation(
        &mut self,
        interpolation: &ruff_python_ast::InterpolatedElement,
    ) -> Result<(), CompileError> {
        self.compile_expression(&interpolation.expression)?;
        self.assembler
            .set_location(self.source_location(interpolation.range));
        match interpolation.conversion {
            ConversionFlag::None
                if interpolation.debug_text.is_some() && interpolation.format_spec.is_none() =>
            {
                self.emit(CONVERT_VALUE, 2, 0)?;
            }
            ConversionFlag::None => {}
            ConversionFlag::Str => self.emit(CONVERT_VALUE, 1, 0)?,
            ConversionFlag::Repr => self.emit(CONVERT_VALUE, 2, 0)?,
            ConversionFlag::Ascii => self.emit(CONVERT_VALUE, 3, 0)?,
        }
        if let Some(format_spec) = &interpolation.format_spec {
            self.compile_interpolated_elements(&format_spec.elements, format_spec.range)?;
            self.assembler
                .set_location(self.source_location(interpolation.range));
            self.emit(FORMAT_WITH_SPEC, 0, -1)
        } else {
            self.emit(FORMAT_SIMPLE, 0, 0)
        }
    }

    fn compile_optimized_percent_format(
        &mut self,
        expression: &ExprBinOp,
    ) -> Result<bool, CompileError> {
        let Some(parts) = optimized_percent_format(expression) else {
            return Ok(false);
        };

        let expression_location = self.source_location(expression.range);
        let part_count = parts.len();
        for part in parts {
            match part {
                PercentFormatPart::Literal(value) => {
                    // The preprocessing pass synthesizes these constants without
                    // source locations.
                    self.assembler.set_location(SourceLocation::NONE);
                    self.load_string_constant(&value)?;
                }
                PercentFormatPart::Formatted {
                    expression,
                    conversion,
                    format_spec,
                } => {
                    self.compile_expression(expression)?;
                    let location = self.source_location(expression.range());
                    self.assembler.set_location(location);
                    self.emit(CONVERT_VALUE, conversion, 0)?;
                    if let Some(format_spec) = format_spec {
                        self.assembler.set_location(SourceLocation::NONE);
                        self.load_string_constant(&format_spec)?;
                        self.assembler.set_location(location);
                        self.emit(FORMAT_WITH_SPEC, 0, -1)?;
                    } else {
                        self.emit(FORMAT_SIMPLE, 0, 0)?;
                    }
                }
            }
        }

        self.assembler.set_location(expression_location);
        match part_count {
            0 => self.load_string_constant("")?,
            1 => {}
            count => self.emit(
                BUILD_STRING,
                to_u32(count, "percent-format component count")?,
                1 - i32::try_from(count).unwrap(),
            )?,
        }
        Ok(true)
    }

    fn load_string_constant(&mut self, value: &str) -> Result<(), CompileError> {
        let constant = self.add_constant(Constant::String(value.to_string()))?;
        self.emit(LOAD_CONST, constant, 1)
    }

    fn finish_interpolated_string(
        &mut self,
        value_count: usize,
        description: &str,
    ) -> Result<(), CompileError> {
        match value_count {
            0 => self.load_string_constant(""),
            1 => Ok(()),
            count => self.emit(
                BUILD_STRING,
                to_u32(count, description)?,
                1 - i32::try_from(count).unwrap(),
            ),
        }
    }

    fn compile_await(
        &mut self,
        expression: &ruff_python_ast::ExprAwait,
    ) -> Result<(), CompileError> {
        if self.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("await outside coroutine code"));
        }

        self.compile_expression(&expression.value)?;
        self.compile_awaitable_on_stack(0)
    }

    fn compile_awaitable_on_stack(
        &mut self,
        get_awaitable_argument: u32,
    ) -> Result<(), CompileError> {
        let base_depth = self.depth - 1;
        let cleanup_location = self.assembler.location();
        self.emit(GET_AWAITABLE, get_awaitable_argument, 0)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;

        let send = self.assembler.label();
        let end = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit(RESUME, 3, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;

        self.assembler.mark(cleanup);
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup,
            (base_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.set_location(cleanup_location);
        self.set_depth(base_depth + 3);
        self.emit(CLEANUP_THROW, 0, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        Ok(())
    }

    fn emit_build(&mut self, opcode: Opcode, length: usize) -> Result<(), CompileError> {
        let length = to_u32(length, "collection element count")?;
        self.emit(opcode, length, 1 - i32::try_from(length).unwrap())
    }

    fn compile_call(&mut self, call: &ruff_python_ast::ExprCall) -> Result<(), CompileError> {
        if let Some(callable) = optimized_generator_callable(call) {
            return self.compile_optimized_generator_call(call, callable);
        }

        let has_starred = call.arguments.args.iter().any(Expr::is_starred_expr);
        let has_keyword_unpack = call
            .arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none());
        let direct_method = if let Some(attribute) = self.direct_method_attribute(call) {
            if !self.compile_super_attribute(attribute, true)? {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.assembler
                    .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                self.emit(LOAD_ATTR, (index << 1) | 1, 1)?;
            }
            true
        } else {
            false
        };
        if !direct_method && !self.compile_direct_global_callable(&call.func)? {
            let newly_owned_callable = if call
                .arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
                .any(expression_contains_inlined_comprehension)
                && let Expr::Name(name) = call.func.as_ref()
            {
                self.owned_load_locals.insert(name.id.to_string())
            } else {
                false
            };
            self.compile_expression(&call.func)?;
            if newly_owned_callable && let Expr::Name(name) = call.func.as_ref() {
                self.owned_load_locals.remove(name.id.as_str());
            }
            self.assembler
                .set_location(self.source_location(call.func.range()));
            self.emit(PUSH_NULL, 0, 1)?;
        }
        self.assembler
            .set_location(self.source_location(self.call_opcode_range(call)));

        if !has_starred && !has_keyword_unpack {
            if call.arguments.keywords.len() > 15 {
                let keyword_names = call
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| {
                        self.add_constant(Constant::String(
                            keyword.arg.as_ref().unwrap().as_str().to_string(),
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if call.arguments.args.is_empty() {
                    self.add_constant(Constant::None)?;
                    self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
                } else {
                    self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
                }
                self.emit(BUILD_MAP, 0, 1)?;
                for (keyword, name) in call.arguments.keywords.iter().zip(keyword_names) {
                    self.assembler
                        .set_location(self.source_location(self.call_opcode_range(call)));
                    self.emit(LOAD_CONST, name, 1)?;
                    self.compile_expression(&keyword.value)?;
                    self.assembler
                        .set_location(self.source_location(keyword.value.range()));
                    self.emit(MAP_ADD, 1, -2)?;
                }
                self.assembler
                    .set_location(self.source_location(self.call_opcode_range(call)));
                return self.emit(CALL_FUNCTION_EX, 0, -3);
            }
            for argument in &call.arguments.args {
                self.compile_expression(argument)?;
            }
            for keyword in &call.arguments.keywords {
                self.compile_expression(&keyword.value)?;
            }
            let argument_count = call.arguments.args.len() + call.arguments.keywords.len();
            let argument_count = to_u32(argument_count, "call argument count")?;
            if call.arguments.keywords.is_empty() {
                return self.emit(
                    CALL,
                    argument_count,
                    -i32::try_from(argument_count).unwrap() - 1,
                );
            }

            let keyword_names = call
                .arguments
                .keywords
                .iter()
                .map(|keyword| Constant::String(keyword.arg.as_ref().unwrap().as_str().to_string()))
                .collect();
            let names = self.add_interned_string_tuple(keyword_names)?;
            if direct_method {
                let range = if let Expr::Attribute(attribute) = call.func.as_ref() {
                    self.attribute_opcode_range(attribute)
                } else {
                    call.func.range()
                };
                self.assembler.set_location(self.source_location(range));
            }
            self.emit(LOAD_CONST, names, 1)?;
            self.assembler
                .set_location(self.source_location(self.call_opcode_range(call)));
            return self.emit(
                CALL_KW,
                argument_count,
                -i32::try_from(argument_count).unwrap() - 2,
            );
        }

        if call.arguments.args.is_empty() {
            self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
        } else if call.arguments.args.len() == 1 {
            if let Expr::Starred(starred) = &call.arguments.args[0] {
                self.compile_expression(&starred.value)?;
            } else {
                self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
            }
        } else {
            self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
        }

        if call.arguments.keywords.is_empty() {
            self.emit(PUSH_NULL, 0, 1)?;
        } else {
            let mut have_dict = false;
            let mut named_start = 0;
            for (index, keyword) in call.arguments.keywords.iter().enumerate() {
                if keyword.arg.is_some() {
                    continue;
                }
                if named_start < index {
                    self.compile_keyword_map(call, &call.arguments.keywords[named_start..index])?;
                    if have_dict {
                        self.emit(DICT_MERGE, 1, -1)?;
                    }
                    have_dict = true;
                }
                if !have_dict {
                    self.emit(BUILD_MAP, 0, 1)?;
                    have_dict = true;
                }
                self.compile_expression(&keyword.value)?;
                self.assembler
                    .set_location(self.source_location(self.call_opcode_range(call)));
                self.emit(DICT_MERGE, 1, -1)?;
                named_start = index + 1;
            }
            if named_start < call.arguments.keywords.len() {
                self.compile_keyword_map(call, &call.arguments.keywords[named_start..])?;
                if have_dict {
                    self.emit(DICT_MERGE, 1, -1)?;
                }
            }
        }
        self.emit(CALL_FUNCTION_EX, 0, -3)
    }

    fn compile_optimized_generator_call(
        &mut self,
        call: &ruff_python_ast::ExprCall,
        callable: &str,
    ) -> Result<(), CompileError> {
        let (common_constant, initial_result, collect_tuple) = match callable {
            "all" => (3, true, false),
            "any" => (4, false, false),
            "tuple" => (2, false, true),
            _ => unreachable!("optimized generator call must use all, any, or tuple"),
        };
        let callable_location = self.source_location(call.func.range());
        let base_depth = self.depth;
        let fallback = self.assembler.label();
        let end = self.assembler.label();

        self.compile_expression(&call.func)?;
        self.assembler.set_location(callable_location);
        self.emit(COPY, 1, 1)?;
        self.emit(LOAD_COMMON_CONSTANT, common_constant, 1)?;
        self.emit(IS_OP, 0, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, fallback, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(POP_TOP, 0, -1)?;

        if collect_tuple {
            self.emit(BUILD_LIST, 0, 1)?;
        }
        self.compile_expression(&call.arguments.args[0])?;

        let loop_start = self.assembler.label();
        let exhausted = self.assembler.label();
        let iterator_depth = self.depth;
        self.assembler.mark(loop_start);
        self.assembler.set_location(callable_location);
        self.emit_jump_forward(FOR_ITER, exhausted, 1)?;
        if collect_tuple {
            self.emit(LIST_APPEND, 2, -1)?;
            self.emit_jump_backward(JUMP_BACKWARD, loop_start, 0)?;
        } else {
            self.emit(TO_BOOL, 0, 0)?;
            let matched = self.assembler.label();
            self.emit_jump_forward(
                if initial_result {
                    POP_JUMP_IF_FALSE
                } else {
                    POP_JUMP_IF_TRUE
                },
                matched,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, loop_start, 0)?;
            self.assembler.mark(matched);
            self.emit(POP_ITER, 0, -1)?;
            let result = self.add_constant(Constant::Bool(!initial_result))?;
            self.emit(LOAD_CONST, result, 1)?;
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }

        self.assembler.mark(exhausted);
        self.set_depth(iterator_depth + 1);
        self.assembler.set_location(callable_location);
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        self.assembler.set_location(callable_location);
        if collect_tuple {
            self.emit(CALL_INTRINSIC_1, 6, 0)?;
        } else {
            let result = self.add_constant(Constant::Bool(initial_result))?;
            self.emit(LOAD_CONST, result, 1)?;
        }
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(fallback);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(callable_location);
        self.emit(PUSH_NULL, 0, 1)?;
        self.compile_expression(&call.arguments.args[0])?;
        self.assembler
            .set_location(self.source_location(self.call_opcode_range(call)));
        self.emit(CALL, 1, -2)?;

        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_keyword_map(
        &mut self,
        call: &ruff_python_ast::ExprCall,
        keywords: &[Keyword],
    ) -> Result<(), CompileError> {
        debug_assert!(!keywords.is_empty() && keywords.iter().all(|keyword| keyword.arg.is_some()));
        let call_location = self.source_location(self.call_opcode_range(call));
        if keywords.len() * 2 > 30 {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(BUILD_MAP, 0, 1)?;
            for keyword in keywords {
                let name = keyword.arg.as_ref().unwrap();
                let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
                self.assembler.set_location(call_location);
                self.emit(LOAD_CONST, key, 1)?;
                self.compile_expression(&keyword.value)?;
                self.assembler.set_location(SourceLocation::NONE);
                self.emit(MAP_ADD, 1, -2)?;
            }
        } else {
            for keyword in keywords {
                let name = keyword.arg.as_ref().unwrap();
                let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
                self.assembler.set_location(call_location);
                self.emit(LOAD_CONST, key, 1)?;
                self.compile_expression(&keyword.value)?;
            }
            self.assembler.set_location(call_location);
            let count = to_u32(keywords.len(), "keyword count")?;
            self.emit(
                BUILD_MAP,
                count,
                1 - i32::try_from(keywords.len() * 2).unwrap(),
            )?;
        }
        Ok(())
    }

    fn imported_module_attribute(&self, attribute: &ruff_python_ast::ExprAttribute) -> bool {
        let Expr::Name(name) = attribute.value.as_ref() else {
            return false;
        };
        // CPython only checks whether the name is import-originated in the module symbol table.
        self.imported_scope_names.contains(name.id.as_str())
    }

    fn compile_super_attribute(
        &mut self,
        attribute: &ruff_python_ast::ExprAttribute,
        method: bool,
    ) -> Result<bool, CompileError> {
        let Expr::Call(call) = attribute.value.as_ref() else {
            return Ok(false);
        };
        let Expr::Name(super_name) = call.func.as_ref() else {
            return Ok(false);
        };
        if super_name.id.as_str() != "super"
            || attribute.attr.as_str() == "__class__"
            || !call.arguments.keywords.is_empty()
            || self.imported_scope_names.contains("super")
            || self.imported_scope_names.contains(SHADOWED_SUPER_SENTINEL)
        {
            return Ok(false);
        }
        let explicit = match call.arguments.args.as_ref() {
            [] => false,
            [first, second]
                if !matches!(first, Expr::Starred(_)) && !matches!(second, Expr::Starred(_)) =>
            {
                true
            }
            _ => return Ok(false),
        };
        let globally_resolved = match &self.scope {
            Scope::Module => true,
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } => {
                globals.contains("super")
                    && !indices.contains_key("super")
                    && !free_indices.contains_key("super")
            }
            Scope::Class { .. } => false,
        };
        if !globally_resolved {
            return Ok(false);
        }

        if explicit {
            self.assembler
                .set_location(self.source_location(super_name.range));
            self.load_name("super")?;
            self.compile_expression(&call.arguments.args[0])?;
            self.compile_expression(&call.arguments.args[1])?;
            self.assembler
                .set_location(self.source_location(attribute.range));
            let attribute_index = self.name_index(attribute.attr.as_str())?;
            self.emit(
                LOAD_SUPER_ATTR,
                (attribute_index << 2) | 2 | u32::from(method),
                if method { -1 } else { -2 },
            )?;
            self.assembler
                .set_location(self.source_location(self.attribute_opcode_range(attribute)));
            self.emit(NOP, 0, 0)?;
            return Ok(true);
        }

        if self.arg_count == 0 {
            return Ok(false);
        }
        let (class_index, first_parameter) = match &self.scope {
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } if globals.contains("super") && !indices.contains_key("super") => {
                let Some(class_index) = free_indices.get("__class__").copied() else {
                    return Ok(false);
                };
                let Some(first_parameter) = self.locals.first() else {
                    return Ok(false);
                };
                let Some(first_parameter) = indices.get(first_parameter).copied() else {
                    return Ok(false);
                };
                (class_index, first_parameter)
            }
            _ => return Ok(false),
        };

        self.assembler
            .set_location(self.source_location(super_name.range));
        let super_index = self.name_index("super")?;
        self.emit(LOAD_GLOBAL, super_index << 1, 1)?;
        self.assembler
            .set_location(self.source_location(call.range));
        self.emit(LOAD_DEREF, class_index, 1)?;
        self.emit(LOAD_FAST, first_parameter, 1)?;
        self.assembler
            .set_location(self.source_location(attribute.range));
        let attribute_index = self.name_index(attribute.attr.as_str())?;
        self.emit(
            LOAD_SUPER_ATTR,
            (attribute_index << 2) | u32::from(method),
            if method { -1 } else { -2 },
        )?;
        self.assembler
            .set_location(self.source_location(self.attribute_opcode_range(attribute)));
        self.emit(NOP, 0, 0)?;
        Ok(true)
    }

    fn compile_direct_global_callable(&mut self, expression: &Expr) -> Result<bool, CompileError> {
        if self.annotation_classdict_index.is_some() {
            return Ok(false);
        }
        let Expr::Name(name) = expression else {
            return Ok(false);
        };
        let is_global = match &self.scope {
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } => {
                globals.contains(name.id.as_str())
                    || (!indices.contains_key(name.id.as_str())
                        && !free_indices.contains_key(name.id.as_str()))
            }
            Scope::Module | Scope::Class { .. } => false,
        };
        if !is_global {
            return Ok(false);
        }
        self.assembler
            .set_location(self.source_location(expression.range()));
        let index = self.name_index(name.id.as_str())?;
        self.emit(LOAD_GLOBAL, (index << 1) | 1, 2)?;
        Ok(true)
    }

    fn compile_lambda(&mut self, lambda: &ruff_python_ast::ExprLambda) -> Result<(), CompileError> {
        let empty_parameters = ruff_python_ast::Parameters::default();
        let parameters = lambda.parameters.as_deref().unwrap_or(&empty_parameters);
        if parameters
            .iter()
            .any(|parameter| parameter.annotation().is_some())
        {
            return Err(unsupported("lambda parameter annotation"));
        }

        let positional_defaults: Vec<_> = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .filter_map(|parameter| parameter.default.as_deref())
            .collect();
        if !positional_defaults.is_empty() {
            if let Some(defaults) = positional_defaults
                .iter()
                .map(|default| fold_constant(default))
                .collect::<Option<Vec<_>>>()
            {
                for default in &positional_defaults {
                    self.record_folded_value(default)?;
                }
                self.assembler
                    .set_location(self.source_location(lambda.range));
                self.emit_deferred_constant(Constant::Tuple(defaults))?;
            } else {
                for default in &positional_defaults {
                    self.compile_expression(default)?;
                }
                self.emit_build(BUILD_TUPLE, positional_defaults.len())?;
            }
        }
        let keyword_defaults: Vec<_> = parameters
            .kwonlyargs
            .iter()
            .filter_map(|parameter| {
                parameter
                    .default
                    .as_deref()
                    .map(|default| (parameter.name().as_str(), default))
            })
            .collect();
        if !keyword_defaults.is_empty() {
            for (name, default) in &keyword_defaults {
                let name = self.add_constant(Constant::String(self.mangled_name(name)))?;
                self.emit(LOAD_CONST, name, 1)?;
                self.compile_expression(default)?;
            }
            let count = to_u32(keyword_defaults.len(), "keyword default count")?;
            self.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        }

        let analysis = analyze_lambda_scope(lambda);
        let freevars = analysis
            .required
            .iter()
            .filter(|name| self.can_provide_closure(name))
            .cloned()
            .collect::<BTreeSet<_>>();
        let globals = analysis.required.difference(&freevars).cloned().collect();
        let closure_names = freevars.iter().cloned().collect::<Vec<_>>();
        let plan = FunctionPlan {
            key: (0, 0),
            locals: analysis.locals,
            globals,
            nonlocals: HashSet::new(),
            references: analysis.references,
            annotation_references: HashSet::new(),
            cellvars: analysis.cellvars,
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };

        let arg_count = to_u32(
            parameters.posonlyargs.len() + parameters.args.len(),
            "lambda argument count",
        )?;
        let positional_only = to_u32(
            parameters.posonlyargs.len(),
            "lambda positional-only argument count",
        )?;
        let keyword_only = to_u32(
            parameters.kwonlyargs.len(),
            "lambda keyword-only argument count",
        )?;
        let mut parameter_flags = (if parameters.vararg.is_some() {
            CO_VARARGS
        } else {
            0
        } | if parameters.kwarg.is_some() {
            CO_VARKEYWORDS
        } else {
            0
        } | if matches!(self.scope, Scope::Function { .. }) {
            CO_NESTED
        } else if matches!(self.scope, Scope::Class { .. }) {
            CO_METHOD
                | if self.class_scope_is_nested {
                    CO_NESTED
                } else {
                    0
                }
        } else {
            0
        }) | (self.flags & CO_FUTURE_MASK);
        if expression_contains_yield(&lambda.body) {
            parameter_flags |= CO_GENERATOR;
        }
        let qualified_name = self.child_qualified_name("<lambda>");
        let first_line_number = self.line_number(u32::from(lambda.range.start()));
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            "<lambda>",
            qualified_name,
            first_line_number,
            plan,
            arg_count,
            positional_only,
            keyword_only,
            parameter_flags,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        child.emit_function_prologue()?;
        if parameter_flags & CO_GENERATOR != 0 {
            // CPython does not wrap generator lambdas in the stop-iteration
            // handler used by `def` generators and generator expressions.
            child.generator_region_start = None;
        }
        child.compile_expression(&lambda.body)?;
        child
            .assembler
            .set_location(child.source_location(lambda.body.range()));
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;

        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let constant = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, constant, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        if !keyword_defaults.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 2, -1)?;
        }
        if !positional_defaults.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 1, -1)?;
        }
        Ok(())
    }

    fn compile_comprehension(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        key: Option<&Expr>,
        value: &Expr,
        build_opcode: Opcode,
        add_opcode: Opcode,
        discard_result: bool,
    ) -> Result<(), CompileError> {
        if generators.is_empty() {
            return Err(CompileError::Internal(
                "comprehension has no generators".to_string(),
            ));
        }
        let base_depth = self.depth;
        let comprehension_location = self.assembler.location();
        if generators[0].is_async {
            self.compile_expression(&generators[0].iter)?;
        } else {
            self.compile_iterable_expression(&generators[0].iter)?;
        }
        self.assembler
            .set_location(self.source_location(generators[0].iter.range()));
        self.emit(
            if generators[0].is_async {
                GET_AITER
            } else {
                GET_ITER
            },
            0,
            0,
        )?;
        self.assembler.set_location(comprehension_location);

        let mut temporary_names = Vec::new();
        for (index, generator) in generators.iter().enumerate() {
            collect_target_names(&generator.target, &mut temporary_names);
            // The first iterable runs before entering the inlined comprehension scope.
            if index > 0 {
                collect_nested_comprehension_target_names(&generator.iter, &mut temporary_names);
            }
            for condition in &generator.ifs {
                collect_nested_comprehension_target_names(condition, &mut temporary_names);
            }
        }
        if let Some(key) = key {
            collect_nested_comprehension_target_names(key, &mut temporary_names);
        }
        collect_nested_comprehension_target_names(value, &mut temporary_names);
        let active_temporary_names = temporary_names.clone();
        if matches!(self.scope, Scope::Module) {
            // CPython assigns a hidden fast-local slot to module-scope walrus targets while
            // inlining a comprehension. Function-scope targets remain bindings in the
            // containing function and are not part of the comprehension's save set.
            if let Some(key) = key {
                collect_named_expression_target_names(key, &mut temporary_names);
            }
            collect_named_expression_target_names(value, &mut temporary_names);
            for generator in generators {
                for condition in &generator.ifs {
                    collect_named_expression_target_names(condition, &mut temporary_names);
                }
            }
        }
        let mut seen_temporaries = HashSet::new();
        temporary_names.retain(|name| seen_temporaries.insert(name.clone()));
        let mut temporary_indices = Vec::with_capacity(temporary_names.len());
        for name in &temporary_names {
            let index = self.ensure_temporary(name)?;
            temporary_indices.push(index);
            self.emit(LOAD_FAST_AND_CLEAR, index, 1)?;
        }
        self.emit(
            SWAP,
            to_u32(temporary_names.len() + 1, "comprehension local count")?,
            0,
        )?;
        self.active_temporaries
            .extend(active_temporary_names.iter().cloned());

        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let parent_cleanup = self.active_comprehension_cleanups.last().copied();
        let cleanup_location = self.assembler.location();
        self.assembler.mark(protected_start);
        self.emit(build_opcode, 0, 1)?;
        self.emit(SWAP, 2, 0)?;
        self.active_comprehension_cleanups.push((
            cleanup,
            (base_depth + i32::try_from(temporary_names.len()).unwrap() + 1).cast_unsigned(),
        ));
        self.active_comprehension_region_exclusions.push(Vec::new());
        self.compile_comprehension_generator(generators, 0, key, value, add_opcode)?;
        let region_exclusions = self
            .active_comprehension_region_exclusions
            .pop()
            .expect("active comprehension has region exclusions");
        self.active_comprehension_cleanups.pop();
        self.assembler.mark(protected_end);

        let inline_cleanup = true;
        if inline_cleanup {
            let normal_cleanup = self.assembler.label();
            self.emit_jump_forward(JUMP_FORWARD, normal_cleanup, 0)?;
            if discard_result && generators.iter().any(|generator| generator.is_async) {
                self.assembler.prevent_last_jump_inlining();
            }
            self.assembler.mark(cleanup);
            self.set_depth(base_depth + i32::try_from(temporary_indices.len()).unwrap() + 2);
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(SWAP, 2, 0)?;
            self.emit(POP_TOP, 0, -1)?;
            self.assembler.set_location(cleanup_location);
            self.emit(
                SWAP,
                to_u32(temporary_indices.len() + 1, "comprehension local count")?,
                0,
            )?;
            for (position, index) in temporary_indices.iter().rev().enumerate() {
                if position > 0 {
                    self.assembler.fusion_barrier();
                }
                self.emit(STORE_FAST, *index, -1)?;
            }
            self.emit(RERAISE, 0, -1)?;
            let cleanup_end = self.assembler.label();
            self.assembler.mark(cleanup_end);
            if let Some((parent, depth)) = parent_cleanup {
                self.assembler
                    .add_exception_region(cleanup, cleanup_end, parent, depth, false);
            }
            self.assembler.mark(normal_cleanup);
        }

        let deferred_normal_restore = inline_cleanup && self.defer_async_comprehension_restore;
        self.set_depth(base_depth + i32::try_from(temporary_names.len()).unwrap() + 1);
        self.assembler.set_location(comprehension_location);
        if deferred_normal_restore {
            self.pending_comprehension_restores
                .push((temporary_indices.clone(), comprehension_location));
        } else {
            if discard_result {
                self.emit(POP_TOP, 0, -1)?;
            } else {
                self.emit(
                    SWAP,
                    to_u32(temporary_names.len() + 1, "comprehension local count")?,
                    0,
                )?;
            }
            if discard_result {
                let final_index = temporary_indices.last();
                let restore_indices = temporary_indices
                    [..temporary_indices.len().saturating_sub(1)]
                    .iter()
                    .rev()
                    .chain(final_index);
                for (position, index) in restore_indices.enumerate() {
                    if position > 0 {
                        self.assembler.fusion_barrier();
                    }
                    self.emit(STORE_FAST, *index, -1)?;
                    self.assembler
                        .prevent_last_instruction_fusion_with_previous();
                }
            } else {
                for (position, index) in temporary_indices.iter().rev().enumerate() {
                    if position > 0 {
                        self.assembler.fusion_barrier();
                    }
                    self.emit(STORE_FAST, *index, -1)?;
                    self.assembler
                        .prevent_last_instruction_fusion_with_previous();
                }
            }
            // CPython converts STORE_FAST_MAYBE_NULL only after inserting superinstructions.
            self.assembler.prevent_last_instruction_fusion();
        }
        for name in &active_temporary_names {
            self.active_temporaries.remove(name);
        }

        let cleanup_depth =
            (base_depth + i32::try_from(temporary_names.len()).unwrap() + 1).cast_unsigned();
        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                cleanup,
                cleanup_depth,
                false,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            cleanup,
            cleanup_depth,
            false,
        );
        if !inline_cleanup {
            self.deferred_comprehension_cleanups
                .push(DeferredComprehensionCleanup {
                    label: cleanup,
                    base_depth,
                    temporary_indices,
                    location: cleanup_location,
                    parent: parent_cleanup,
                });
        }
        self.set_depth(
            base_depth
                + if deferred_normal_restore {
                    i32::try_from(temporary_names.len()).unwrap() + 1
                } else {
                    i32::from(!discard_result)
                },
        );
        Ok(())
    }

    fn compile_generator_expression(
        &mut self,
        generator: &ruff_python_ast::ExprGenerator,
    ) -> Result<(), CompileError> {
        if generator.generators.is_empty() {
            return Err(CompileError::Internal(
                "generator expression has no generators".to_string(),
            ));
        }
        let is_async = generator.generators.iter().any(|generator| {
            generator.is_async
                || expression_contains_await(&generator.iter)
                || generator.ifs.iter().any(expression_contains_await)
        }) || expression_contains_await(&generator.elt);

        let mut locals = vec![".0".to_string()];
        for comprehension in &generator.generators {
            collect_target_names(&comprehension.target, &mut locals);
        }
        let mut seen = HashSet::new();
        locals.retain(|name| seen.insert(name.clone()));
        let required_names = generator_required_names(generator);
        let generator_cellvars = generator_cellvars(generator);
        let mut globals = HashSet::new();
        let mut freevars = BTreeSet::new();
        for name in required_names {
            if self.can_provide_closure(&name) {
                freevars.insert(name);
            } else {
                globals.insert(name);
            }
        }
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let generator_range = self.generator_expression_range(generator.range);
        let plan = FunctionPlan {
            key: (0, 0),
            locals,
            globals,
            nonlocals: HashSet::new(),
            references: HashSet::new(),
            annotation_references: HashSet::new(),
            cellvars: generator_cellvars,
            freevars,
            annotation_freevars: BTreeSet::new(),
            children: Vec::new(),
        };
        let flags = if is_async {
            CO_ASYNC_GENERATOR
        } else {
            CO_GENERATOR
        } | if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | if matches!(self.scope, Scope::Class { .. }) {
            CO_METHOD
        } else {
            0
        } | (self.flags & CO_FUTURE_MASK);
        let mut child = Self::function(
            &self.filename,
            Arc::clone(&self.source),
            "<genexpr>",
            self.child_qualified_name("<genexpr>"),
            self.line_number(u32::from(generator_range.start())),
            plan,
            1,
            0,
            0,
            flags,
        )?;
        child.private_name.clone_from(&self.private_name);
        child
            .imported_scope_names
            .clone_from(&self.imported_scope_names);
        // CPython only inserts `.<locals>` for function, async-function, and lambda parents.
        child.child_qualified_name_parent = Some(child.qualified_name.clone());
        child.emit_function_prologue()?;
        let generator_location = child.source_location(generator_range);
        child.assembler.set_location(generator_location);
        child.emit(LOAD_FAST_OWNED, 0, 1)?;
        child.compile_generator_expression_loop(
            &generator.generators,
            0,
            &generator.elt,
            generator_location,
        )?;
        let child = child.finish()?;

        self.assembler
            .set_location(self.source_location(generator_range));
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit(SET_FUNCTION_ATTRIBUTE, 8, -1)?;
        }
        if generator.generators[0].is_async {
            self.compile_expression(&generator.generators[0].iter)?;
        } else {
            self.compile_iterable_expression(&generator.generators[0].iter)?;
        }
        self.assembler
            .set_location(self.source_location(generator.generators[0].iter.range()));
        self.emit(
            if generator.generators[0].is_async {
                GET_AITER
            } else {
                GET_ITER
            },
            0,
            0,
        )?;
        self.assembler
            .set_location(self.source_location(generator_range));
        self.emit(CALL, 0, -1)
    }

    fn compile_generator_expression_loop(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        element: &Expr,
        generator_location: SourceLocation,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        if generator.is_async {
            return self.compile_async_generator_expression_loop(
                generators,
                index,
                element,
                generator_location,
            );
        }
        let loop_depth = self.depth;
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(start);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&generator.target)?;
        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            self.compile_generator_filter(condition, element, accepted, start)?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_generator_expression_loop(
                generators,
                index + 1,
                element,
                generator_location,
            )?;
            self.assembler
                .set_location(self.source_location(element.range()));
        } else {
            self.compile_expression(element)?;
            self.assembler
                .set_location(self.source_location(element.range()));
            if self.flags & CO_ASYNC_GENERATOR != 0 {
                self.emit(CALL_INTRINSIC_1, 4, 0)?;
            }
            self.emit(YIELD_VALUE, 0, 0)?;
            self.emit(RESUME, 5, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.mark(cleanup);
        self.set_depth(loop_depth + 1);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        Ok(())
    }

    fn compile_async_generator_expression_loop(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        element: &Expr,
        generator_location: SourceLocation,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        let loop_depth = self.depth;
        let cleanup_location = generator_location;
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let async_cleanup = self.assembler.label();

        self.assembler.set_location(generator_location);
        self.assembler.mark(start);
        self.assembler.mark(protected_start);
        self.emit(GET_ANEXT, 0, 1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, send_end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit(RESUME, 3, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
        self.assembler.mark(send_end);
        self.set_depth(loop_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            self.compile_generator_filter(condition, element, accepted, start)?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_generator_expression_loop(
                generators,
                index + 1,
                element,
                generator_location,
            )?;
            self.assembler
                .set_location(self.source_location(element.range()));
        } else {
            self.compile_expression(element)?;
            self.assembler
                .set_location(self.source_location(element.range()));
            self.emit(CALL_INTRINSIC_1, 4, 0)?;
            self.emit(YIELD_VALUE, 0, 0)?;
            self.emit(RESUME, 5, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.set_location(cleanup_location);

        self.assembler.mark(cleanup_throw);
        self.set_depth(loop_depth + 3);
        self.emit(CLEANUP_THROW, 0, -1)?;
        let cleanup_throw_end = self.assembler.label();
        self.assembler.mark(cleanup_throw_end);
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (loop_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.mark(async_cleanup);
        self.generator_region_exclusions
            .push((cleanup_throw, async_cleanup));
        self.set_depth(loop_depth + 1);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(END_ASYNC_FOR, send_end);
        self.assembler.add_exception_region(
            protected_start,
            protected_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        Ok(())
    }

    fn compile_generator_filter(
        &mut self,
        condition: &Expr,
        element: &Expr,
        accepted: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if early_condition_truthiness(condition) == Some(true) {
            self.record_folded_value(condition)?;
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = condition
            && boolean.op == BoolOp::And
        {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            for value in leading {
                let next = self.assembler.label();
                self.compile_generator_filter(value, element, next, restart)?;
                self.assembler.mark(next);
            }
            return self.compile_generator_filter(last, element, accepted, restart);
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler
                .set_location(self.source_location(element.range()));
            self.emit_jump_forward(
                if comparison.ops[0] == CmpOp::Is {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                accepted,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.generator_region_exclusions
                .push((exclusion_start, exclusion_end));
            return Ok(());
        }
        self.compile_expression(condition)?;
        self.assembler
            .set_location(self.source_location(condition.range()));
        self.emit(TO_BOOL, 0, 0)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.assembler
            .set_location(self.source_location(element.range()));
        self.emit_jump_forward(POP_JUMP_IF_TRUE, accepted, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        self.generator_region_exclusions
            .push((exclusion_start, exclusion_end));
        Ok(())
    }

    fn compile_comprehension_filter(
        &mut self,
        condition: &Expr,
        element_location: SourceLocation,
        accepted: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if early_condition_truthiness(condition) == Some(true) {
            self.record_folded_value(condition)?;
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = condition
            && boolean.op == BoolOp::And
        {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            for value in leading {
                let next = self.assembler.label();
                self.compile_comprehension_filter(value, element_location, next, restart)?;
                self.assembler.mark(next);
            }
            return self.compile_comprehension_filter(last, element_location, accepted, restart);
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() > 1
        {
            let base_depth = self.depth;
            let cleanup = self.assembler.label();
            let residue = self.assembler.label();
            let comparison_location = self.source_location(comparison.range);
            self.compile_expression(&comparison.left)?;
            for (operator, comparator) in comparison
                .ops
                .iter()
                .zip(&comparison.comparators)
                .take(comparison.ops.len() - 1)
            {
                self.compile_expression(comparator)?;
                self.assembler.set_location(comparison_location);
                self.emit(SWAP, 2, 0)?;
                self.emit(COPY, 2, 1)?;
                let (opcode, argument) = comparison_operator_boolean(*operator);
                self.emit(opcode, argument, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }

            self.compile_expression(comparison.comparators.last().unwrap())?;
            self.assembler.set_location(comparison_location);
            let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
            self.emit(opcode, argument, -1)?;
            let terminal_exclusion_start = self.assembler.label();
            self.assembler.mark(terminal_exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_forward(POP_JUMP_IF_TRUE, residue, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let terminal_exclusion_end = self.assembler.label();
            self.assembler.mark(terminal_exclusion_end);

            self.assembler.mark(residue);
            self.assembler.set_location(comparison_location);
            self.emit_jump_forward(JUMP_FORWARD, accepted, 0)?;

            self.assembler.mark(cleanup);
            self.set_depth(base_depth + 1);
            self.emit(POP_TOP, 0, -1)?;
            let cleanup_exclusion_start = self.assembler.label();
            self.assembler.mark(cleanup_exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let cleanup_exclusion_end = self.assembler.label();
            self.assembler.mark(cleanup_exclusion_end);
            self.set_depth(base_depth);

            for (start, end) in [
                (terminal_exclusion_start, terminal_exclusion_end),
                (cleanup_exclusion_start, cleanup_exclusion_end),
            ] {
                self.generator_region_exclusions.push((start, end));
                for exclusions in &mut self.active_comprehension_region_exclusions {
                    exclusions.push((start, end));
                }
            }
            return Ok(());
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_forward(
                if comparison.ops[0] == CmpOp::Is {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                accepted,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.generator_region_exclusions
                .push((exclusion_start, exclusion_end));
            for exclusions in &mut self.active_comprehension_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            return Ok(());
        }
        self.compile_expression(condition)?;
        self.assembler
            .set_location(self.source_location(condition.range()));
        self.emit(TO_BOOL, 0, 0)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.assembler.set_location(element_location);
        self.emit_jump_forward(POP_JUMP_IF_TRUE, accepted, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        self.generator_region_exclusions
            .push((exclusion_start, exclusion_end));
        for exclusions in &mut self.active_comprehension_region_exclusions {
            exclusions.push((exclusion_start, exclusion_end));
        }
        Ok(())
    }

    fn compile_comprehension_generator(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        key: Option<&Expr>,
        value: &Expr,
        add_opcode: Opcode,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        if generator.is_async {
            return self
                .compile_async_comprehension_generator(generators, index, key, value, add_opcode);
        }
        let loop_depth = self.depth;
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(start);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            let element_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.compile_comprehension_filter(
                condition,
                self.source_location(element_range),
                accepted,
                start,
            )?;
            self.assembler.mark(accepted);
        }

        let mut loop_back_location = None;
        if index + 1 < generators.len() {
            let nested = &generators[index + 1];
            let singleton = match &nested.iter {
                Expr::List(list) if list.elts.len() == 1 => Some(&list.elts[0]),
                Expr::Tuple(tuple) if tuple.elts.len() == 1 => Some(&tuple.elts[0]),
                _ => None,
            };
            if index + 2 == generators.len()
                && nested.ifs.is_empty()
                && !nested.is_async
                && let Some(singleton) = singleton
            {
                self.compile_expression(singleton)?;
                self.compile_store_target(&nested.target)?;
                self.compile_comprehension_element(key, value)?;
                let add_range = key.map_or_else(
                    || value.range(),
                    |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
                );
                self.assembler.set_location(self.source_location(add_range));
                let effect = if key.is_some() { -2 } else { -1 };
                self.emit(
                    add_opcode,
                    to_u32(index + 2, "comprehension depth")?,
                    effect,
                )?;
                loop_back_location =
                    Some(self.source_location(key.map_or_else(|| value.range(), Ranged::range)));
            } else {
                if nested.is_async {
                    self.compile_expression(&nested.iter)?;
                } else {
                    self.compile_iterable_expression(&nested.iter)?;
                }
                self.assembler
                    .set_location(self.source_location(nested.iter.range()));
                self.emit(if nested.is_async { GET_AITER } else { GET_ITER }, 0, 0)?;
                self.compile_comprehension_generator(
                    generators,
                    index + 1,
                    key,
                    value,
                    add_opcode,
                )?;
                loop_back_location = Some(self.source_location(key.unwrap_or(value).range()));
            }
        } else {
            self.compile_comprehension_element(key, value)?;
            let add_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.assembler.set_location(self.source_location(add_range));
            let effect = if key.is_some() { -2 } else { -1 };
            self.emit(
                add_opcode,
                to_u32(generators.len() + 1, "comprehension depth")?,
                effect,
            )?;
        }
        if let Some(location) = loop_back_location {
            self.assembler.set_location(location);
        } else if let Some(location) = self.assembler.last_instruction_location() {
            self.assembler.set_location(location);
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.mark(cleanup);
        self.set_depth(loop_depth + 1);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        Ok(())
    }

    fn compile_comprehension_element(
        &mut self,
        key: Option<&Expr>,
        value: &Expr,
    ) -> Result<(), CompileError> {
        let folded_conditional_key = matches!(key, Some(Expr::If(expression)) if early_condition_truthiness(&expression.test).is_some());
        self.compile_with_strict_owned_loads(folded_conditional_key, |compiler| {
            if let Some(key) = key {
                if folded_conditional_key {
                    // CPython retains the folded conditional's entry and join boundaries. They
                    // prevent fusion around the key and force both MAP_ADD operands to be owned.
                    compiler.assembler.fusion_barrier();
                }
                compiler.compile_expression(key)?;
                if folded_conditional_key {
                    compiler.assembler.fusion_barrier();
                }
            }
            compiler.compile_expression(value)
        })
    }

    fn compile_async_comprehension_generator(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        key: Option<&Expr>,
        value: &Expr,
        add_opcode: Opcode,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        let loop_depth = self.depth;
        let cleanup_location = self.assembler.location();
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let async_cleanup = self.assembler.label();

        self.assembler.mark(start);
        self.assembler.mark(protected_start);
        self.emit(GET_ANEXT, 0, 1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, send_end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit(RESUME, 3, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
        self.assembler.mark(send_end);
        self.set_depth(loop_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            let element_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.compile_comprehension_filter(
                condition,
                self.source_location(element_range),
                accepted,
                start,
            )?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_comprehension_generator(generators, index + 1, key, value, add_opcode)?;
            self.assembler
                .set_location(self.source_location(key.unwrap_or(value).range()));
        } else {
            self.compile_comprehension_element(key, value)?;
            let add_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.assembler.set_location(self.source_location(add_range));
            let effect = if key.is_some() { -2 } else { -1 };
            self.emit(
                add_opcode,
                to_u32(generators.len() + 1, "comprehension depth")?,
                effect,
            )?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.set_location(cleanup_location);

        self.assembler.mark(cleanup_throw);
        self.set_depth(loop_depth + 3);
        self.emit(CLEANUP_THROW, 0, -1)?;
        let cleanup_throw_end = self.assembler.label();
        self.assembler.mark(cleanup_throw_end);
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (loop_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.mark(async_cleanup);
        self.generator_region_exclusions
            .push((cleanup_throw, async_cleanup));
        for exclusions in &mut self.active_comprehension_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        for exclusions in &mut self.active_with_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        for exclusions in &mut self.active_exception_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        self.set_depth(loop_depth + 1);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(END_ASYNC_FOR, send_end);
        self.assembler.add_exception_region(
            protected_start,
            protected_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        Ok(())
    }

    fn ensure_temporary(&mut self, name: &str) -> Result<u32, CompileError> {
        if let Some(index) = self.temporary_indices.get(name).copied() {
            return Ok(index);
        }
        let existing = match &self.scope {
            Scope::Function { indices, .. } => indices.get(name).copied(),
            Scope::Module | Scope::Class { .. } => None,
        };
        let index = if let Some(index) = existing {
            index
        } else {
            let index = to_u32(self.locals.len(), "comprehension local count")?;
            self.locals.push(name.to_string());
            if !matches!(self.scope, Scope::Function { .. }) {
                self.hidden_names.insert(name.to_string());
            }
            index
        };
        self.temporary_indices.insert(name.to_string(), index);
        Ok(index)
    }

    fn allocate_match_temporary(&mut self) -> Result<u32, CompileError> {
        let name = format!(".match_temp_{}", self.match_temporary_index);
        self.match_temporary_index += 1;
        self.ensure_temporary(&name)
    }

    fn compile_collection(
        &mut self,
        elements: &[Expr],
        kind: CollectionKind,
    ) -> Result<(), CompileError> {
        if !elements.iter().any(Expr::is_starred_expr) {
            if kind == CollectionKind::Tuple
                && let Some(constants) = elements
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
            {
                for element in elements {
                    self.record_folded_value(element)?;
                }
                return self.emit_deferred_constant(Constant::Tuple(constants));
            }
            if elements.len() > 30 {
                self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
                for element in elements {
                    self.compile_expression(element)?;
                    self.emit(kind.append_opcode(), 1, -1)?;
                }
                if kind == CollectionKind::Tuple {
                    self.emit(CALL_INTRINSIC_1, 6, 0)?;
                }
                return Ok(());
            }
            if matches!(kind, CollectionKind::List | CollectionKind::Set)
                && elements.len() >= 3
                && let Some(constants) = elements
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
            {
                for element in elements {
                    self.record_folded_value(element)?;
                }
                let constant = if kind == CollectionKind::Set {
                    let mut unique = Vec::with_capacity(constants.len());
                    for constant in constants {
                        if !unique
                            .iter()
                            .any(|existing| python_constants_equal(existing, &constant))
                        {
                            unique.push(constant);
                        }
                    }
                    Constant::FrozenSet(unique)
                } else {
                    Constant::Tuple(constants)
                };
                self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
                self.emit_deferred_constant(constant)?;
                self.emit(
                    if kind == CollectionKind::List {
                        LIST_EXTEND
                    } else {
                        SET_UPDATE
                    },
                    1,
                    -1,
                )?;
                return Ok(());
            }
            for element in elements {
                self.compile_expression(element)?;
            }
            return self.emit_build(kind.build_opcode(), elements.len());
        }

        let unpacking_start = elements
            .iter()
            .position(Expr::is_starred_expr)
            .unwrap_or(elements.len());
        if unpacking_start >= 3
            && let Some(constants) = elements[..unpacking_start]
                .iter()
                .map(fold_constant)
                .collect::<Option<Vec<_>>>()
        {
            for element in &elements[..unpacking_start] {
                self.record_folded_value(element)?;
            }
            let constant = if kind == CollectionKind::Set {
                let mut unique = Vec::with_capacity(constants.len());
                for constant in constants {
                    if !unique
                        .iter()
                        .any(|existing| python_constants_equal(existing, &constant))
                    {
                        unique.push(constant);
                    }
                }
                Constant::FrozenSet(unique)
            } else {
                Constant::Tuple(constants)
            };
            self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
            self.emit_deferred_constant(constant)?;
            self.emit(kind.extend_opcode(), 1, -1)?;
        } else {
            for element in &elements[..unpacking_start] {
                self.compile_expression(element)?;
            }
            self.emit_build(kind.build_opcode_for_unpacking(), unpacking_start)?;
        }
        for element in &elements[unpacking_start..] {
            if let Expr::Starred(starred) = element {
                self.compile_expression(&starred.value)?;
                self.emit(kind.extend_opcode(), 1, -1)?;
            } else {
                self.compile_expression(element)?;
                self.emit(kind.append_opcode(), 1, -1)?;
            }
        }
        if kind == CollectionKind::Tuple {
            self.emit(CALL_INTRINSIC_1, 6, 0)?;
        }
        Ok(())
    }

    fn compile_iterable_expression(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = match expression {
            Expr::Tuple(tuple)
                if tuple.elts.iter().any(Expr::is_starred_expr) || tuple.elts.len() > 30 =>
            {
                // CPython lowers these tuples through a temporary list, then removes the
                // list-to-tuple intrinsic when the tuple is immediately iterated.
                self.compile_collection(&tuple.elts, CollectionKind::List)
            }
            Expr::List(list) if !list.elts.iter().any(Expr::is_starred_expr) => {
                if let Some(constants) = list
                    .elts
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
                {
                    for element in &list.elts {
                        self.record_folded_value(element)?;
                    }
                    self.emit_deferred_constant(Constant::Tuple(constants))
                } else {
                    self.compile_collection(&list.elts, CollectionKind::Tuple)
                }
            }
            Expr::Set(set)
                if !set.elts.iter().any(Expr::is_starred_expr)
                    && set
                        .elts
                        .iter()
                        .all(|element| fold_constant(element).is_some()) =>
            {
                let mut unique = Vec::with_capacity(set.elts.len());
                for element in &set.elts {
                    self.record_folded_value(element)?;
                    let constant = fold_constant(element).unwrap();
                    if !unique
                        .iter()
                        .any(|existing| python_constants_equal(existing, &constant))
                    {
                        unique.push(constant);
                    }
                }
                self.emit_deferred_constant(Constant::FrozenSet(unique))
            }
            _ => self.compile_expression_inner(expression),
        };
        self.assembler.set_location(previous);
        result
    }

    fn compile_dict(&mut self, items: &[ruff_python_ast::DictItem]) -> Result<(), CompileError> {
        if items.iter().all(|item| item.key.is_some()) {
            if items.len() > 15 {
                self.emit(BUILD_MAP, 0, 1)?;
                let mut index = 0;
                let mut first_group = true;
                while items.len() - index > 15 {
                    if !first_group {
                        self.emit(BUILD_MAP, 0, 1)?;
                    }
                    let group_end = (index + 17).min(items.len());
                    while index < group_end {
                        self.compile_expression(items[index].key.as_ref().unwrap())?;
                        self.compile_expression(&items[index].value)?;
                        self.emit(MAP_ADD, 1, -2)?;
                        index += 1;
                    }
                    if !first_group {
                        self.emit(DICT_UPDATE, 1, -1)?;
                    }
                    first_group = false;
                }
                if index < items.len() {
                    let tail_start = index;
                    while index < items.len() {
                        self.compile_expression(items[index].key.as_ref().unwrap())?;
                        self.compile_expression(&items[index].value)?;
                        index += 1;
                    }
                    self.emit_build_map(index - tail_start)?;
                    self.emit(DICT_UPDATE, 1, -1)?;
                }
                return Ok(());
            }
            for item in items {
                self.compile_expression(item.key.as_ref().unwrap())?;
                self.compile_expression(&item.value)?;
            }
            let length = to_u32(items.len(), "dictionary item count")?;
            return self.emit(BUILD_MAP, length, 1 - i32::try_from(length).unwrap() * 2);
        }

        let mut index = 0;
        while index < items.len() && items[index].key.is_some() {
            self.compile_expression(items[index].key.as_ref().unwrap())?;
            self.compile_expression(&items[index].value)?;
            index += 1;
        }
        self.emit_build_map(index)?;

        while index < items.len() {
            self.compile_expression(&items[index].value)?;
            self.emit(DICT_UPDATE, 1, -1)?;
            index += 1;

            let group_start = index;
            while index < items.len() && items[index].key.is_some() {
                self.compile_expression(items[index].key.as_ref().unwrap())?;
                self.compile_expression(&items[index].value)?;
                index += 1;
            }
            if index > group_start {
                self.emit_build_map(index - group_start)?;
                self.emit(DICT_UPDATE, 1, -1)?;
            }
        }
        Ok(())
    }

    fn emit_build_map(&mut self, length: usize) -> Result<(), CompileError> {
        let length = to_u32(length, "dictionary item count")?;
        self.emit(BUILD_MAP, length, 1 - i32::try_from(length).unwrap() * 2)
    }

    fn compile_slice(&mut self, slice: &ruff_python_ast::ExprSlice) -> Result<(), CompileError> {
        let slice_location = self.source_location(slice.range);
        if let Some(constant) = constant_slice(slice) {
            let index = self.add_constant(constant)?;
            self.assembler.set_location(slice_location);
            return self.emit(LOAD_CONST, index, 1);
        }
        for part in [&slice.lower, &slice.upper] {
            self.compile_optional_slice_bound(part.as_deref(), slice_location)?;
        }
        let argument_count = if let Some(step) = &slice.step {
            self.compile_expression(step)?;
            3
        } else {
            2
        };
        self.emit(
            BUILD_SLICE,
            argument_count,
            1 - argument_count.cast_signed(),
        )
    }

    fn compile_optional_slice_bound(
        &mut self,
        bound: Option<&Expr>,
        fallback_location: SourceLocation,
    ) -> Result<(), CompileError> {
        if let Some(bound) = bound {
            self.compile_expression(bound)
        } else {
            let none = self.add_constant(Constant::None)?;
            self.assembler.set_location(fallback_location);
            self.emit(LOAD_CONST, none, 1)
        }
    }

    fn compile_if_expression(
        &mut self,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        if let Some(truthiness) = early_condition_truthiness(&expression.test) {
            if let Some(constant) = fold_constant(&expression.test)
                && (self.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            self.pre_register_expression_names(&expression.body)?;
            self.pre_register_expression_names(&expression.orelse)?;
            let selected = if truthiness {
                &expression.body
            } else {
                &expression.orelse
            };
            if self.source_location(expression.test.range()).line
                != self.source_location(selected.range()).line
            {
                // Folding a multiline condition leaves its source-position marker in CPython's
                // optimized flow graph.
                self.assembler
                    .set_location(self.source_location(expression.test.range()));
                self.emit(NOP, 0, 0)?;
            }
            self.compile_expression(selected)?;
            // CPython leaves the eliminated branch as an empty CFG block before the join.
            // Stack-depth analysis crosses that block, but borrowed-load analysis does not,
            // so loads emitted after the folded conditional remain owned.
            self.assembler.prevent_next_borrow_reachability();
            return Ok(());
        }
        let base_depth = self.depth;
        let else_label = self.assembler.label();
        let end = self.assembler.label();
        self.compile_jump_if(&expression.test, false, else_label)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(expression.body.range()));
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        self.assembler.mark(else_label);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        self.assembler.preserve_block_boundary(end);
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_bool_expression(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
    ) -> Result<(), CompileError> {
        self.compile_bool_expression_at(expression, self.source_location(expression.range))
    }

    fn compile_bool_expression_at(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
        location: SourceLocation,
    ) -> Result<(), CompileError> {
        let mut effective = Vec::with_capacity(expression.values.len());
        let mut short_circuited = false;
        let mut folded_prefix = false;
        for (index, value) in expression.values.iter().enumerate() {
            let is_last = index + 1 == expression.values.len();
            if short_circuited {
                self.pre_register_expression_names(value)?;
                if fold_constant(value).is_some() {
                    self.record_folded_value(value)?;
                }
                continue;
            }
            if !is_last && let Some(truthiness) = early_condition_truthiness(value) {
                let decisive = matches!(expression.op, BoolOp::And) && !truthiness
                    || matches!(expression.op, BoolOp::Or) && truthiness;
                if decisive {
                    effective.push(value);
                    short_circuited = true;
                } else {
                    self.record_folded_value(value)?;
                    folded_prefix |= index == 0 && fold_constant(value).is_some();
                }
                continue;
            }
            effective.push(value);
        }

        let Some((last, leading)) = effective.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler.set_location(location);
        if folded_prefix {
            // CPython folds the leading constant load and its boolean jump, but retains the
            // jump as a source-position NOP when the next operand starts on another line.
            self.emit(NOP, 0, 0)?;
        }
        let base_depth = self.depth;
        let end = self.assembler.label();
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };

        for value in leading {
            if let Expr::BoolOp(inner) = value
                && inner.op == expression.op
            {
                self.compile_bool_expression_at(inner, location)?;
            } else {
                self.compile_expression(value)?;
            }
            self.assembler.set_location(location);
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            if self.generator_region_start.is_some() {
                self.generator_region_exclusions
                    .push((exclusion_start, exclusion_end));
            }
            self.emit_jump_forward(jump, end, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_location(self.source_location(expression.range));
            self.emit(POP_TOP, 0, -1)?;
        }
        if let Expr::BoolOp(inner) = last
            && inner.op == expression.op
        {
            self.compile_bool_expression_at(inner, location)?;
        } else {
            self.compile_expression(last)?;
        }
        self.assembler.preserve_block_boundary(end);
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_compare_expression(
        &mut self,
        expression: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        self.compile_expression(&expression.left)?;
        if expression.ops.len() == 1 {
            self.compile_comparison_rhs(expression.ops[0], &expression.comparators[0])?;
            let (opcode, argument) = comparison_operator(expression.ops[0]);
            return self.emit(opcode, argument, -1);
        }

        let base_depth = self.depth - 1;
        let cleanup = self.assembler.label();
        for (operator, comparator) in expression
            .ops
            .iter()
            .zip(&expression.comparators)
            .take(expression.ops.len() - 1)
        {
            self.compile_expression(comparator)?;
            self.emit(Opcode::new(117, 0), 2, 0)?;
            self.emit(COPY, 2, 1)?;
            let (opcode, argument) = comparison_operator(*operator);
            self.emit(opcode, argument, -1)?;
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.compile_expression(expression.comparators.last().unwrap())?;
        let (opcode, argument) = comparison_operator(*expression.ops.last().unwrap());
        self.emit(opcode, argument, -1)?;
        let end = self.assembler.label();
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.emit(Opcode::new(117, 0), 2, 0)?;
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_comparison_rhs(
        &mut self,
        operator: CmpOp,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        if matches!(operator, CmpOp::In | CmpOp::NotIn) {
            self.compile_iterable_expression(expression)
        } else {
            self.compile_expression(expression)
        }
    }

    fn compile_jump_if(
        &mut self,
        expression: &Expr,
        jump_on: bool,
        label: Label,
    ) -> Result<(), CompileError> {
        if let Expr::UnaryOp(unary) = expression
            && unary.op == UnaryOp::Not
        {
            return self.compile_jump_if(&unary.operand, !jump_on, label);
        }
        if let Expr::If(conditional) = expression
            && let Some(truthiness) = early_condition_truthiness(&conditional.test)
        {
            if let Some(constant) = fold_constant(&conditional.test)
                && (self.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            self.pre_register_expression_names(&conditional.body)?;
            self.pre_register_expression_names(&conditional.orelse)?;
            return self.compile_jump_if(
                if truthiness {
                    &conditional.body
                } else {
                    &conditional.orelse
                },
                jump_on,
                label,
            );
        }
        let constant_truthiness = jump_constant_truthiness(expression);
        if let Some(truthiness) = constant_truthiness {
            self.record_folded_value(expression)?;
            self.assembler
                .set_location(self.source_location(expression.range()));
            if truthiness == jump_on {
                self.emit_jump_forward(JUMP_FORWARD, label, 0)?;
                self.assembler.preserve_last_inlined_jump_nop();
            } else {
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOP, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if self.generator_region_start.is_some() {
                    self.generator_region_exclusions
                        .push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.active_with_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.active_exception_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
            }
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = expression {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            let short_circuit_jumps_to_target = matches!(boolean.op, BoolOp::Or) == jump_on;
            if short_circuit_jumps_to_target {
                for value in leading {
                    self.compile_jump_if(value, jump_on, label)?;
                }
                return self.compile_jump_if(last, jump_on, label);
            }
            let end = self.assembler.label();
            for value in leading {
                self.compile_jump_if(value, !jump_on, end)?;
            }
            self.compile_jump_if(last, jump_on, label)?;
            self.assembler.preserve_block_boundary(end);
            self.assembler.mark(end);
            return Ok(());
        }
        if let Expr::If(conditional) = expression {
            let otherwise = self.assembler.label();
            let end = self.assembler.label();
            self.compile_jump_if(&conditional.test, false, otherwise)?;
            self.compile_jump_if(&conditional.body, jump_on, label)?;
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            self.assembler.mark(otherwise);
            self.compile_jump_if(&conditional.orelse, jump_on, label)?;
            self.assembler.preserve_block_boundary(end);
            self.assembler.mark(end);
            return Ok(());
        }
        if let Expr::Compare(comparison) = expression
            && comparison.ops.len() > 1
        {
            let base_depth = self.depth;
            let cleanup = self.assembler.label();
            let end = self.assembler.label();
            self.compile_expression(&comparison.left)?;
            for (operator, comparator) in comparison
                .ops
                .iter()
                .zip(&comparison.comparators)
                .take(comparison.ops.len() - 1)
            {
                self.compile_expression(comparator)?;
                self.assembler
                    .set_location(self.source_location(comparison.range));
                self.emit(SWAP, 2, 0)?;
                self.emit(COPY, 2, 1)?;
                let (opcode, argument) = comparison_operator_boolean(*operator);
                self.emit(opcode, argument, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }

            self.compile_expression(comparison.comparators.last().unwrap())?;
            self.assembler
                .set_location(self.source_location(comparison.range));
            let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
            self.emit(opcode, argument, -1)?;
            self.emit_jump_forward(
                if jump_on {
                    POP_JUMP_IF_TRUE
                } else {
                    POP_JUMP_IF_FALSE
                },
                label,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if !jump_on {
                self.assembler.prevent_last_jump_inlining();
            }

            self.assembler.mark(cleanup);
            self.set_depth(base_depth + 1);
            self.emit(POP_TOP, 0, -1)?;
            if !jump_on {
                self.emit_jump_forward(JUMP_FORWARD, label, 0)?;
            }

            self.assembler.mark(end);
            self.set_depth(base_depth);
            return Ok(());
        }
        if let Expr::Compare(comparison) = expression
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let left_location = self.source_location(comparison.left.range());
            let none_location = self.source_location(comparison.comparators[0].range());
            if left_location.line != none_location.line {
                self.assembler.set_location(none_location);
                self.emit(NOP, 0, 0)?;
            }
            self.assembler
                .set_location(self.source_location(expression.range()));
            let jump_if_none = jump_on == (comparison.ops[0] == CmpOp::Is);
            self.emit_jump_forward(
                if jump_if_none {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                label,
                -1,
            )?;
            if self.exclude_terminal_if_not_taken || self.exclude_condition_not_taken_from_exception
            {
                let exclude_from_generator = self.exclude_condition_not_taken_from_exception
                    && self.generator_region_start.is_some();
                let exclude_from_exception = self.exclude_condition_not_taken_from_exception
                    || (self.exclude_terminal_if_not_taken
                        && !self.active_with_region_exclusions.is_empty());
                self.exclude_terminal_if_not_taken = false;
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOT_TAKEN, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if exclude_from_generator {
                    self.generator_region_exclusions
                        .push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.active_with_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
                if exclude_from_exception {
                    if self.exclude_condition_not_taken_from_all_exception_regions {
                        for exclusions in &mut self.active_exception_region_exclusions {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    } else if let Some(exclusions) =
                        self.active_exception_region_exclusions.last_mut()
                    {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                }
                return Ok(());
            }
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_last_normalized_exception_owner(self.active_normal_finally_bodies > 0);
            return Ok(());
        }
        let comparison_is_boolean = if let Expr::Compare(comparison) = expression
            && comparison.ops.len() == 1
        {
            self.compile_expression(&comparison.left)?;
            self.compile_comparison_rhs(comparison.ops[0], &comparison.comparators[0])?;
            self.assembler
                .set_location(self.source_location(expression.range()));
            let (opcode, argument) = comparison_operator_boolean(comparison.ops[0]);
            self.emit(opcode, argument, -1)?;
            true
        } else {
            self.compile_expression(expression)?;
            false
        };
        self.assembler
            .set_location(self.source_location(expression.range()));
        if !comparison_is_boolean {
            self.emit(TO_BOOL, 0, 0)?;
        }
        self.emit_jump_forward(
            if jump_on {
                POP_JUMP_IF_TRUE
            } else {
                POP_JUMP_IF_FALSE
            },
            label,
            -1,
        )?;
        if self.exclude_terminal_if_not_taken || self.exclude_condition_not_taken_from_exception {
            let exclude_from_generator = self.exclude_condition_not_taken_from_exception
                && self.generator_region_start.is_some();
            let exclude_from_exception = self.exclude_condition_not_taken_from_exception
                || (self.exclude_terminal_if_not_taken
                    && !self.active_with_region_exclusions.is_empty());
            self.exclude_terminal_if_not_taken = false;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_last_normalized_exception_owner(comparison_is_boolean);
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            if exclude_from_generator {
                self.generator_region_exclusions
                    .push((exclusion_start, exclusion_end));
            }
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            if exclude_from_exception {
                if self.exclude_condition_not_taken_from_all_exception_regions {
                    for exclusions in &mut self.active_exception_region_exclusions {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                } else if let Some(exclusions) = self.active_exception_region_exclusions.last_mut()
                {
                    exclusions.push((exclusion_start, exclusion_end));
                }
            }
            Ok(())
        } else {
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler.set_last_normalized_exception_owner(
                comparison_is_boolean || self.active_normal_finally_bodies > 0,
            );
            Ok(())
        }
    }

    fn compile_store_target(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = self.compile_store_target_inner(expression);
        self.assembler.set_location(previous);
        result
    }

    fn compile_store_target_inner(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::Name(name) => self.store_name(name.id.as_str()),
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.assembler
                    .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                self.emit(STORE_ATTR, index, -2)
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref()
                    && two_element_slice_optimization(slice)
                {
                    let slice_location = self.source_location(slice.range);
                    self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                    self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                    self.assembler
                        .set_location(self.source_location(subscript.range));
                    self.emit(STORE_SLICE, 0, -4)
                } else {
                    self.compile_expression(&subscript.slice)?;
                    self.assembler
                        .set_location(self.source_location(subscript.range));
                    self.emit(STORE_SUBSCR, 0, -3)
                }
            }
            Expr::List(list) => self.compile_unpack_target(&list.elts),
            Expr::Tuple(tuple) => self.compile_unpack_target(&tuple.elts),
            Expr::Starred(starred) => self.compile_store_target(&starred.value),
            _ => Err(unsupported("assignment target")),
        }
    }

    fn compile_unpack_target(&mut self, elements: &[Expr]) -> Result<(), CompileError> {
        let starred = elements.iter().position(Expr::is_starred_expr);
        if let Some(starred) = starred {
            let before = to_u32(starred, "unpack target count")?;
            let after = to_u32(elements.len() - starred - 1, "unpack target count")?;
            self.emit(
                UNPACK_EX,
                before | (after << 8),
                i32::try_from(elements.len()).unwrap() - 1,
            )?;
        } else {
            let count = to_u32(elements.len(), "unpack target count")?;
            self.emit(
                UNPACK_SEQUENCE,
                count,
                i32::try_from(elements.len()).unwrap() - 1,
            )?;
        }
        for element in elements {
            self.compile_store_target(element)?;
        }
        Ok(())
    }

    fn compile_delete_target(&mut self, expression: &Expr) -> Result<(), CompileError> {
        self.assembler
            .set_location(self.source_location(expression.range()));
        match expression {
            Expr::Name(name) => self.delete_name(name.id.as_str()),
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.emit(DELETE_ATTR, index, -1)
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                self.compile_expression(&subscript.slice)?;
                self.emit(DELETE_SUBSCR, 0, -2)
            }
            Expr::List(list) => {
                for element in &list.elts {
                    self.compile_delete_target(element)?;
                }
                Ok(())
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.compile_delete_target(element)?;
                }
                Ok(())
            }
            _ => Err(unsupported("deletion target")),
        }
    }

    fn load_name(&mut self, name: &str) -> Result<(), CompileError> {
        if self.active_temporaries.contains(name) {
            let index = self.temporary_indices[name];
            return self.emit(LOAD_FAST, index, 1);
        }
        let annotation_classdict_index = self.annotation_classdict_index;
        match &self.scope {
            Scope::Module => {
                let index = self.name_index(name)?;
                if self.module_globals.contains(name) {
                    self.emit(LOAD_GLOBAL, index << 1, 1)
                } else {
                    self.emit(LOAD_NAME, index, 1)
                }
            }
            Scope::Class {
                globals,
                free_indices,
                ..
            } => {
                if globals.contains(name) {
                    let index = self.name_index(name)?;
                    self.emit(LOAD_GLOBAL, index << 1, 1)
                } else if let Some(index) = free_indices.get(name).copied() {
                    self.emit(LOAD_LOCALS, 0, 1)?;
                    self.emit(LOAD_FROM_DICT_OR_DEREF, index, 0)
                } else {
                    let index = self.name_index(name)?;
                    self.emit(LOAD_NAME, index, 1)
                }
            }
            Scope::Function {
                indices,
                free_indices,
                cells,
                globals,
            } => {
                if globals.contains(name) {
                    if let Some(classdict) = annotation_classdict_index
                        && name != "__classdict__"
                    {
                        self.emit(LOAD_DEREF, classdict, 1)?;
                        let name = self.annotation_mangled_name(name);
                        let index = self.name_index(&name)?;
                        return self.emit(LOAD_FROM_DICT_OR_GLOBALS, index, 0);
                    }
                    let index = self.name_index(name)?;
                    return self.emit(LOAD_GLOBAL, index << 1, 1);
                }
                if let Some(index) = indices.get(name).copied() {
                    if cells.contains(name) {
                        self.emit(LOAD_DEREF, index, 1)
                    } else if self.initialized_locals.contains(name) {
                        self.emit(
                            if self.owned_load_locals.contains(name) {
                                LOAD_FAST_OWNED
                            } else {
                                LOAD_FAST
                            },
                            index,
                            1,
                        )
                    } else {
                        self.emit(LOAD_FAST_CHECK, index, 1)
                    }
                } else if let Some(index) = free_indices.get(name).copied() {
                    if let Some(classdict) = annotation_classdict_index
                        && name != "__classdict__"
                    {
                        self.emit(LOAD_DEREF, classdict, 1)?;
                        self.emit(LOAD_FROM_DICT_OR_DEREF, index, 0)
                    } else {
                        self.emit(LOAD_DEREF, index, 1)
                    }
                } else {
                    if let Some(classdict) = annotation_classdict_index {
                        self.emit(LOAD_DEREF, classdict, 1)?;
                        let index = self.name_index(name)?;
                        return self.emit(LOAD_FROM_DICT_OR_GLOBALS, index, 0);
                    }
                    let index = self.name_index(name)?;
                    self.emit(LOAD_GLOBAL, index << 1, 1)
                }
            }
        }
    }

    fn mark_definitely_evaluated_locals(&mut self, expression: &Expr) {
        let Some(references) = definitely_evaluated_references(expression) else {
            return;
        };
        let Scope::Function { indices, .. } = &self.scope else {
            return;
        };
        self.initialized_locals.extend(
            references
                .into_iter()
                .filter(|name| indices.contains_key(name)),
        );
    }

    fn store_name(&mut self, name: &str) -> Result<(), CompileError> {
        if self.active_temporaries.contains(name) {
            let index = self.temporary_indices[name];
            return self.emit(STORE_FAST, index, -1);
        }
        match &self.scope {
            Scope::Module => {
                let index = self.name_index(name)?;
                if self.module_globals.contains(name) {
                    self.emit(STORE_GLOBAL, index, -1)
                } else {
                    self.emit(STORE_NAME, index, -1)
                }
            }
            Scope::Class {
                globals,
                nonlocals,
                free_indices,
            } => {
                if globals.contains(name) {
                    let index = self.name_index(name)?;
                    self.emit(STORE_GLOBAL, index, -1)
                } else if nonlocals.contains(name) {
                    let index = free_indices.get(name).copied().ok_or_else(|| {
                        CompileError::Internal(format!("missing class nonlocal variable `{name}`"))
                    })?;
                    self.emit(STORE_DEREF, index, -1)
                } else {
                    let index = self.name_index(name)?;
                    self.emit(STORE_NAME, index, -1)
                }
            }
            Scope::Function {
                indices,
                free_indices,
                cells,
                globals,
            } => {
                if globals.contains(name) {
                    let index = self.name_index(name)?;
                    return self.emit(STORE_GLOBAL, index, -1);
                }
                if let Some(index) = indices.get(name).copied() {
                    self.initialized_locals.insert(name.to_string());
                    if cells.contains(name) {
                        self.emit(STORE_DEREF, index, -1)
                    } else {
                        self.emit(STORE_FAST, index, -1)
                    }
                } else if let Some(index) = free_indices.get(name).copied() {
                    self.emit(STORE_DEREF, index, -1)
                } else {
                    Err(CompileError::Internal(format!(
                        "missing local variable `{name}`"
                    )))
                }
            }
        }
    }

    fn delete_name(&mut self, name: &str) -> Result<(), CompileError> {
        if self.active_temporaries.contains(name) {
            let index = self.temporary_indices[name];
            return self.emit(DELETE_FAST, index, 0);
        }
        match &self.scope {
            Scope::Module => {
                let index = self.name_index(name)?;
                if self.module_globals.contains(name) {
                    self.emit(DELETE_GLOBAL, index, 0)
                } else {
                    self.emit(DELETE_NAME, index, 0)
                }
            }
            Scope::Class {
                globals,
                nonlocals,
                free_indices,
            } => {
                if globals.contains(name) {
                    let index = self.name_index(name)?;
                    self.emit(DELETE_GLOBAL, index, 0)
                } else if nonlocals.contains(name) {
                    let index = free_indices.get(name).copied().ok_or_else(|| {
                        CompileError::Internal(format!("missing class nonlocal variable `{name}`"))
                    })?;
                    self.emit(DELETE_DEREF, index, 0)
                } else {
                    let index = self.name_index(name)?;
                    self.emit(DELETE_NAME, index, 0)
                }
            }
            Scope::Function {
                indices,
                free_indices,
                cells,
                globals,
            } => {
                if globals.contains(name) {
                    let index = self.name_index(name)?;
                    return self.emit(DELETE_GLOBAL, index, 0);
                }
                if let Some(index) = indices.get(name).copied() {
                    self.initialized_locals.remove(name);
                    if cells.contains(name) {
                        self.emit(DELETE_DEREF, index, 0)
                    } else {
                        self.emit(DELETE_FAST, index, 0)
                    }
                } else if let Some(index) = free_indices.get(name).copied() {
                    self.emit(DELETE_DEREF, index, 0)
                } else {
                    Err(CompileError::Internal(format!(
                        "missing local variable `{name}`"
                    )))
                }
            }
        }
    }

    fn closure_index(&self, name: &str) -> Result<u32, CompileError> {
        match &self.scope {
            Scope::Module => Err(CompileError::Internal(format!(
                "module cannot provide closure variable `{name}`"
            ))),
            Scope::Class { .. } => self
                .locals
                .iter()
                .position(|local| local == name)
                .map(|index| to_u32(index, "class closure variable index"))
                .transpose()?
                .ok_or_else(|| {
                    CompileError::Internal(format!(
                        "class `{}` cannot provide closure variable `{name}` (locals: {:?}; free names: {:?})",
                        self.qualified_name, self.locals, self.free_names
                    ))
                }),
            Scope::Function {
                indices,
                free_indices,
                ..
            } => indices
                .get(name)
                .or_else(|| free_indices.get(name))
                .copied()
                .ok_or_else(|| {
                    CompileError::Internal(format!("missing closure variable `{name}`"))
                }),
        }
    }

    fn can_provide_closure(&self, name: &str) -> bool {
        match &self.scope {
            Scope::Module => false,
            Scope::Class { free_indices, .. } => {
                self.cell_names.contains(name) || free_indices.contains_key(name)
            }
            Scope::Function {
                indices,
                free_indices,
                cells,
                ..
            } => {
                indices.contains_key(name) && cells.contains(name)
                    || free_indices.contains_key(name)
            }
        }
    }

    fn emit_closure_tuple(&mut self, names: &[String]) -> Result<(), CompileError> {
        for (index, name) in names.iter().enumerate() {
            if index > 0 {
                self.assembler.fusion_barrier();
            }
            let local = self.closure_index(name)?;
            self.emit(LOAD_FAST, local, 1)?;
        }
        self.emit_build(BUILD_TUPLE, names.len())
    }

    fn emit_pending_comprehension_restores(&mut self) -> Result<(), CompileError> {
        let restores = std::mem::take(&mut self.pending_comprehension_restores);
        for (indices, location) in restores {
            self.assembler.set_location(location);
            for (position, index) in indices.iter().enumerate() {
                self.assembler.fusion_barrier();
                if position > 0 {
                    self.assembler.set_location(location);
                }
                self.emit(STORE_FAST, *index, -1)?;
                self.assembler
                    .prevent_last_instruction_fusion_with_previous();
            }
            // CPython converts STORE_FAST_MAYBE_NULL only after inserting superinstructions.
            self.assembler.prevent_last_instruction_fusion();
        }
        Ok(())
    }

    fn child_function_is_nested(&self) -> bool {
        match self.scope {
            Scope::Function { .. } => true,
            Scope::Class { .. } => self.class_scope_is_nested,
            Scope::Module => false,
        }
    }

    fn child_qualified_name(&self, name: &str) -> String {
        if let Some(parent) = &self.child_qualified_name_parent {
            return if parent.is_empty() {
                name.to_string()
            } else {
                format!("{parent}.{name}")
            };
        }
        if self.annotation_thunk
            && let Some(prefix) = self.qualified_name.strip_suffix("__annotate__")
        {
            return format!("{prefix}{name}");
        }
        match &self.scope {
            Scope::Module => name.to_string(),
            Scope::Class { .. } => format!("{}.{}", self.qualified_name, name),
            Scope::Function { globals, .. } if globals.contains(name) => name.to_string(),
            Scope::Function { .. } => format!("{}.<locals>.{name}", self.qualified_name),
        }
    }

    fn annotation_mangled_name(&self, name: &str) -> String {
        if self.private_name.is_some() {
            return self.mangled_name(name);
        }
        if !name.starts_with("__") || name.ends_with("__") || name.contains('.') {
            return name.to_string();
        }
        let Some(class_name) = self
            .qualified_name
            .strip_suffix(".__annotate__")
            .and_then(|parent| parent.rsplit('.').next())
            .map(|name| name.trim_start_matches('_'))
            .filter(|name| !name.is_empty())
        else {
            return name.to_string();
        };
        format!("_{class_name}{name}")
    }

    fn mangled_name(&self, name: &str) -> String {
        if !name.starts_with("__") || name.ends_with("__") || name.contains('.') {
            return name.to_string();
        }
        let Some(class_name) = self
            .private_name
            .as_deref()
            .map(|name| name.trim_start_matches('_'))
            .filter(|name| !name.is_empty())
        else {
            return name.to_string();
        };
        format!("_{class_name}{name}")
    }

    fn line_number(&self, offset: u32) -> u32 {
        let offset = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.source.len());
        1 + u32::try_from(
            self.source[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        )
        .unwrap_or(u32::MAX - 1)
    }

    fn source_location(&self, range: ruff_text_size::TextRange) -> SourceLocation {
        let (line, column) = self.source_position(u32::from(range.start()));
        let (end_line, end_column) = self.source_position(u32::from(range.end()));
        SourceLocation::new(line, end_line, column, end_column)
    }

    fn source_location_including_trailing_semicolon(
        &self,
        range: ruff_text_size::TextRange,
    ) -> SourceLocation {
        let (line, column) = self.source_position(u32::from(range.start()));
        let (end_line, end_column) = self.definition_end_position(range);
        SourceLocation::new(line, end_line, column, end_column)
    }

    fn definition_location(
        &self,
        range: ruff_text_size::TextRange,
        name_range: ruff_text_size::TextRange,
        keyword: &[u8],
        is_async: bool,
    ) -> SourceLocation {
        fn find_keyword(bytes: &[u8], start: usize, end: usize, keyword: &[u8]) -> Option<usize> {
            let last_start = end.checked_sub(keyword.len())?;
            (start..=last_start).rev().find(|&offset| {
                bytes.get(offset..offset + keyword.len()) == Some(keyword)
                    && bytes
                        .get(offset.wrapping_sub(1))
                        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
                    && bytes
                        .get(offset + keyword.len())
                        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
            })
        }

        let bytes = self.source.as_bytes();
        let range_start = usize::from(range.start());
        let name_start = usize::from(name_range.start());
        let keyword_start = find_keyword(bytes, range_start, name_start, keyword)
            .unwrap_or_else(|| name_start.saturating_sub(keyword.len() + 1));
        let start = if is_async {
            find_keyword(bytes, range_start, keyword_start, b"async").unwrap_or(keyword_start)
        } else {
            keyword_start
        };
        let (line, column) = self.source_position(u32::try_from(start).unwrap_or(u32::MAX));
        let (end_line, end_column) = self.definition_end_position(range);
        SourceLocation::new(line, end_line, column, end_column)
    }

    fn definition_end_position(&self, range: ruff_text_size::TextRange) -> (i32, i32) {
        let bytes = self.source.as_bytes();
        let original_end = usize::from(range.end());
        let mut end = original_end;
        loop {
            while matches!(bytes.get(end), Some(b' ' | b'\t' | b'\x0c')) {
                end += 1;
            }
            if bytes.get(end) == Some(&b';') {
                return self.source_position(u32::try_from(end + 1).unwrap_or(u32::MAX));
            }
            if bytes.get(end) != Some(&b'\\') {
                break;
            }
            end += 1;
            if bytes.get(end) == Some(&b'\r') {
                end += 1;
            }
            if bytes.get(end) != Some(&b'\n') {
                break;
            }
            end += 1;
        }
        self.source_position(u32::try_from(original_end).unwrap_or(u32::MAX))
    }

    fn source_position(&self, offset: u32) -> (i32, i32) {
        let offset = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.source.len());
        let prefix = &self.source.as_bytes()[..offset];
        let mut line = 1;
        for byte in prefix {
            if *byte == b'\n' {
                line += 1;
            }
        }
        let line_start = prefix
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |position| position + 1);
        (line, i32::try_from(offset - line_start).unwrap_or(i32::MAX))
    }

    fn source_text(&self, range: ruff_text_size::TextRange) -> &str {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        &self.source[start..end]
    }

    fn string_literal_constant(&self, string: &ruff_python_ast::ExprStringLiteral) -> Constant {
        let mut bytes = Vec::new();
        let mut has_surrogates = false;
        for part in &string.value {
            if let Some(part_bytes) =
                explicit_surrogate_string(part.as_str(), self.source_text(part.range))
            {
                bytes.extend(part_bytes);
                has_surrogates = true;
            } else {
                bytes.extend_from_slice(part.as_str().as_bytes());
            }
        }
        if has_surrogates {
            Constant::SurrogateString(bytes)
        } else {
            Constant::String(string.value.to_str().to_string())
        }
    }

    fn generator_expression_range(
        &self,
        range: ruff_text_size::TextRange,
    ) -> ruff_text_size::TextRange {
        let bytes = self.source.as_bytes();
        let original_start = usize::from(range.start());
        let original_end = usize::from(range.end());
        if range_is_wrapped_in_parentheses(&self.source, original_start, original_end) {
            return range;
        }
        let mut start = usize::from(range.start());
        loop {
            while start > 0 && bytes[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            let line_start = bytes[..start]
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |position| position + 1);
            let comment_line = bytes[line_start..start]
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(&b'#');
            if !comment_line {
                break;
            }
            start = line_start;
        }
        if start > 0 && bytes[start - 1] == b'(' {
            start -= 1;
        } else {
            start = usize::from(range.start());
        }

        let mut end = usize::from(range.end());
        loop {
            while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if bytes.get(end) != Some(&b'#') {
                break;
            }
            while end < bytes.len() && !matches!(bytes[end], b'\n' | b'\r') {
                end += 1;
            }
        }
        if end < bytes.len() && bytes[end] == b')' {
            end += 1;
        } else {
            end = usize::from(range.end());
        }

        ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(u32::try_from(start).unwrap_or(u32::MAX)),
            ruff_text_size::TextSize::new(u32::try_from(end).unwrap_or(u32::MAX)),
        )
    }

    fn interpolated_string_content_range(
        &self,
        range: ruff_text_size::TextRange,
    ) -> ruff_text_size::TextRange {
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let text = &self.source.as_bytes()[start..end];
        let Some(quote_offset) = text.iter().position(|byte| matches!(byte, b'\'' | b'"')) else {
            return range;
        };
        let quote = text[quote_offset];
        let quote_len = if text.get(quote_offset..quote_offset + 3) == Some(&[quote; 3]) {
            3
        } else {
            1
        };
        let content_start = start + quote_offset + quote_len;
        let has_closing_quote = text.len() >= quote_len
            && text[text.len() - quote_len..]
                .iter()
                .all(|byte| *byte == quote);
        let content_end = if has_closing_quote {
            end - quote_len
        } else {
            end
        };
        ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(u32::try_from(content_start).unwrap_or(u32::MAX)),
            ruff_text_size::TextSize::new(u32::try_from(content_end).unwrap_or(u32::MAX)),
        )
    }

    fn fstring_result_range(
        &self,
        fstring: &ruff_python_ast::ExprFString,
    ) -> ruff_text_size::TextRange {
        let mut component_count = 0;
        let mut pending_literal = false;
        let mut only_interpolation = None;
        for part in &fstring.value {
            match part {
                FStringPart::Literal(literal) => {
                    pending_literal |= !literal.value.is_empty();
                }
                FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                pending_literal |= !literal.value.is_empty();
                            }
                            InterpolatedStringElement::Interpolation(interpolation) => {
                                if interpolation.debug_text.is_some() {
                                    pending_literal = true;
                                }
                                if std::mem::take(&mut pending_literal) {
                                    component_count += 1;
                                }
                                component_count += 1;
                                only_interpolation = Some(interpolation.range);
                            }
                        }
                    }
                }
            }
        }
        if pending_literal {
            component_count += 1;
            only_interpolation = None;
        }
        if component_count == 1 {
            only_interpolation.unwrap_or_else(|| {
                fstring_literal_span(fstring)
                    .unwrap_or_else(|| self.interpolated_string_content_range(fstring.range))
            })
        } else {
            fstring.range
        }
    }

    fn attribute_opcode_range(
        &self,
        attribute: &ruff_python_ast::ExprAttribute,
    ) -> ruff_text_size::TextRange {
        let location = self.source_location(attribute.range);
        if location.line == location.end_line {
            attribute.range
        } else {
            attribute.attr.range()
        }
    }

    fn call_opcode_range(&self, call: &ruff_python_ast::ExprCall) -> ruff_text_size::TextRange {
        if let Some(attribute) = self.direct_method_attribute(call) {
            let attribute_location = self.source_location(attribute.range);
            if attribute_location.line != attribute_location.end_line {
                return ruff_text_size::TextRange::new(
                    attribute.attr.range().start(),
                    call.range.end(),
                );
            }
        }
        call.range
    }

    fn direct_method_attribute<'a>(
        &self,
        call: &'a ruff_python_ast::ExprCall,
    ) -> Option<&'a ruff_python_ast::ExprAttribute> {
        if call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
            || call.arguments.keywords.len() > 15
            || call.arguments.args.len() + call.arguments.keywords.len() >= 30
        {
            return None;
        }
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            return None;
        };
        (!self.imported_module_attribute(attribute)).then_some(attribute)
    }

    fn pre_register_suite_names(&mut self, body: &[Stmt]) -> Result<(), CompileError> {
        #[derive(Default)]
        struct Collector {
            names: Vec<(String, bool)>,
        }

        impl<'ast> Visitor<'ast> for Collector {
            fn visit_stmt(&mut self, statement: &'ast Stmt) {
                match statement {
                    Stmt::AnnAssign(assignment) => {
                        if let Some(value) = &assignment.value {
                            self.visit_expr(value);
                            self.visit_expr(&assignment.target);
                        }
                        return;
                    }
                    Stmt::FunctionDef(definition) => {
                        for decorator in &definition.decorator_list {
                            self.visit_expr(&decorator.expression);
                        }
                        for parameter in definition
                            .parameters
                            .posonlyargs
                            .iter()
                            .chain(&definition.parameters.args)
                            .chain(&definition.parameters.kwonlyargs)
                        {
                            if let Some(default) = &parameter.default {
                                self.visit_expr(default);
                            }
                        }
                        self.names
                            .push((definition.name.as_str().to_string(), false));
                        return;
                    }
                    Stmt::ClassDef(definition) => {
                        for decorator in &definition.decorator_list {
                            self.visit_expr(&decorator.expression);
                        }
                        if definition.type_params.is_none()
                            && let Some(arguments) = definition.arguments.as_deref()
                        {
                            for argument in &arguments.args {
                                self.visit_expr(argument);
                            }
                            for keyword in &arguments.keywords {
                                self.visit_expr(&keyword.value);
                            }
                        }
                        self.names
                            .push((definition.name.as_str().to_string(), false));
                        return;
                    }
                    Stmt::TypeAlias(statement) => {
                        if let Expr::Name(name) = statement.name.as_ref() {
                            self.names.push((name.id.as_str().to_string(), false));
                        }
                        return;
                    }
                    Stmt::Import(import) => {
                        for alias in &import.names {
                            let imported = alias.name.as_str();
                            self.names.push((imported.to_string(), true));
                            let bound = alias.asname.as_ref().map_or_else(
                                || imported.split('.').next().unwrap_or(imported),
                                |name| name.as_str(),
                            );
                            self.names.push((bound.to_string(), false));
                        }
                        return;
                    }
                    Stmt::ImportFrom(import) => {
                        if let Some(module) = &import.module {
                            self.names.push((module.as_str().to_string(), true));
                        }
                        for alias in &import.names {
                            self.names.push((alias.name.as_str().to_string(), true));
                            let bound = alias.asname.as_ref().unwrap_or(&alias.name);
                            self.names.push((bound.as_str().to_string(), false));
                        }
                        return;
                    }
                    _ => {}
                }
                walk_stmt(self, statement);
            }

            fn visit_except_handler(
                &mut self,
                except_handler: &'ast ruff_python_ast::ExceptHandler,
            ) {
                let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = except_handler;
                if let Some(type_) = &handler.type_ {
                    self.visit_expr(type_);
                }
                if let Some(name) = &handler.name {
                    self.names.push((name.as_str().to_string(), false));
                }
                for statement in &handler.body {
                    self.visit_stmt(statement);
                }
            }

            fn visit_expr(&mut self, expression: &'ast Expr) {
                match expression {
                    Expr::Name(name) => self.names.push((name.id.as_str().to_string(), false)),
                    Expr::Attribute(attribute) => {
                        self.visit_expr(&attribute.value);
                        self.names.push((attribute.attr.as_str().to_string(), true));
                        return;
                    }
                    Expr::Lambda(lambda) => {
                        if let Some(parameters) = lambda.parameters.as_deref() {
                            for parameter in parameters
                                .posonlyargs
                                .iter()
                                .chain(&parameters.args)
                                .chain(&parameters.kwonlyargs)
                            {
                                if let Some(default) = &parameter.default {
                                    self.visit_expr(default);
                                }
                            }
                        }
                        return;
                    }
                    _ => {}
                }
                walk_expr(self, expression);
            }

            fn visit_pattern(&mut self, pattern: &'ast Pattern) {
                let capture = match pattern {
                    Pattern::MatchAs(pattern) => pattern.name.as_ref(),
                    Pattern::MatchStar(pattern) => pattern.name.as_ref(),
                    Pattern::MatchMapping(pattern) => pattern.rest.as_ref(),
                    _ => None,
                };
                if let Some(name) = capture {
                    self.names.push((name.as_str().to_string(), false));
                }
                walk_pattern(self, pattern);
            }
        }

        let mut collector = Collector::default();
        for statement in body {
            collector.visit_stmt(statement);
        }
        for (name, force_name) in collector.names {
            let is_fast_local = matches!(
                &self.scope,
                Scope::Function { indices, globals, .. }
                    if indices.contains_key(&name) && !globals.contains(&name)
            ) || self.temporary_indices.contains_key(&name);
            if force_name || !is_fast_local {
                self.name_index(&name)?;
            }
        }
        Ok(())
    }

    fn pre_register_expression_names(&mut self, expression: &Expr) -> Result<(), CompileError> {
        #[derive(Default)]
        struct Collector {
            names: Vec<(String, bool)>,
        }

        impl<'ast> Visitor<'ast> for Collector {
            fn visit_expr(&mut self, expression: &'ast Expr) {
                match expression {
                    Expr::Name(name) => self.names.push((name.id.as_str().to_string(), false)),
                    Expr::Attribute(attribute) => {
                        self.visit_expr(&attribute.value);
                        self.names.push((attribute.attr.as_str().to_string(), true));
                        return;
                    }
                    _ => {}
                }
                walk_expr(self, expression);
            }
        }

        let mut collector = Collector::default();
        collector.visit_expr(expression);
        for (name, force_name) in collector.names {
            let is_fast_local = matches!(
                &self.scope,
                Scope::Function { indices, globals, .. }
                    if indices.contains_key(&name) && !globals.contains(&name)
            );
            if force_name || !is_fast_local {
                self.name_index(&name)?;
            }
        }
        Ok(())
    }

    fn pre_register_expression_constants(&mut self, expression: &Expr) -> Result<(), CompileError> {
        #[derive(Default)]
        struct Collector {
            constants: Vec<Constant>,
        }

        impl<'ast> Visitor<'ast> for Collector {
            fn visit_expr(&mut self, expression: &'ast Expr) {
                if let Some(constant) = literal_constant(expression) {
                    self.constants.push(constant);
                } else {
                    walk_expr(self, expression);
                }
            }
        }

        let mut collector = Collector::default();
        collector.visit_expr(expression);
        for constant in collector.constants {
            self.add_constant(constant)?;
        }
        Ok(())
    }

    fn name_index(&mut self, name: &str) -> Result<u32, CompileError> {
        let name = self.mangled_name(name);
        if let Some(index) = self.name_indices.get(&name) {
            return Ok(*index);
        }
        let index = to_u32(self.names.len(), "name count")?;
        self.names.push(name.clone());
        self.name_indices.insert(name, index);
        Ok(index)
    }

    fn add_constant(&mut self, constant: Constant) -> Result<u32, CompileError> {
        if let Some(index) = self
            .constants
            .iter()
            .position(|existing| constants_equal(existing, &constant))
        {
            return to_u32(index, "constant index");
        }
        let index = to_u32(self.constants.len(), "constant count")?;
        self.constants.push(constant);
        Ok(index)
    }

    fn add_interned_string_tuple(&mut self, values: Vec<Constant>) -> Result<u32, CompileError> {
        self.interned_constant_strings
            .extend(values.iter().filter_map(|value| match value {
                Constant::String(value) => Some(value.clone()),
                _ => None,
            }));
        self.add_constant(Constant::Tuple(values))
    }

    fn remove_unused_constants(&mut self) -> Result<(), CompileError> {
        let used = self.assembler.used_constant_indices(LOAD_CONST);
        let mut index_map = vec![None; self.constants.len()];
        let mut retained = Vec::with_capacity(self.constants.len());
        for (old_index, constant) in std::mem::take(&mut self.constants).into_iter().enumerate() {
            if old_index == 0 || used.contains(&u32::try_from(old_index).unwrap()) {
                let new_index = to_u32(retained.len(), "constant count")?;
                index_map[old_index] = Some(new_index);
                retained.push(constant);
            }
        }
        self.assembler
            .remap_constant_indices(LOAD_CONST, &index_map);
        self.constants = retained;
        Ok(())
    }

    fn emit_deferred_constant(&mut self, constant: Constant) -> Result<(), CompileError> {
        self.apply_stack_effect(1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(LOAD_CONST, self.depth.cast_unsigned());
        self.deferred_constants.push((Some(instruction), constant));
        Ok(())
    }

    fn emit_deferred_constant_before_return(
        &mut self,
        constant: Constant,
    ) -> Result<(), CompileError> {
        self.apply_stack_effect(1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(LOAD_CONST, self.depth.cast_unsigned());
        self.deferred_constants_before_return
            .push((instruction, constant));
        Ok(())
    }

    fn emit_deferred_store_name(&mut self, name: &str) -> Result<(), CompileError> {
        self.apply_stack_effect(-1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(STORE_NAME, self.depth.cast_unsigned());
        self.deferred_names.push((instruction, name.to_string()));
        Ok(())
    }

    fn emit(&mut self, opcode: Opcode, argument: u32, effect: i32) -> Result<(), CompileError> {
        self.apply_stack_effect(effect)?;
        self.assembler
            .emit_with_depth(opcode, argument, self.depth.cast_unsigned());
        Ok(())
    }

    fn emit_jump_forward(
        &mut self,
        opcode: Opcode,
        label: Label,
        effect: i32,
    ) -> Result<(), CompileError> {
        if matches!(opcode.code(), 75..=77)
            && let Some(location) = self.take_trailing_nop_location()
        {
            self.assembler.set_location(location);
        }
        self.apply_stack_effect(effect)?;
        self.assembler
            .emit_forward_with_depth(opcode, label, self.depth.cast_unsigned());
        Ok(())
    }

    fn emit_jump_backward(
        &mut self,
        opcode: Opcode,
        label: Label,
        effect: i32,
    ) -> Result<(), CompileError> {
        if matches!(opcode.code(), 75..=77)
            && let Some(location) = self.take_trailing_nop_location()
        {
            self.assembler.set_location(location);
        }
        self.apply_stack_effect(effect)?;
        self.assembler
            .emit_backward_with_depth(opcode, label, self.depth.cast_unsigned());
        Ok(())
    }

    fn take_trailing_nop_location(&mut self) -> Option<SourceLocation> {
        let (location, exclusion) = self.assembler.take_trailing_nop_location()?;
        if let Some(exclusion) = exclusion {
            self.generator_region_exclusions
                .retain(|candidate| *candidate != exclusion);
            for exclusions in &mut self.active_with_region_exclusions {
                exclusions.retain(|candidate| *candidate != exclusion);
            }
            for exclusions in &mut self.active_exception_region_exclusions {
                exclusions.retain(|candidate| *candidate != exclusion);
            }
            for exclusions in &mut self.active_comprehension_region_exclusions {
                exclusions.retain(|candidate| *candidate != exclusion);
            }
        }
        Some(location)
    }

    fn apply_stack_effect(&mut self, effect: i32) -> Result<(), CompileError> {
        self.depth += effect;
        if self.depth < 0 {
            return Err(CompileError::Internal(
                "compiler produced a negative stack depth".to_string(),
            ));
        }
        self.max_depth = self.max_depth.max(self.depth.cast_unsigned());
        Ok(())
    }

    fn set_depth(&mut self, depth: i32) {
        debug_assert!(depth >= 0);
        self.depth = depth;
        self.max_depth = self.max_depth.max(depth.cast_unsigned());
    }
}

#[derive(Debug, Default)]
struct LocalCollector {
    names: Vec<String>,
    seen: HashSet<String>,
    known_bindings: HashSet<String>,
    comprehension_targets: Vec<String>,
    seen_comprehension_targets: HashSet<String>,
    annotation_only: HashSet<String>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
}

impl LocalCollector {
    fn insert_comprehension_target(&mut self, name: String) {
        if self.seen_comprehension_targets.insert(name.clone()) {
            self.comprehension_targets.push(name);
        }
    }

    fn insert(&mut self, name: &str) {
        self.annotation_only.remove(name);
        if self.globals.contains(name) || self.nonlocals.contains(name) {
            return;
        }
        if self.seen.insert(name.to_string()) {
            self.names.push(name.to_string());
        }
    }

    fn insert_annotation_only(&mut self, name: &str) {
        if self.globals.contains(name) || self.nonlocals.contains(name) {
            return;
        }
        if self.seen.insert(name.to_string()) {
            self.names.push(name.to_string());
            self.annotation_only.insert(name.to_string());
        }
    }

    fn collect_globals(&mut self, body: &[Stmt]) {
        for statement in body {
            match statement {
                Stmt::Global(statement) => {
                    self.globals
                        .extend(statement.names.iter().map(|name| name.as_str().to_string()));
                }
                Stmt::Nonlocal(statement) => {
                    self.nonlocals
                        .extend(statement.names.iter().map(|name| name.as_str().to_string()));
                }
                Stmt::If(statement) => {
                    self.collect_globals(&statement.body);
                    for clause in &statement.elif_else_clauses {
                        self.collect_globals(&clause.body);
                    }
                }
                Stmt::For(statement) => {
                    self.collect_globals(&statement.body);
                    self.collect_globals(&statement.orelse);
                }
                Stmt::While(statement) => {
                    self.collect_globals(&statement.body);
                    self.collect_globals(&statement.orelse);
                }
                Stmt::Try(statement) => {
                    self.collect_globals(&statement.body);
                    for handler in &statement.handlers {
                        if let Some(handler) = handler.as_except_handler() {
                            self.collect_globals(&handler.body);
                        }
                    }
                    self.collect_globals(&statement.orelse);
                    self.collect_globals(&statement.finalbody);
                }
                Stmt::With(statement) => self.collect_globals(&statement.body),
                Stmt::Match(statement) => {
                    for case in &statement.cases {
                        self.collect_globals(&case.body);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_suite(&mut self, body: &[Stmt]) {
        for statement in body {
            match statement {
                Stmt::Assign(assignment) => {
                    let mut references = ReferenceCollector::default();
                    references.visit_expr(&assignment.value);
                    for target in &assignment.targets {
                        let mut names = Vec::new();
                        collect_target_names(target, &mut names);
                        for name in names {
                            if references.references.contains(name.as_str()) {
                                self.insert(&name);
                            }
                        }
                    }
                    self.collect_expression(&assignment.value);
                    for target in &assignment.targets {
                        self.collect_target(target);
                    }
                }
                Stmt::AugAssign(assignment) => {
                    self.collect_expression(&assignment.value);
                    if let Expr::Name(name) = assignment.target.as_ref() {
                        self.insert(name.id.as_str());
                    }
                }
                Stmt::AnnAssign(assignment) => {
                    if let Some(value) = &assignment.value {
                        self.collect_expression(value);
                    }
                    if (assignment.simple || assignment.value.is_some())
                        && let Expr::Name(name) = assignment.target.as_ref()
                    {
                        if assignment.value.is_some() {
                            self.insert(name.id.as_str());
                        } else {
                            self.insert_annotation_only(name.id.as_str());
                        }
                    }
                }
                Stmt::FunctionDef(definition) => self.insert(definition.name.as_str()),
                Stmt::ClassDef(definition) => self.insert(definition.name.as_str()),
                Stmt::TypeAlias(statement) => {
                    if let Expr::Name(name) = statement.name.as_ref() {
                        self.insert(name.id.as_str());
                    }
                    self.collect_expression(&statement.value);
                }
                Stmt::For(statement) => {
                    self.collect_expression(&statement.iter);
                    self.collect_target(&statement.target);
                    self.collect_suite(&statement.body);
                    self.collect_suite(&statement.orelse);
                }
                Stmt::Import(statement) => {
                    for alias in &statement.names {
                        self.insert(alias.asname.as_ref().map_or_else(
                            || alias.name.as_str().split('.').next().unwrap_or(""),
                            |name| name.as_str(),
                        ));
                    }
                }
                Stmt::ImportFrom(statement) => {
                    for alias in &statement.names {
                        if alias.name.as_str() != "*" {
                            self.insert(
                                alias
                                    .asname
                                    .as_ref()
                                    .map_or(alias.name.as_str(), |name| name.as_str()),
                            );
                        }
                    }
                }
                Stmt::If(statement) => {
                    self.collect_expression(&statement.test);
                    self.collect_suite(&statement.body);
                    for clause in &statement.elif_else_clauses {
                        if let Some(test) = &clause.test {
                            self.collect_expression(test);
                        }
                        self.collect_suite(&clause.body);
                    }
                }
                Stmt::While(statement) => {
                    self.collect_expression(&statement.test);
                    self.collect_suite(&statement.body);
                    self.collect_suite(&statement.orelse);
                }
                Stmt::Try(statement) => {
                    self.collect_suite(&statement.body);
                    for handler in &statement.handlers {
                        if let Some(handler) = handler.as_except_handler() {
                            if let Some(name) = &handler.name {
                                self.insert(name.as_str());
                            }
                            if let Some(exception_type) = &handler.type_ {
                                self.collect_expression(exception_type);
                            }
                            self.collect_suite(&handler.body);
                        }
                    }
                    self.collect_suite(&statement.orelse);
                    self.collect_suite(&statement.finalbody);
                }
                Stmt::With(statement) => {
                    for item in &statement.items {
                        self.collect_expression(&item.context_expr);
                        if let Some(target) = &item.optional_vars {
                            self.collect_target(target);
                        }
                    }
                    self.collect_suite(&statement.body);
                }
                Stmt::Match(statement) => {
                    if let Expr::Name(name) = statement.subject.as_ref()
                        && name.ctx == ExprContext::Load
                        && self.known_bindings.contains(name.id.as_str())
                    {
                        self.insert(name.id.as_str());
                    }
                    self.collect_expression(&statement.subject);
                    for case in &statement.cases {
                        self.collect_pattern(&case.pattern);
                        if let Some(guard) = &case.guard {
                            self.collect_expression(guard);
                        }
                        self.collect_suite(&case.body);
                    }
                }
                Stmt::Assert(statement) => {
                    self.collect_expression(&statement.test);
                    if let Some(message) = &statement.msg {
                        self.collect_expression(message);
                    }
                }
                Stmt::Expr(statement) => self.collect_expression(&statement.value),
                Stmt::Raise(statement) => {
                    if let Some(exception) = &statement.exc {
                        self.collect_expression(exception);
                    }
                    if let Some(cause) = &statement.cause {
                        self.collect_expression(cause);
                    }
                }
                Stmt::Return(statement) => {
                    if let Some(value) = &statement.value {
                        self.collect_expression(value);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_target(&mut self, target: &Expr) {
        match target {
            Expr::Name(name) => self.insert(name.id.as_str()),
            Expr::List(list) => {
                for element in &list.elts {
                    self.collect_target(element);
                }
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.collect_target(element);
                }
            }
            Expr::Starred(starred) => self.collect_target(&starred.value),
            _ => {}
        }
    }

    fn collect_expression(&mut self, expression: &Expr) {
        NamedExpressionCollector {
            collector: self,
            in_inlined_comprehension: false,
            collect_known_loads: true,
        }
        .visit_expr(expression);
    }

    fn collect_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::MatchValue(pattern) => self.collect_expression(&pattern.value),
            Pattern::MatchSingleton(_) => {}
            Pattern::MatchSequence(pattern) => {
                for pattern in &pattern.patterns {
                    self.collect_pattern(pattern);
                }
            }
            Pattern::MatchMapping(pattern) => {
                for key in &pattern.keys {
                    self.collect_expression(key);
                }
                for pattern in &pattern.patterns {
                    self.collect_pattern(pattern);
                }
                if let Some(rest) = &pattern.rest {
                    self.insert(rest.as_str());
                }
            }
            Pattern::MatchClass(pattern) => {
                self.collect_expression(&pattern.cls);
                for argument in &pattern.arguments.patterns {
                    self.collect_pattern(argument);
                }
                for keyword in &pattern.arguments.keywords {
                    self.collect_pattern(&keyword.pattern);
                }
            }
            Pattern::MatchStar(pattern) => {
                if let Some(name) = &pattern.name {
                    self.insert(name.as_str());
                }
            }
            Pattern::MatchAs(pattern) => {
                if let Some(inner) = &pattern.pattern {
                    self.collect_pattern(inner);
                }
                if let Some(name) = &pattern.name {
                    self.insert(name.as_str());
                }
            }
            Pattern::MatchOr(pattern) => {
                for pattern in &pattern.patterns {
                    self.collect_pattern(pattern);
                }
            }
        }
    }
}

struct NamedExpressionCollector<'a> {
    collector: &'a mut LocalCollector,
    in_inlined_comprehension: bool,
    collect_known_loads: bool,
}

impl<'ast> Visitor<'ast> for NamedExpressionCollector<'_> {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if matches!(expression, Expr::Lambda(_)) {
            return;
        }
        if matches!(expression, Expr::Generator(_)) {
            let previous = std::mem::replace(&mut self.in_inlined_comprehension, false);
            let previous_loads = std::mem::replace(&mut self.collect_known_loads, false);
            walk_expr(self, expression);
            self.collect_known_loads = previous_loads;
            self.in_inlined_comprehension = previous;
            return;
        }
        if self.collect_known_loads
            && let Expr::Name(name) = expression
            && name.ctx == ExprContext::Load
            && self.collector.known_bindings.contains(name.id.as_str())
        {
            self.collector.insert(name.id.as_str());
        }
        let generators = match expression {
            Expr::ListComp(comprehension) => Some(comprehension.generators.as_slice()),
            Expr::SetComp(comprehension) => Some(comprehension.generators.as_slice()),
            Expr::DictComp(comprehension) => Some(comprehension.generators.as_slice()),
            _ => None,
        };
        if let Some(generators) = generators {
            for generator in generators {
                let mut names = Vec::new();
                collect_target_names(&generator.target, &mut names);
                for name in names {
                    self.collector.insert_comprehension_target(name);
                }
                self.collector.collect_target(&generator.target);
            }
            let previous = std::mem::replace(&mut self.in_inlined_comprehension, true);
            walk_expr(self, expression);
            self.in_inlined_comprehension = previous;
            return;
        }
        if let Expr::Named(named) = expression {
            if self.in_inlined_comprehension
                && let Expr::Name(name) = named.target.as_ref()
            {
                self.collector
                    .insert_comprehension_target(name.id.as_str().to_string());
            }
            self.collector.collect_target(&named.target);
        }
        walk_expr(self, expression);
    }
}

#[derive(Default)]
struct ReferenceCollector {
    references: HashSet<String>,
    skip_annotations: bool,
    explicit_dunder_class_reference: bool,
}

impl<'ast> Visitor<'ast> for ReferenceCollector {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
            return;
        }
        if self.skip_annotations
            && let Stmt::AnnAssign(annotation) = statement
        {
            self.visit_expr(&annotation.target);
            if let Some(value) = &annotation.value {
                self.visit_expr(value);
            }
            return;
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if matches!(expression, Expr::Lambda(_)) {
            return;
        }
        if let Expr::Name(name) = expression {
            if name.ctx == ExprContext::Load {
                self.references.insert(name.id.as_str().to_string());
                if name.id.as_str() == "super" {
                    self.references.insert("__class__".to_string());
                } else if name.id.as_str() == "__class__" {
                    self.explicit_dunder_class_reference = true;
                }
            }
        }
        walk_expr(self, expression);
    }
}

fn definitely_evaluated_references(expression: &Expr) -> Option<HashSet<String>> {
    #[derive(Default)]
    struct ConditionalEvaluationDetector(bool);

    impl<'ast> Visitor<'ast> for ConditionalEvaluationDetector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(
                expression,
                Expr::BoolOp(_)
                    | Expr::If(_)
                    | Expr::Lambda(_)
                    | Expr::Generator(_)
                    | Expr::ListComp(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            ) || matches!(expression, Expr::Compare(comparison) if comparison.ops.len() > 1)
            {
                self.0 = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut detector = ConditionalEvaluationDetector::default();
    detector.visit_expr(expression);
    if detector.0 {
        return None;
    }
    let mut references = ReferenceCollector::default();
    references.visit_expr(expression);
    Some(references.references)
}

struct LambdaScopeAnalysis {
    locals: Vec<String>,
    references: HashSet<String>,
    cellvars: HashSet<String>,
    required: BTreeSet<String>,
}

fn analyze_lambda_scope(lambda: &ruff_python_ast::ExprLambda) -> LambdaScopeAnalysis {
    let empty_parameters = ruff_python_ast::Parameters::default();
    let parameters = lambda.parameters.as_deref().unwrap_or(&empty_parameters);
    let mut locals = LocalCollector::default();
    for parameter in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
    {
        locals.insert(parameter.name().as_str());
    }
    if let Some(parameter) = &parameters.vararg {
        locals.insert(parameter.name.as_str());
    }
    if let Some(parameter) = &parameters.kwarg {
        locals.insert(parameter.name.as_str());
    }
    locals.collect_expression(&lambda.body);

    let mut references = ReferenceCollector::default();
    references.visit_expr(&lambda.body);
    let nested_requirements = nested_lambda_required_names_in_expression(&lambda.body);
    let local_names = locals.names.iter().cloned().collect::<HashSet<_>>();
    let cellvars = nested_requirements
        .iter()
        .filter(|name| local_names.contains(*name))
        .cloned()
        .collect();
    let required = references
        .references
        .iter()
        .chain(&nested_requirements)
        .filter(|name| !local_names.contains(*name))
        .cloned()
        .collect();
    LambdaScopeAnalysis {
        locals: locals.names,
        references: references.references,
        cellvars,
        required,
    }
}

fn nested_lambda_required_names_in_expression(expression: &Expr) -> BTreeSet<String> {
    #[derive(Default)]
    struct Collector {
        required: BTreeSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Lambda(lambda) = expression {
                self.required.extend(analyze_lambda_scope(lambda).required);
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.required
}

fn type_parameter_dependency_names(
    type_params: &ruff_python_ast::TypeParams,
    type_names: &BTreeSet<String>,
) -> HashSet<String> {
    let mut references = ReferenceCollector::default();
    for parameter in type_params {
        if let TypeParam::TypeVar(parameter) = parameter
            && let Some(bound) = &parameter.bound
        {
            references.visit_expr(bound);
        }
        if let Some(default) = parameter.default() {
            references.visit_expr(default);
        }
    }
    references
        .references
        .into_iter()
        .filter(|name| type_names.contains(name))
        .collect()
}

fn nested_lambda_required_names_in_suite(body: &[Stmt]) -> BTreeSet<String> {
    #[derive(Default)]
    struct Collector {
        required: BTreeSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Lambda(lambda) = expression {
                self.required.extend(analyze_lambda_scope(lambda).required);
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.required
}

fn class_required_names(definition: &StmtClassDef, future_annotations: bool) -> BTreeSet<String> {
    let mut locals = LocalCollector::default();
    locals.collect_globals(&definition.body);
    locals.collect_suite(&definition.body);

    let mut references = ReferenceCollector {
        skip_annotations: future_annotations,
        ..ReferenceCollector::default()
    };
    for statement in &definition.body {
        references.visit_stmt(statement);
    }
    let mut required = references
        .references
        .into_iter()
        .filter(|name| {
            locals.nonlocals.contains(name)
                || (!locals.globals.contains(name) && !locals.seen.contains(name))
        })
        .collect::<BTreeSet<_>>();
    required.extend(locals.nonlocals.iter().cloned());
    let body_requires_dunder_class = required.contains("__class__");

    let mut methods = Vec::new();
    collect_nested_functions(&definition.body, &mut methods, future_annotations);
    for mut method in methods {
        required.extend(method.resolve());
        if !future_annotations {
            required.extend(
                method
                    .annotation_references
                    .iter()
                    .filter(|name| !locals.seen.contains(*name) && !locals.globals.contains(*name))
                    .cloned(),
            );
        }
    }
    required.extend(nested_class_required_names(
        &definition.body,
        future_annotations,
    ));
    if !body_requires_dunder_class && !locals.nonlocals.contains("__class__") {
        required.remove("__class__");
    }
    required.remove("__classdict__");
    required.remove("__conditional_annotations__");
    required
}

fn nested_class_required_names(body: &[Stmt], future_annotations: bool) -> BTreeSet<String> {
    let mut required = BTreeSet::new();
    for statement in body {
        match statement {
            Stmt::ClassDef(definition) => {
                required.extend(class_required_names(definition, future_annotations));
            }
            Stmt::FunctionDef(_) => {}
            Stmt::If(statement) => {
                required.extend(nested_class_required_names(
                    &statement.body,
                    future_annotations,
                ));
                for clause in &statement.elif_else_clauses {
                    required.extend(nested_class_required_names(
                        &clause.body,
                        future_annotations,
                    ));
                }
            }
            Stmt::For(statement) => {
                required.extend(nested_class_required_names(
                    &statement.body,
                    future_annotations,
                ));
                required.extend(nested_class_required_names(
                    &statement.orelse,
                    future_annotations,
                ));
            }
            Stmt::While(statement) => {
                required.extend(nested_class_required_names(
                    &statement.body,
                    future_annotations,
                ));
                required.extend(nested_class_required_names(
                    &statement.orelse,
                    future_annotations,
                ));
            }
            Stmt::Try(statement) => {
                required.extend(nested_class_required_names(
                    &statement.body,
                    future_annotations,
                ));
                for handler in &statement.handlers {
                    if let Some(handler) = handler.as_except_handler() {
                        required.extend(nested_class_required_names(
                            &handler.body,
                            future_annotations,
                        ));
                    }
                }
                required.extend(nested_class_required_names(
                    &statement.orelse,
                    future_annotations,
                ));
                required.extend(nested_class_required_names(
                    &statement.finalbody,
                    future_annotations,
                ));
            }
            Stmt::With(statement) => {
                required.extend(nested_class_required_names(
                    &statement.body,
                    future_annotations,
                ));
            }
            Stmt::Match(statement) => {
                for case in &statement.cases {
                    required.extend(nested_class_required_names(&case.body, future_annotations));
                }
            }
            _ => {}
        }
    }
    required
}

fn class_needs_class_closure(body: &[Stmt], future_annotations: bool) -> bool {
    fn suite_needs_class_closure(body: &[Stmt], future_annotations: bool) -> bool {
        body.iter().any(|statement| match statement {
            Stmt::FunctionDef(definition) => {
                let mut plan = FunctionPlan::build(definition, future_annotations);
                plan.resolve().contains("__class__")
            }
            Stmt::ClassDef(definition) => {
                class_required_names(definition, future_annotations).contains("__class__")
            }
            Stmt::If(statement) => {
                suite_needs_class_closure(&statement.body, future_annotations)
                    || statement
                        .elif_else_clauses
                        .iter()
                        .any(|clause| suite_needs_class_closure(&clause.body, future_annotations))
            }
            Stmt::For(statement) => {
                suite_needs_class_closure(&statement.body, future_annotations)
                    || suite_needs_class_closure(&statement.orelse, future_annotations)
            }
            Stmt::While(statement) => {
                suite_needs_class_closure(&statement.body, future_annotations)
                    || suite_needs_class_closure(&statement.orelse, future_annotations)
            }
            Stmt::Try(statement) => {
                suite_needs_class_closure(&statement.body, future_annotations)
                    || statement
                        .handlers
                        .iter()
                        .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                        .any(|handler| suite_needs_class_closure(&handler.body, future_annotations))
                    || suite_needs_class_closure(&statement.orelse, future_annotations)
                    || suite_needs_class_closure(&statement.finalbody, future_annotations)
            }
            Stmt::With(statement) => suite_needs_class_closure(&statement.body, future_annotations),
            Stmt::Match(statement) => statement
                .cases
                .iter()
                .any(|case| suite_needs_class_closure(&case.body, future_annotations)),
            _ => false,
        })
    }

    suite_needs_class_closure(body, future_annotations)
        || nested_lambda_required_names_in_suite(body).contains("__class__")
}

fn collect_nested_functions(
    body: &[Stmt],
    functions: &mut Vec<FunctionPlan>,
    future_annotations: bool,
) {
    for statement in body {
        match statement {
            Stmt::FunctionDef(definition) => {
                functions.push(FunctionPlan::build(definition, future_annotations));
            }
            Stmt::If(statement) => {
                collect_nested_functions(&statement.body, functions, future_annotations);
                for clause in &statement.elif_else_clauses {
                    collect_nested_functions(&clause.body, functions, future_annotations);
                }
            }
            Stmt::For(statement) => {
                collect_nested_functions(&statement.body, functions, future_annotations);
                collect_nested_functions(&statement.orelse, functions, future_annotations);
            }
            Stmt::While(statement) => {
                collect_nested_functions(&statement.body, functions, future_annotations);
                collect_nested_functions(&statement.orelse, functions, future_annotations);
            }
            Stmt::Try(statement) => {
                collect_nested_functions(&statement.body, functions, future_annotations);
                for handler in &statement.handlers {
                    if let Some(handler) = handler.as_except_handler() {
                        collect_nested_functions(&handler.body, functions, future_annotations);
                    }
                }
                collect_nested_functions(&statement.orelse, functions, future_annotations);
                collect_nested_functions(&statement.finalbody, functions, future_annotations);
            }
            Stmt::With(statement) => {
                collect_nested_functions(&statement.body, functions, future_annotations);
            }
            Stmt::Match(statement) => {
                for case in &statement.cases {
                    collect_nested_functions(&case.body, functions, future_annotations);
                }
            }
            _ => {}
        }
    }
}

fn contains_function_definition(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::FunctionDef(_) => true,
        Stmt::If(statement) => {
            contains_function_definition(&statement.body)
                || statement
                    .elif_else_clauses
                    .iter()
                    .any(|clause| contains_function_definition(&clause.body))
        }
        Stmt::For(statement) => {
            contains_function_definition(&statement.body)
                || contains_function_definition(&statement.orelse)
        }
        Stmt::While(statement) => {
            contains_function_definition(&statement.body)
                || contains_function_definition(&statement.orelse)
        }
        Stmt::Try(statement) => {
            contains_function_definition(&statement.body)
                || statement
                    .handlers
                    .iter()
                    .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                    .any(|handler| contains_function_definition(&handler.body))
                || contains_function_definition(&statement.orelse)
                || contains_function_definition(&statement.finalbody)
        }
        Stmt::With(statement) => contains_function_definition(&statement.body),
        Stmt::Match(statement) => statement
            .cases
            .iter()
            .any(|case| contains_function_definition(&case.body)),
        _ => false,
    })
}

fn contains_type_alias(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::TypeAlias(_) => true,
        Stmt::If(statement) => {
            contains_type_alias(&statement.body)
                || statement
                    .elif_else_clauses
                    .iter()
                    .any(|clause| contains_type_alias(&clause.body))
        }
        Stmt::For(statement) => {
            contains_type_alias(&statement.body) || contains_type_alias(&statement.orelse)
        }
        Stmt::While(statement) => {
            contains_type_alias(&statement.body) || contains_type_alias(&statement.orelse)
        }
        Stmt::Try(statement) => {
            contains_type_alias(&statement.body)
                || statement
                    .handlers
                    .iter()
                    .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                    .any(|handler| contains_type_alias(&handler.body))
                || contains_type_alias(&statement.orelse)
                || contains_type_alias(&statement.finalbody)
        }
        Stmt::With(statement) => contains_type_alias(&statement.body),
        Stmt::Match(statement) => statement
            .cases
            .iter()
            .any(|case| contains_type_alias(&case.body)),
        _ => false,
    })
}

fn class_static_attributes(body: &[Stmt]) -> Vec<String> {
    #[derive(Default)]
    struct Collector {
        attributes: BTreeSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            // A nested class becomes the nearest enclosing class compiler for
            // any `self.attr` stores in its body.
            if matches!(statement, Stmt::ClassDef(_)) {
                return;
            }
            // CPython's augmented-assignment lowering and an annotation without
            // a value never pass the attribute through the ordinary Store path.
            if matches!(statement, Stmt::AugAssign(_))
                || matches!(statement, Stmt::AnnAssign(annotation) if annotation.value.is_none())
            {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Attribute(attribute) = expression
                && attribute.ctx == ExprContext::Store
                && matches!(
                    attribute.value.as_ref(),
                    Expr::Name(name) if name.id.as_str() == "self"
                )
            {
                self.attributes.insert(attribute.attr.as_str().to_string());
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.attributes.into_iter().collect()
}

fn collect_target_names(target: &Expr, names: &mut Vec<String>) {
    match target {
        Expr::Name(name) => names.push(name.id.as_str().to_string()),
        Expr::List(list) => {
            for element in &list.elts {
                collect_target_names(element, names);
            }
        }
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                collect_target_names(element, names);
            }
        }
        Expr::Starred(starred) => collect_target_names(&starred.value, names),
        _ => {}
    }
}

fn collect_nested_comprehension_target_names(expression: &Expr, names: &mut Vec<String>) {
    struct Collector<'a> {
        names: &'a mut Vec<String>,
    }

    impl<'ast> Visitor<'ast> for Collector<'_> {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            let generators = match expression {
                Expr::ListComp(comprehension) => Some(comprehension.generators.as_slice()),
                Expr::SetComp(comprehension) => Some(comprehension.generators.as_slice()),
                Expr::DictComp(comprehension) => Some(comprehension.generators.as_slice()),
                Expr::Generator(_) | Expr::Lambda(_) => return,
                _ => None,
            };
            if let Some(generators) = generators {
                for generator in generators {
                    collect_target_names(&generator.target, self.names);
                }
            }
            walk_expr(self, expression);
        }
    }

    Collector { names }.visit_expr(expression);
}

fn collect_named_expression_target_names(expression: &Expr, names: &mut Vec<String>) {
    struct Collector<'a> {
        names: &'a mut Vec<String>,
    }

    impl<'ast> Visitor<'ast> for Collector<'_> {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Named(named) = expression
                && let Expr::Name(name) = named.target.as_ref()
            {
                self.names.push(name.id.as_str().to_string());
            }
            if matches!(expression, Expr::Generator(_) | Expr::Lambda(_)) {
                return;
            }
            walk_expr(self, expression);
        }
    }

    Collector { names }.visit_expr(expression);
}

fn generator_named_targets(generator: &ruff_python_ast::ExprGenerator) -> HashSet<String> {
    #[derive(Default)]
    struct Collector {
        names: HashSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Named(named) = expression
                && let Expr::Name(name) = named.target.as_ref()
            {
                self.names.insert(name.id.as_str().to_string());
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(&generator.elt);
    for comprehension in &generator.generators {
        collector.visit_expr(&comprehension.iter);
        for condition in &comprehension.ifs {
            collector.visit_expr(condition);
        }
    }
    collector.names
}

fn generator_required_names(generator: &ruff_python_ast::ExprGenerator) -> BTreeSet<String> {
    let mut local_names = HashSet::new();
    for comprehension in &generator.generators {
        let mut names = Vec::new();
        collect_target_names(&comprehension.target, &mut names);
        local_names.extend(names);
    }

    let mut references = ReferenceCollector::default();
    references.visit_expr(&generator.elt);
    for (index, comprehension) in generator.generators.iter().enumerate() {
        // The outermost iterable is evaluated by the enclosing scope and passed
        // to the generator as its implicit `.0` argument.
        if index > 0 {
            references.visit_expr(&comprehension.iter);
        }
        for condition in &comprehension.ifs {
            references.visit_expr(condition);
        }
    }
    references
        .references
        .extend(generator_named_targets(generator));
    references
        .references
        .into_iter()
        .filter(|name| !local_names.contains(name))
        .collect()
}

fn generator_cellvars(generator: &ruff_python_ast::ExprGenerator) -> HashSet<String> {
    let mut local_names = HashSet::new();
    for comprehension in &generator.generators {
        let mut names = Vec::new();
        collect_target_names(&comprehension.target, &mut names);
        local_names.extend(names);
    }

    let mut nested_requirements = nested_lambda_required_names_in_expression(&generator.elt);
    for (index, comprehension) in generator.generators.iter().enumerate() {
        if index > 0 {
            nested_requirements.extend(nested_lambda_required_names_in_expression(
                &comprehension.iter,
            ));
        }
        for condition in &comprehension.ifs {
            nested_requirements.extend(nested_lambda_required_names_in_expression(condition));
        }
    }
    nested_requirements
        .into_iter()
        .filter(|name| local_names.contains(name))
        .collect()
}

fn nested_generator_required_names_in_suite(body: &[Stmt]) -> BTreeSet<String> {
    #[derive(Default)]
    struct Collector {
        required: BTreeSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Generator(generator) = expression {
                self.required.extend(generator_required_names(generator));
                return;
            }
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.required
}

fn generator_named_targets_in_suite(body: &[Stmt]) -> HashSet<String> {
    #[derive(Default)]
    struct Collector {
        names: HashSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if let Expr::Generator(generator) = expression {
                self.names.extend(generator_named_targets(generator));
                return;
            }
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.names
}

fn function_key(definition: &StmtFunctionDef) -> (u32, u32) {
    (
        u32::from(definition.range.start()),
        u32::from(definition.range.end()),
    )
}

fn explicit_surrogate_string(value: &str, source: &str) -> Option<Vec<u8>> {
    let source = source.as_bytes();
    // Ruff stores invalid Unicode escapes as replacement characters. Retain their source order
    // relative to real replacement characters so the marshal layer can restore surrogate-pass
    // UTF-8 without changing a neighboring U+FFFD.
    let replacement_character = [0xef, 0xbf, 0xbd];
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < source.len() {
        if source[index..].starts_with(&replacement_character) {
            replacements.push(None);
            index += replacement_character.len();
            continue;
        }
        if source[index] != b'\\' {
            index += 1;
            continue;
        }
        let preceding_backslashes = source[..index]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if preceding_backslashes % 2 == 1 {
            index += 1;
            continue;
        }
        let (digit_count, escape_len) = match source.get(index + 1) {
            Some(b'u') => (4, 6),
            Some(b'U') => (8, 10),
            _ => {
                index += 1;
                continue;
            }
        };
        let Some(end) = index
            .checked_add(escape_len)
            .filter(|end| *end <= source.len())
        else {
            index += 1;
            continue;
        };
        let Some(codepoint) = std::str::from_utf8(&source[index + 2..index + 2 + digit_count])
            .ok()
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        else {
            index += 1;
            continue;
        };
        if (0xd800..=0xdfff).contains(&codepoint) {
            replacements.push(Some(codepoint as u16));
        } else if codepoint == 0xfffd {
            replacements.push(None);
        }
        index = end;
    }
    if !replacements.iter().any(Option::is_some) {
        return None;
    }

    let mut bytes = value.as_bytes().to_vec();
    let mut search_start = 0;
    let mut replaced = false;
    for surrogate in replacements {
        let Some(offset) = bytes[search_start..]
            .windows(replacement_character.len())
            .position(|window| window == replacement_character)
            .map(|offset| search_start + offset)
        else {
            continue;
        };
        let Some(surrogate) = surrogate else {
            search_start = offset + replacement_character.len();
            continue;
        };
        let surrogate_bytes = [
            0xe0 | (surrogate >> 12) as u8,
            0x80 | ((surrogate >> 6) & 0x3f) as u8,
            0x80 | (surrogate & 0x3f) as u8,
        ];
        bytes.splice(
            offset..offset + replacement_character.len(),
            surrogate_bytes,
        );
        search_start = offset + surrogate_bytes.len();
        replaced = true;
    }
    replaced.then_some(bytes)
}

fn constants_equal(left: &Constant, right: &Constant) -> bool {
    match (left, right) {
        (Constant::None, Constant::None) | (Constant::Ellipsis, Constant::Ellipsis) => true,
        (Constant::Bool(left), Constant::Bool(right)) => left == right,
        (Constant::Int(left), Constant::Int(right)) => left == right,
        (Constant::SignedInt(left), Constant::SignedInt(right)) => left == right,
        (
            Constant::BigInt {
                negative: left_negative,
                digits: left_digits,
            },
            Constant::BigInt {
                negative: right_negative,
                digits: right_digits,
            },
        ) => left_negative == right_negative && left_digits == right_digits,
        (Constant::Float(left), Constant::Float(right)) => left.to_bits() == right.to_bits(),
        (
            Constant::Complex {
                real: left_real,
                imag: left_imag,
            },
            Constant::Complex {
                real: right_real,
                imag: right_imag,
            },
        ) => {
            left_real.to_bits() == right_real.to_bits()
                && left_imag.to_bits() == right_imag.to_bits()
        }
        (Constant::String(left), Constant::String(right)) => left == right,
        (Constant::SurrogateString(left), Constant::SurrogateString(right)) => left == right,
        (Constant::Bytes(left), Constant::Bytes(right)) => left == right,
        (Constant::Tuple(left), Constant::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| constants_equal(left, right))
        }
        (Constant::FrozenSet(left), Constant::FrozenSet(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|left| right.iter().any(|right| constants_equal(left, right)))
        }
        (
            Constant::Slice {
                lower: left_lower,
                upper: left_upper,
                step: left_step,
            },
            Constant::Slice {
                lower: right_lower,
                upper: right_upper,
                step: right_step,
            },
        ) => {
            constants_equal(left_lower, right_lower)
                && constants_equal(left_upper, right_upper)
                && constants_equal(left_step, right_step)
        }
        (Constant::Code(left), Constant::Code(right)) => code_objects_equal(left, right),
        (_, _) => false,
    }
}

#[derive(Debug)]
enum NumericReal {
    Integer { negative: bool, digits: Vec<u16> },
    Float(f64),
}

fn python_constants_equal(left: &Constant, right: &Constant) -> bool {
    if let (Some((left_real, left_imag)), Some((right_real, right_imag))) =
        (numeric_parts(left), numeric_parts(right))
    {
        return left_imag == right_imag && numeric_reals_equal(&left_real, &right_real);
    }
    match (left, right) {
        (Constant::Tuple(left), Constant::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| python_constants_equal(left, right))
        }
        (Constant::FrozenSet(left), Constant::FrozenSet(right)) => {
            left.len() == right.len()
                && left.iter().all(|left| {
                    right
                        .iter()
                        .any(|right| python_constants_equal(left, right))
                })
        }
        _ => constants_equal(left, right),
    }
}

fn numeric_parts(value: &Constant) -> Option<(NumericReal, f64)> {
    match value {
        Constant::Bool(value) => Some((integer_numeric_real(u64::from(*value), false), 0.0)),
        Constant::Int(value) => Some((integer_numeric_real(*value, false), 0.0)),
        Constant::SignedInt(value) => Some((
            integer_numeric_real(value.unsigned_abs(), value.is_negative()),
            0.0,
        )),
        Constant::BigInt { negative, digits } => Some((
            NumericReal::Integer {
                negative: *negative,
                digits: normalized_digits(digits.clone()),
            },
            0.0,
        )),
        Constant::Float(value) => Some((NumericReal::Float(*value), 0.0)),
        Constant::Complex { real, imag } => Some((NumericReal::Float(*real), *imag)),
        _ => None,
    }
}

fn integer_numeric_real(mut value: u64, negative: bool) -> NumericReal {
    let mut digits = Vec::new();
    while value != 0 {
        digits.push((value & 0x7fff) as u16);
        value >>= 15;
    }
    NumericReal::Integer {
        negative: negative && !digits.is_empty(),
        digits,
    }
}

fn normalized_digits(mut digits: Vec<u16>) -> Vec<u16> {
    while digits.last() == Some(&0) {
        digits.pop();
    }
    digits
}

fn numeric_reals_equal(left: &NumericReal, right: &NumericReal) -> bool {
    match (left, right) {
        (NumericReal::Float(left), NumericReal::Float(right)) => left == right,
        (
            NumericReal::Integer {
                negative: left_negative,
                digits: left_digits,
            },
            NumericReal::Integer {
                negative: right_negative,
                digits: right_digits,
            },
        ) => left_negative == right_negative && left_digits == right_digits,
        (integer @ NumericReal::Integer { .. }, NumericReal::Float(float))
        | (NumericReal::Float(float), integer @ NumericReal::Integer { .. }) => {
            float_integer_numeric_real(*float)
                .is_some_and(|float_integer| numeric_reals_equal(integer, &float_integer))
        }
    }
}

fn float_integer_numeric_real(value: f64) -> Option<NumericReal> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(integer_numeric_real(0, false));
    }

    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mut mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };

    if exponent < 0 {
        let shift = u32::try_from(-exponent).ok()?;
        if shift >= 64 || mantissa & ((1_u64 << shift) - 1) != 0 {
            return None;
        }
        mantissa >>= shift;
        return Some(integer_numeric_real(mantissa, negative));
    }

    let mut digits = match integer_numeric_real(mantissa, negative) {
        NumericReal::Integer { digits, .. } => digits,
        NumericReal::Float(_) => unreachable!(),
    };
    let shift = u32::try_from(exponent).ok()?;
    let digit_shift = usize::try_from(shift / 15).ok()?;
    let bit_shift = shift % 15;
    if digit_shift > 0 {
        digits.splice(0..0, std::iter::repeat_n(0, digit_shift));
    }
    if bit_shift > 0 {
        let mut carry = 0_u32;
        for digit in &mut digits {
            let shifted = (u32::from(*digit) << bit_shift) | carry;
            *digit = (shifted & 0x7fff) as u16;
            carry = shifted >> 15;
        }
        if carry != 0 {
            digits.push(carry as u16);
        }
    }
    Some(NumericReal::Integer { negative, digits })
}

fn code_objects_equal(left: &CodeObject, right: &CodeObject) -> bool {
    left.arg_count == right.arg_count
        && left.positional_only_arg_count == right.positional_only_arg_count
        && left.keyword_only_arg_count == right.keyword_only_arg_count
        && left.stack_size == right.stack_size
        && left.flags == right.flags
        && left.bytecode == right.bytecode
        && left.constants.len() == right.constants.len()
        && left
            .constants
            .iter()
            .zip(&right.constants)
            .all(|(left, right)| constants_equal(left, right))
        && left.names == right.names
        && left.locals == right.locals
        && left.local_kinds == right.local_kinds
        && left.filename == right.filename
        && left.name == right.name
        && left.qualified_name == right.qualified_name
        && left.first_line_number == right.first_line_number
        && left.line_table == right.line_table
        && left.exception_table == right.exception_table
        && left.annotation_thunk == right.annotation_thunk
}

fn suite_terminates(body: &[Stmt]) -> bool {
    body.iter().any(statement_terminates)
}

fn try_requires_loop_tail_inlining(statement: &ruff_python_ast::StmtTry) -> bool {
    !statement.is_star
        && statement.finalbody.is_empty()
        && statement.orelse.is_empty()
        && matches!(
            statement.body.last(),
            Some(Stmt::If(statement))
                if statement.elif_else_clauses.is_empty()
                    && !suite_terminates(&statement.body)
        )
}

fn suite_contains_yield(body: &[Stmt]) -> bool {
    #[derive(Default)]
    struct YieldCollector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for YieldCollector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            match statement {
                Stmt::FunctionDef(_) => return,
                Stmt::ClassDef(definition) => {
                    for decorator in &definition.decorator_list {
                        self.visit_expr(&decorator.expression);
                    }
                    if let Some(arguments) = definition.arguments.as_deref() {
                        for argument in &arguments.args {
                            self.visit_expr(argument);
                        }
                        for keyword in &arguments.keywords {
                            self.visit_expr(&keyword.value);
                        }
                    }
                    return;
                }
                _ => {}
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
                self.found = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = YieldCollector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.found
}

fn expression_contains_yield(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
                self.found = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}

fn expression_contains_inlined_comprehension(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if self.found || matches!(expression, Expr::Lambda(_) | Expr::Generator(_)) {
                return;
            }
            if matches!(
                expression,
                Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)
            ) {
                self.found = true;
            } else {
                walk_expr(self, expression);
            }
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}

fn optimized_generator_callable(call: &ruff_python_ast::ExprCall) -> Option<&str> {
    let Expr::Name(name) = call.func.as_ref() else {
        return None;
    };
    if !matches!(name.id.as_str(), "all" | "any" | "tuple")
        || !matches!(call.arguments.args.as_ref(), [Expr::Generator(_)])
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }
    Some(name.id.as_str())
}

fn expression_contains_await(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if matches!(expression, Expr::Await(_)) {
                self.found = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}

fn has_future_annotations(body: &[Stmt]) -> bool {
    future_feature_flags(body) & CO_FUTURE_ANNOTATIONS != 0
}

fn future_feature_flags(body: &[Stmt]) -> u32 {
    body.iter()
        .filter_map(|statement| {
            let Stmt::ImportFrom(import) = statement else {
                return None;
            };
            import
                .module
                .as_ref()
                .is_some_and(|module| module.as_str() == "__future__")
                .then_some(import)
        })
        .flat_map(|import| &import.names)
        .fold(0, |flags, alias| {
            flags
                | match alias.name.as_str() {
                    "barry_as_FLUFL" => CO_FUTURE_BARRY_AS_BDFL,
                    "annotations" => CO_FUTURE_ANNOTATIONS,
                    _ => 0,
                }
        })
}

fn module_global_names(body: &[Stmt]) -> HashSet<String> {
    #[derive(Default)]
    struct Collector {
        names: HashSet<String>,
        comprehension_depth: usize,
        nested_scope_depth: usize,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_)) {
                self.nested_scope_depth += 1;
                walk_stmt(self, statement);
                self.nested_scope_depth -= 1;
                return;
            }
            if let Stmt::Global(global) = statement {
                self.names
                    .extend(global.names.iter().map(|name| name.as_str().to_string()));
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            let is_comprehension = matches!(
                expression,
                Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_)
            );
            if is_comprehension {
                self.comprehension_depth += 1;
            }
            if self.nested_scope_depth == 0
                && self.comprehension_depth > 0
                && let Expr::Named(named) = expression
                && let Expr::Name(name) = named.target.as_ref()
            {
                self.names.insert(name.id.as_str().to_string());
            }
            walk_expr(self, expression);
            if is_comprehension {
                self.comprehension_depth -= 1;
            }
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.names
}

fn module_imported_names(body: &[Stmt]) -> HashSet<String> {
    #[derive(Default)]
    struct Collector {
        names: HashSet<String>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            match statement {
                Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let module = alias.name.as_str();
                        let bound = alias.asname.as_ref().map_or_else(
                            || module.split('.').next().unwrap_or(module),
                            |name| name.as_str(),
                        );
                        self.names.insert(bound.to_string());
                    }
                }
                Stmt::ImportFrom(import)
                    if !import
                        .module
                        .as_ref()
                        .is_some_and(|module| module.as_str() == "__future__") =>
                {
                    for alias in &import.names {
                        if alias.name.as_str() != "*" {
                            let bound = alias.asname.as_ref().unwrap_or(&alias.name);
                            self.names.insert(bound.as_str().to_string());
                        }
                    }
                }
                _ => {}
            }
            walk_stmt(self, statement);
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.names
}

fn future_annotations_statement(body: &[Stmt]) -> Option<&Stmt> {
    body.iter().find(|statement| {
        let Stmt::ImportFrom(import) = statement else {
            return false;
        };
        import
            .module
            .as_ref()
            .is_some_and(|module| module.as_str() == "__future__")
            && import
                .names
                .iter()
                .any(|alias| alias.name.as_str() == "annotations")
    })
}

fn has_simple_annotations(body: &[Stmt]) -> bool {
    let mut annotations = Vec::new();
    collect_simple_annotations(body, &mut annotations);
    !annotations.is_empty()
}

fn has_annotations(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::AnnAssign(_) => true,
        Stmt::If(statement) => {
            has_annotations(&statement.body)
                || statement
                    .elif_else_clauses
                    .iter()
                    .any(|clause| has_annotations(&clause.body))
        }
        Stmt::For(statement) => {
            has_annotations(&statement.body) || has_annotations(&statement.orelse)
        }
        Stmt::While(statement) => {
            has_annotations(&statement.body) || has_annotations(&statement.orelse)
        }
        Stmt::Try(statement) => {
            has_annotations(&statement.body)
                || statement
                    .handlers
                    .iter()
                    .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                    .any(|handler| has_annotations(&handler.body))
                || has_annotations(&statement.orelse)
                || has_annotations(&statement.finalbody)
        }
        Stmt::With(statement) => has_annotations(&statement.body),
        Stmt::Match(statement) => statement
            .cases
            .iter()
            .any(|case| has_annotations(&case.body)),
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => false,
        _ => false,
    })
}

fn collect_simple_annotations<'a>(
    body: &'a [Stmt],
    annotations: &mut Vec<&'a ruff_python_ast::StmtAnnAssign>,
) {
    for statement in body {
        match statement {
            Stmt::AnnAssign(annotation) if annotation.simple => annotations.push(annotation),
            Stmt::If(statement) => {
                collect_simple_annotations(&statement.body, annotations);
                for clause in &statement.elif_else_clauses {
                    collect_simple_annotations(&clause.body, annotations);
                }
            }
            Stmt::For(statement) => {
                collect_simple_annotations(&statement.body, annotations);
                collect_simple_annotations(&statement.orelse, annotations);
            }
            Stmt::While(statement) => {
                collect_simple_annotations(&statement.body, annotations);
                collect_simple_annotations(&statement.orelse, annotations);
            }
            _ => {}
        }
    }
}

fn literal_constant(expression: &Expr) -> Option<Constant> {
    match expression {
        Expr::NoneLiteral(_) => Some(Constant::None),
        Expr::BooleanLiteral(boolean) => Some(Constant::Bool(boolean.value)),
        Expr::EllipsisLiteral(_) => Some(Constant::Ellipsis),
        Expr::NumberLiteral(number) => Some(match &number.value {
            Number::Int(value) => value.as_u64().map_or_else(
                || Constant::BigInt {
                    negative: false,
                    digits: big_integer_digits(&value.to_string()),
                },
                Constant::Int,
            ),
            Number::Float(value) => Constant::Float(*value),
            Number::Complex { real, imag } => Constant::Complex {
                real: *real,
                imag: *imag,
            },
        }),
        Expr::StringLiteral(string) => Some(Constant::String(string.value.to_str().to_string())),
        Expr::BytesLiteral(bytes) => Some(Constant::Bytes(bytes.value.bytes().collect())),
        _ => None,
    }
}

fn debug_text_range(
    interpolation: &ruff_python_ast::InterpolatedElement,
) -> ruff_text_size::TextRange {
    let debug = interpolation
        .debug_text
        .as_ref()
        .expect("debug text range requires debug text");
    let start = u32::from(interpolation.range.start()).saturating_add(1);
    let length = debug.leading().len() + debug.expression().len() + debug.trailing().len();
    let end = start.saturating_add(u32::try_from(length).unwrap_or(u32::MAX));
    ruff_text_size::TextRange::new(
        ruff_text_size::TextSize::new(start),
        ruff_text_size::TextSize::new(end),
    )
}

fn fstring_literal_span(
    fstring: &ruff_python_ast::ExprFString,
) -> Option<ruff_text_size::TextRange> {
    let mut first = None;
    let mut last = None;
    let mut include = |range: ruff_text_size::TextRange| {
        first.get_or_insert(range.start());
        last = Some(range.end());
    };
    for part in &fstring.value {
        match part {
            FStringPart::Literal(literal) => include(literal.range),
            FStringPart::FString(fstring) => {
                for element in &fstring.elements {
                    match element {
                        InterpolatedStringElement::Literal(literal) => include(literal.range),
                        InterpolatedStringElement::Interpolation(_) => return None,
                    }
                }
            }
        }
    }
    Some(ruff_text_size::TextRange::new(first?, last?))
}

fn range_is_wrapped_in_parentheses(source: &str, start: usize, end: usize) -> bool {
    let bytes = source.as_bytes();
    if end <= start || bytes.get(start) != Some(&b'(') || bytes.get(end - 1) != Some(&b')') {
        return false;
    }

    let mut stack = Vec::new();
    let mut index = start;
    let mut quote = None;
    let mut triple_quoted = false;
    while index < end {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' {
                index = (index + 2).min(end);
                continue;
            }
            if bytes[index] == delimiter {
                if triple_quoted {
                    if bytes.get(index..index + 3) == Some(&[delimiter; 3]) {
                        index += 3;
                        quote = None;
                        triple_quoted = false;
                        continue;
                    }
                } else {
                    index += 1;
                    quote = None;
                    continue;
                }
            }
            index += 1;
            continue;
        }

        match bytes[index] {
            b'#' => {
                while index < end && !matches!(bytes[index], b'\n' | b'\r') {
                    index += 1;
                }
            }
            delimiter @ (b'\'' | b'"') => {
                triple_quoted = bytes.get(index..index + 3) == Some(&[delimiter; 3]);
                index += if triple_quoted { 3 } else { 1 };
                quote = Some(delimiter);
            }
            opening @ (b'(' | b'[' | b'{') => {
                stack.push(opening);
                index += 1;
            }
            closing @ (b')' | b']' | b'}') => {
                let Some(opening) = stack.pop() else {
                    return false;
                };
                if !matches!(
                    (opening, closing),
                    (b'(', b')') | (b'[', b']') | (b'{', b'}')
                ) {
                    return false;
                }
                index += 1;
                if stack.is_empty() {
                    return index == end;
                }
            }
            _ => index += 1,
        }
    }
    false
}

/// Removes Python comments while preserving the whitespace and newlines around
/// them, as CPython does for the source text stored by a t-string interpolation.
fn strip_expression_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut quote = None;
    let mut triple_quoted = false;

    while index < bytes.len() {
        if let Some(delimiter) = quote {
            if bytes[index] == b'\\' {
                output.push(bytes[index]);
                index += 1;
                if index < bytes.len() {
                    output.push(bytes[index]);
                    index += 1;
                }
                continue;
            }
            if bytes[index] == delimiter {
                if triple_quoted {
                    if bytes.get(index..index + 3) == Some(&[delimiter; 3]) {
                        output.extend_from_slice(&bytes[index..index + 3]);
                        index += 3;
                        quote = None;
                        triple_quoted = false;
                        continue;
                    }
                } else {
                    output.push(bytes[index]);
                    index += 1;
                    quote = None;
                    continue;
                }
            }
            output.push(bytes[index]);
            index += 1;
            continue;
        }

        match bytes[index] {
            b'#' => {
                while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                    index += 1;
                }
            }
            delimiter @ (b'\'' | b'"') => {
                triple_quoted = bytes.get(index..index + 3) == Some(&[delimiter; 3]);
                let length = if triple_quoted { 3 } else { 1 };
                output.extend_from_slice(&bytes[index..index + length]);
                index += length;
                quote = Some(delimiter);
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8(output).expect("removing ASCII comments preserves UTF-8")
}

enum PercentFormatPart<'a> {
    Literal(String),
    Formatted {
        expression: &'a Expr,
        conversion: u32,
        format_spec: Option<String>,
    },
}

fn optimized_percent_format(expression: &ExprBinOp) -> Option<Vec<PercentFormatPart<'_>>> {
    if expression.op != Operator::Mod {
        return None;
    }
    let Expr::StringLiteral(format) = expression.left.as_ref() else {
        return None;
    };
    let Expr::Tuple(arguments) = expression.right.as_ref() else {
        return None;
    };
    if arguments
        .elts
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
    {
        return None;
    }

    let format = format.value.chars().collect::<Vec<_>>();
    let mut position = 0;
    let mut argument_index = 0;
    let mut parts = Vec::with_capacity(arguments.elts.len() * 2 + 1);
    while position < format.len() {
        let mut literal = String::new();
        while position < format.len() {
            if format[position] != '%' {
                literal.push(format[position]);
                position += 1;
            } else if format.get(position + 1) == Some(&'%') {
                literal.push('%');
                position += 2;
            } else {
                break;
            }
        }
        if !literal.is_empty() {
            parts.push(PercentFormatPart::Literal(literal));
        }
        if position == format.len() {
            break;
        }

        let argument = arguments.elts.get(argument_index)?;
        argument_index += 1;
        position += 1;
        let (conversion, format_spec) = parse_percent_format(&format, &mut position)?;
        parts.push(PercentFormatPart::Formatted {
            expression: argument,
            conversion,
            format_spec,
        });
    }
    (argument_index == arguments.elts.len()).then_some(parts)
}

fn parse_percent_format(format: &[char], position: &mut usize) -> Option<(u32, Option<String>)> {
    const MAX_DIGITS: usize = 3;

    let mut next = || {
        let character = format.get(*position).copied()?;
        *position += 1;
        Some(character)
    };

    let mut left_justify = false;
    let mut character = loop {
        let character = next()?;
        match character {
            '-' => left_justify = true,
            '+' | ' ' | '#' | '0' => {}
            _ => break character,
        }
    };

    let mut width = None;
    if character.is_ascii_digit() {
        let mut value = 0_u32;
        let mut digits = 0;
        while character.is_ascii_digit() {
            value = value * 10 + character.to_digit(10).unwrap();
            character = next()?;
            digits += 1;
            if digits >= MAX_DIGITS {
                return None;
            }
        }
        width = Some(value);
    }

    let mut precision = None;
    if character == '.' {
        character = next()?;
        let mut value = 0_u32;
        if character.is_ascii_digit() {
            let mut digits = 0;
            while character.is_ascii_digit() {
                value = value * 10 + character.to_digit(10).unwrap();
                character = next()?;
                digits += 1;
                if digits >= MAX_DIGITS {
                    return None;
                }
            }
        }
        precision = Some(value);
    }

    let conversion = match character {
        's' => 1,
        'r' => 2,
        'a' => 3,
        _ => return None,
    };
    let mut format_spec = String::new();
    if !left_justify && width.is_some_and(|width| width > 0) {
        format_spec.push('>');
    }
    if let Some(width) = width {
        use std::fmt::Write;
        write!(format_spec, "{width}").unwrap();
    }
    if let Some(precision) = precision {
        use std::fmt::Write;
        write!(format_spec, ".{precision}").unwrap();
    }

    Some((conversion, (!format_spec.is_empty()).then_some(format_spec)))
}

fn fold_constant(expression: &Expr) -> Option<Constant> {
    if let Some(constant) = literal_constant(expression) {
        return Some(constant);
    }
    match expression {
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(Constant::Bool(true))
        }
        Expr::FString(fstring) => {
            let mut value = String::new();
            for part in &fstring.value {
                match part {
                    FStringPart::Literal(literal) => value.push_str(&literal.value),
                    FStringPart::FString(fstring) => {
                        for element in &fstring.elements {
                            let InterpolatedStringElement::Literal(literal) = element else {
                                return None;
                            };
                            value.push_str(&literal.value);
                        }
                    }
                }
            }
            Some(Constant::String(value))
        }
        Expr::Tuple(tuple) if tuple.ctx == ExprContext::Load => tuple
            .elts
            .iter()
            .map(fold_constant)
            .collect::<Option<Vec<_>>>()
            .map(Constant::Tuple),
        Expr::Subscript(subscript) if subscript.ctx == ExprContext::Load => {
            fold_literal_subscript(&subscript.value, &subscript.slice)
        }
        Expr::Slice(slice) => constant_slice(slice),
        Expr::UnaryOp(unary) => match unary.op {
            UnaryOp::UAdd => fold_constant(&unary.operand),
            UnaryOp::Not => literal_truthiness(&unary.operand).map(|value| Constant::Bool(!value)),
            UnaryOp::USub => match fold_constant(&unary.operand)? {
                Constant::Int(0) => Some(Constant::Int(0)),
                Constant::Int(value) => i64::try_from(value)
                    .ok()
                    .map(|value| Constant::SignedInt(-value)),
                Constant::BigInt { negative, digits } => Some(Constant::BigInt {
                    negative: !negative,
                    digits,
                }),
                Constant::Float(value) => Some(Constant::Float(-value)),
                Constant::Complex { real, imag } => Some(Constant::Complex {
                    real: -real,
                    imag: -imag,
                }),
                _ => None,
            },
            UnaryOp::Invert => match fold_constant(&unary.operand)? {
                Constant::Int(value) => {
                    let magnitude = u128::from(value) + 1;
                    if magnitude < i64::MIN.unsigned_abs().into() {
                        Some(Constant::SignedInt(-(magnitude as i64)))
                    } else if magnitude == i64::MIN.unsigned_abs().into() {
                        Some(Constant::SignedInt(i64::MIN))
                    } else {
                        Some(Constant::BigInt {
                            negative: true,
                            digits: big_integer_digits(&magnitude.to_string()),
                        })
                    }
                }
                Constant::SignedInt(value) => Some(Constant::SignedInt(!value)),
                _ => None,
            },
        },
        Expr::BinOp(binary) => {
            let left = fold_constant(&binary.left)?;
            let right = fold_constant(&binary.right)?;
            match (left, binary.op, right) {
                (Constant::Bool(left), Operator::Add, Constant::Bool(right)) => {
                    Some(Constant::Int(u64::from(left) + u64::from(right)))
                }
                (Constant::Int(left), Operator::Add, Constant::Int(right)) => {
                    left.checked_add(right).map(Constant::Int)
                }
                (
                    left @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                    Operator::Add,
                    right @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                ) => fold_positive_integer_add(&left, &right),
                (Constant::Int(left), Operator::Sub, Constant::Int(right)) => {
                    if let Some(value) = left.checked_sub(right) {
                        Some(Constant::Int(value))
                    } else {
                        let magnitude = right - left;
                        if magnitude <= i64::MAX.cast_unsigned() {
                            Some(Constant::SignedInt(-(magnitude.cast_signed())))
                        } else if magnitude == i64::MIN.unsigned_abs() {
                            Some(Constant::SignedInt(i64::MIN))
                        } else {
                            Some(Constant::BigInt {
                                negative: true,
                                digits: big_integer_digits(&magnitude.to_string()),
                            })
                        }
                    }
                }
                (Constant::Int(left), Operator::Mult, Constant::Int(right)) => {
                    left.checked_mul(right).map(Constant::Int)
                }
                (Constant::Int(left), Operator::Div, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Float(left as f64 / right as f64))
                }
                (Constant::Int(left), Operator::FloorDiv, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Int(left / right))
                }
                (Constant::Int(left), Operator::Mod, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Int(left % right))
                }
                (Constant::Int(left), Operator::Pow, Constant::Int(right)) => {
                    let right = u32::try_from(right).ok()?;
                    if let Some(value) = left.checked_pow(right) {
                        Some(Constant::Int(value))
                    } else {
                        u128::from(left)
                            .checked_pow(right)
                            .map(|value| Constant::BigInt {
                                negative: false,
                                digits: big_integer_digits(&value.to_string()),
                            })
                    }
                }
                (
                    Constant::Int(0) | Constant::Bool(false),
                    Operator::Pow,
                    Constant::BigInt {
                        negative: false, ..
                    },
                ) => Some(Constant::Int(0)),
                (Constant::Int(left), Operator::LShift, Constant::Int(right)) => {
                    u32::try_from(right)
                        .ok()
                        .and_then(|right| left.checked_shl(right))
                        .map(Constant::Int)
                }
                (Constant::Int(left), Operator::RShift, Constant::Int(right)) => {
                    u32::try_from(right)
                        .ok()
                        .and_then(|right| left.checked_shr(right))
                        .map(Constant::Int)
                }
                (
                    left @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                    operator @ (Operator::BitAnd | Operator::BitOr | Operator::BitXor),
                    right @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                ) => fold_positive_integer_bitwise(&left, operator, &right),
                (Constant::Float(left), Operator::Add, Constant::Float(right)) => {
                    Some(Constant::Float(left + right))
                }
                (Constant::Float(left), Operator::Sub, Constant::Float(right)) => {
                    Some(Constant::Float(left - right))
                }
                (Constant::Float(left), Operator::Mult, Constant::Float(right)) => {
                    Some(Constant::Float(left * right))
                }
                (Constant::Float(left), Operator::Div, Constant::Float(right)) if right != 0.0 => {
                    Some(Constant::Float(left / right))
                }
                (left, operator @ (Operator::Add | Operator::Sub | Operator::Div), right)
                    if matches!(left, Constant::Float(_))
                        && matches!(
                            right,
                            Constant::Bool(_) | Constant::Int(_) | Constant::SignedInt(_)
                        )
                        || matches!(right, Constant::Float(_))
                            && matches!(
                                left,
                                Constant::Bool(_) | Constant::Int(_) | Constant::SignedInt(_)
                            ) =>
                {
                    let to_float = |constant: Constant| match constant {
                        Constant::Bool(value) => Some(f64::from(value)),
                        Constant::Int(value) => Some(value as f64),
                        Constant::SignedInt(value) => Some(value as f64),
                        Constant::Float(value) => Some(value),
                        _ => None,
                    };
                    let left = to_float(left)?;
                    let right = to_float(right)?;
                    match operator {
                        Operator::Add => Some(Constant::Float(left + right)),
                        Operator::Sub => Some(Constant::Float(left - right)),
                        Operator::Div if right != 0.0 => Some(Constant::Float(left / right)),
                        Operator::Div => None,
                        _ => unreachable!(),
                    }
                }
                (Constant::Float(left), Operator::Pow, Constant::Float(right)) => {
                    let result = left.powf(right);
                    result.is_finite().then_some(Constant::Float(result))
                }
                (Constant::Float(left), Operator::FloorDiv, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Float((left / right as f64).floor()))
                }
                (Constant::Bool(left), Operator::Pow, Constant::Bool(right)) => {
                    Some(Constant::Int(u64::from(left).pow(u32::from(right))))
                }
                (left, operator @ (Operator::Add | Operator::Sub | Operator::Mult), right)
                    if matches!(left, Constant::Complex { .. })
                        || matches!(right, Constant::Complex { .. }) =>
                {
                    let (left_real, left_imag) = constant_complex_parts(&left)?;
                    let (right_real, right_imag) = constant_complex_parts(&right)?;
                    let (real, imag) = match operator {
                        Operator::Add => (left_real + right_real, left_imag + right_imag),
                        Operator::Sub => (left_real - right_real, left_imag - right_imag),
                        Operator::Mult => (
                            left_real * right_real - left_imag * right_imag,
                            left_real * right_imag + left_imag * right_real,
                        ),
                        _ => unreachable!(),
                    };
                    Some(Constant::Complex { real, imag })
                }
                (Constant::String(mut left), Operator::Add, Constant::String(right)) => {
                    left.push_str(&right);
                    Some(Constant::String(left))
                }
                (Constant::Bytes(mut left), Operator::Add, Constant::Bytes(right)) => {
                    left.extend(right);
                    Some(Constant::Bytes(left))
                }
                (Constant::String(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::String(value)) => {
                    let length = value.chars().count();
                    if length == 0 {
                        Some(Constant::String(String::new()))
                    } else {
                        let times = usize::try_from(times).ok()?;
                        (times <= 4096 / length).then(|| Constant::String(value.repeat(times)))
                    }
                }
                (Constant::Bytes(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::Bytes(value)) => {
                    if value.is_empty() {
                        Some(Constant::Bytes(Vec::new()))
                    } else {
                        let times = usize::try_from(times).ok()?;
                        (times <= 4096 / value.len()).then(|| Constant::Bytes(value.repeat(times)))
                    }
                }
                (Constant::Tuple(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::Tuple(value)) => {
                    if value.is_empty() {
                        Some(Constant::Tuple(Vec::new()))
                    } else {
                        let times = usize::try_from(times).ok()?;
                        (times <= 256 / value.len()).then(|| {
                            Constant::Tuple(
                                (0..times).flat_map(|_| value.iter().cloned()).collect(),
                            )
                        })
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold_positive_integer_bitwise(
    left: &Constant,
    operator: Operator,
    right: &Constant,
) -> Option<Constant> {
    let left = positive_integer_digits(left)?;
    let right = positive_integer_digits(right)?;
    let length = match operator {
        Operator::BitAnd => left.len().min(right.len()),
        Operator::BitOr | Operator::BitXor => left.len().max(right.len()),
        _ => unreachable!(),
    };
    let mut digits = Vec::with_capacity(length);
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        digits.push(match operator {
            Operator::BitAnd => left & right,
            Operator::BitOr => left | right,
            Operator::BitXor => left ^ right,
            _ => unreachable!(),
        });
    }
    Some(positive_integer_constant(digits))
}

fn fold_positive_integer_add(left: &Constant, right: &Constant) -> Option<Constant> {
    let left = positive_integer_digits(left)?;
    let right = positive_integer_digits(right)?;
    let mut digits = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0_u32;
    for index in 0..left.len().max(right.len()) {
        let value = u32::from(left.get(index).copied().unwrap_or(0))
            + u32::from(right.get(index).copied().unwrap_or(0))
            + carry;
        digits.push((value & 0x7fff) as u16);
        carry = value >> 15;
    }
    if carry != 0 {
        digits.push(carry as u16);
    }
    Some(positive_integer_constant(digits))
}

fn positive_integer_digits(value: &Constant) -> Option<Vec<u16>> {
    match value {
        Constant::Int(value) => {
            let mut value = *value;
            let mut digits = Vec::new();
            while value != 0 {
                digits.push((value & 0x7fff) as u16);
                value >>= 15;
            }
            Some(digits)
        }
        Constant::BigInt {
            negative: false,
            digits,
        } => Some(normalized_digits(digits.clone())),
        _ => None,
    }
}

fn positive_integer_constant(digits: Vec<u16>) -> Constant {
    let digits = normalized_digits(digits);
    if digits.len() <= 5 {
        let value = digits
            .iter()
            .enumerate()
            .fold(0_u128, |value, (index, digit)| {
                value | (u128::from(*digit) << (index * 15))
            });
        if let Ok(value) = u64::try_from(value) {
            return Constant::Int(value);
        }
    }
    Constant::BigInt {
        negative: false,
        digits,
    }
}

fn fold_literal_subscript(value: &Expr, index: &Expr) -> Option<Constant> {
    let value = fold_constant(value)?;
    if !matches!(index, Expr::Slice(_)) {
        let index = match fold_constant(index)? {
            Constant::Bool(value) => i128::from(u8::from(value)),
            Constant::Int(value) => i128::try_from(value).ok()?,
            Constant::SignedInt(value) => i128::from(value),
            _ => return None,
        };
        let length = match &value {
            Constant::String(value) => value.chars().count(),
            Constant::Bytes(value) => value.len(),
            Constant::Tuple(value) => value.len(),
            _ => return None,
        };
        let index = if index < 0 {
            i128::try_from(length).ok()?.checked_add(index)?
        } else {
            index
        };
        let index = usize::try_from(index)
            .ok()
            .filter(|index| *index < length)?;
        return match value {
            Constant::String(value) => value
                .chars()
                .nth(index)
                .map(|character| Constant::String(character.to_string())),
            Constant::Bytes(value) => Some(Constant::Int(u64::from(value[index]))),
            Constant::Tuple(value) => Some(value[index].clone()),
            _ => None,
        };
    }

    let Expr::Slice(slice) = index else {
        unreachable!();
    };
    if slice.step.is_some() && constant_slice(slice).is_none() {
        return None;
    }

    fn integer(expression: &Expr) -> Option<usize> {
        match fold_constant(expression)? {
            Constant::Bool(value) => Some(usize::from(value)),
            Constant::Int(value) => usize::try_from(value).ok(),
            Constant::SignedInt(value) if value >= 0 => usize::try_from(value).ok(),
            Constant::SignedInt(_) => None,
            Constant::BigInt {
                negative: false, ..
            } => Some(usize::MAX),
            Constant::BigInt { negative: true, .. } => None,
            _ => None,
        }
    }

    fn bound(expression: Option<&Expr>, length: usize, default: usize) -> Option<usize> {
        let Some(expression) = expression else {
            return Some(default);
        };
        integer(expression).map(|value| value.min(length))
    }

    fn indices(slice: &ruff_python_ast::ExprSlice, length: usize) -> Option<Vec<usize>> {
        let lower = bound(slice.lower.as_deref(), length, 0)?;
        let upper = bound(slice.upper.as_deref(), length, length)?;
        let step = slice.step.as_deref().map_or(Some(1), integer)?;
        if step == 0 {
            return None;
        }
        Some((lower.min(upper)..upper).step_by(step).collect())
    }

    match value {
        Constant::String(value) => {
            let characters = value.chars().collect::<Vec<_>>();
            Some(Constant::String(
                indices(slice, characters.len())?
                    .into_iter()
                    .map(|index| characters[index])
                    .collect(),
            ))
        }
        Constant::Bytes(value) => Some(Constant::Bytes(
            indices(slice, value.len())?
                .into_iter()
                .map(|index| value[index])
                .collect(),
        )),
        Constant::Tuple(value) => Some(Constant::Tuple(
            indices(slice, value.len())?
                .into_iter()
                .map(|index| value[index].clone())
                .collect(),
        )),
        _ => None,
    }
}

fn folded_bool_operand(expression: &Expr) -> Option<(&Expr, Constant)> {
    let Expr::BoolOp(boolean) = expression else {
        return fold_constant(expression).map(|constant| (expression, constant));
    };
    let (last, leading) = boolean.values.split_last()?;
    for value in leading {
        if matches!(value, Expr::BoolOp(_)) {
            return None;
        }
        let constant = fold_constant(value)?;
        let truthiness = constant_truthiness(&constant)?;
        if matches!(boolean.op, BoolOp::And) && !truthiness
            || matches!(boolean.op, BoolOp::Or) && truthiness
        {
            return Some((value, constant));
        }
    }
    if matches!(last, Expr::BoolOp(_)) {
        None
    } else {
        fold_constant(last).map(|constant| (last, constant))
    }
}

fn constant_complex_parts(constant: &Constant) -> Option<(f64, f64)> {
    match constant {
        Constant::Int(value) => Some((*value as f64, 0.0)),
        Constant::SignedInt(value) => Some((*value as f64, 0.0)),
        Constant::Float(value) => Some((*value, 0.0)),
        Constant::Complex { real, imag } => Some((*real, *imag)),
        _ => None,
    }
}

fn constant_truthiness(constant: &Constant) -> Option<bool> {
    match constant {
        Constant::None => Some(false),
        Constant::Bool(value) => Some(*value),
        Constant::Ellipsis | Constant::Slice { .. } | Constant::Code(_) => Some(true),
        Constant::Int(value) => Some(*value != 0),
        Constant::SignedInt(value) => Some(*value != 0),
        Constant::BigInt { digits, .. } => Some(!digits.is_empty()),
        Constant::Float(value) => Some(*value != 0.0),
        Constant::Complex { real, imag } => Some(*real != 0.0 || *imag != 0.0),
        Constant::String(value) => Some(!value.is_empty()),
        Constant::SurrogateString(value) => Some(!value.is_empty()),
        Constant::Bytes(value) => Some(!value.is_empty()),
        Constant::Tuple(value) | Constant::FrozenSet(value) => Some(!value.is_empty()),
    }
}

fn constant_slice(slice: &ruff_python_ast::ExprSlice) -> Option<Constant> {
    fn bound(expression: Option<&Expr>) -> Option<Constant> {
        expression.map_or(Some(Constant::None), literal_constant)
    }

    Some(Constant::Slice {
        lower: Box::new(bound(slice.lower.as_deref())?),
        upper: Box::new(bound(slice.upper.as_deref())?),
        step: Box::new(bound(slice.step.as_deref())?),
    })
}

fn two_element_slice_optimization(slice: &ruff_python_ast::ExprSlice) -> bool {
    slice.step.is_none() && constant_slice(slice).is_none()
}

fn first_literal_constant(expression: &Expr) -> Option<Constant> {
    if let Some(constant) = literal_constant(expression) {
        return Some(constant);
    }
    match expression {
        Expr::BinOp(binary) => {
            first_literal_constant(&binary.left).or_else(|| first_literal_constant(&binary.right))
        }
        Expr::UnaryOp(unary) => first_literal_constant(&unary.operand),
        Expr::Tuple(tuple) => tuple.elts.iter().find_map(first_literal_constant),
        _ => None,
    }
}

fn first_suite_literal_constant(body: &[Stmt]) -> Option<Constant> {
    #[derive(Default)]
    struct Collector {
        constant: Option<Constant>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if self.constant.is_some()
                || matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_))
            {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if self.constant.is_some() || matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if let Some(constant) = literal_constant(expression) {
                self.constant = Some(constant);
            } else {
                walk_expr(self, expression);
            }
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
        if collector.constant.is_some() {
            break;
        }
    }
    collector.constant
}

fn function_has_annotations(definition: &StmtFunctionDef) -> bool {
    let parameters = &definition.parameters;
    definition.returns.is_some()
        || parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .chain(&parameters.kwonlyargs)
            .any(|parameter| parameter.parameter.annotation.is_some())
        || parameters
            .vararg
            .as_ref()
            .is_some_and(|parameter| parameter.annotation.is_some())
        || parameters
            .kwarg
            .as_ref()
            .is_some_and(|parameter| parameter.annotation.is_some())
}

fn is_literal_constant(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::NoneLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
    )
}

fn statement_terminates(statement: &Stmt) -> bool {
    match statement {
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Raise(_) => true,
        Stmt::While(statement) => {
            early_condition_truthiness(&statement.test) == Some(true)
                && !suite_contains_loop_break(&statement.body)
        }
        Stmt::For(statement) => {
            !suite_contains_loop_break(&statement.body) && suite_terminates(&statement.orelse)
        }
        Stmt::If(statement) => {
            suite_terminates(&statement.body)
                && !statement.elif_else_clauses.is_empty()
                && statement
                    .elif_else_clauses
                    .iter()
                    .all(|clause| suite_terminates(&clause.body))
                && statement
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none())
        }
        Stmt::Try(statement) => suite_terminates(&statement.finalbody),
        _ => false,
    }
}

fn terminating_if_fallthrough_test(statement: &ruff_python_ast::StmtIf) -> Option<&Expr> {
    if !suite_terminates(&statement.body) {
        return None;
    }
    let mut final_test = statement.test.as_ref();
    for clause in &statement.elif_else_clauses {
        let test = clause.test.as_ref()?;
        if !suite_terminates(&clause.body) {
            return None;
        }
        final_test = test;
    }
    Some(final_test)
}

fn terminal_exception_handler_if_supported(statement: &ruff_python_ast::StmtIf) -> bool {
    std::iter::once(statement.body.as_slice())
        .chain(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| clause.body.as_slice()),
        )
        .all(|body| {
            !suite_terminates(body)
                || matches!(body, [Stmt::Return(statement)] if statement.value.is_none())
        })
}

fn statement_uses_implicit_return_location(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Assign(assignment) if assignment.targets.len() > 1 || matches!(assignment.targets.last(), Some(Expr::List(_) | Expr::Tuple(_))))
        || matches!(statement, Stmt::Assign(assignment) if matches!(assignment.value.as_ref(), Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)))
        || matches!(statement, Stmt::Expr(expression) if fold_constant(&expression.value).is_some())
        || matches!(
            statement,
            Stmt::AnnAssign(assignment)
                if assignment.value.is_none()
                    && matches!(assignment.target.as_ref(), Expr::Subscript(_))
        )
        || matches!(statement, Stmt::While(statement) if statement.orelse.is_empty())
        || matches!(
            statement,
            Stmt::For(statement)
                if statement.orelse.is_empty()
                    && !suite_contains_loop_break(&statement.body)
        )
}

fn expression_defers_async_comprehension_restore(expression: &Expr) -> bool {
    match expression {
        Expr::ListComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        Expr::SetComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        Expr::DictComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        _ => false,
    }
}

fn implicit_return_range(compiler: &Compiler, statement: &Stmt) -> ruff_text_size::TextRange {
    match statement {
        Stmt::Assign(assignment) => assignment
            .targets
            .last()
            .map_or(statement.range(), |target| {
                last_store_target_range(compiler, target)
            }),
        Stmt::Expr(expression) => match expression.value.as_ref() {
            Expr::FString(fstring) => compiler.fstring_result_range(fstring),
            expression => expression.range(),
        },
        Stmt::AnnAssign(assignment) => {
            let Expr::Subscript(subscript) = assignment.target.as_ref() else {
                return statement.range();
            };
            last_discarded_annotation_slice_range(&subscript.slice)
        }
        Stmt::While(statement) => statement.test.range(),
        Stmt::For(statement) => statement.iter.range(),
        _ => statement.range(),
    }
}

fn last_store_target_range(compiler: &Compiler, expression: &Expr) -> ruff_text_size::TextRange {
    match expression {
        Expr::List(list) => list.elts.last().map_or(expression.range(), |target| {
            last_store_target_range(compiler, target)
        }),
        Expr::Tuple(tuple) => tuple.elts.last().map_or(expression.range(), |target| {
            last_store_target_range(compiler, target)
        }),
        Expr::Starred(starred) => last_store_target_range(compiler, &starred.value),
        Expr::Attribute(attribute) => compiler.attribute_opcode_range(attribute),
        _ => expression.range(),
    }
}

fn statement_execution_range(statement: &Stmt) -> ruff_text_size::TextRange {
    match statement {
        Stmt::Assign(statement) => first_expression_instruction_range(&statement.value),
        Stmt::AnnAssign(statement) => statement
            .value
            .as_deref()
            .map_or(statement.range, first_expression_instruction_range),
        Stmt::AugAssign(statement) => first_expression_instruction_range(&statement.target),
        Stmt::Expr(statement) => first_expression_instruction_range(&statement.value),
        Stmt::Return(statement) => statement
            .value
            .as_deref()
            .map_or(statement.range, first_expression_instruction_range),
        _ => statement.range(),
    }
}

fn first_expression_instruction_range(expression: &Expr) -> ruff_text_size::TextRange {
    if matches!(
        expression,
        Expr::BinOp(_) | Expr::Name(_) | Expr::UnaryOp(_) | Expr::Tuple(_)
    ) && fold_constant(expression).is_some()
    {
        return expression.range();
    }
    match expression {
        Expr::Attribute(attribute) => first_expression_instruction_range(&attribute.value),
        Expr::BinOp(binary) => first_expression_instruction_range(&binary.left),
        Expr::BoolOp(boolean) => boolean
            .values
            .first()
            .map_or(expression.range(), first_expression_instruction_range),
        Expr::Call(call) => first_expression_instruction_range(&call.func),
        Expr::Compare(compare) => first_expression_instruction_range(&compare.left),
        Expr::If(conditional) => first_expression_instruction_range(&conditional.test),
        Expr::Named(named) => first_expression_instruction_range(&named.value),
        Expr::Subscript(subscript) => first_expression_instruction_range(&subscript.value),
        Expr::UnaryOp(unary) => first_expression_instruction_range(&unary.operand),
        _ => expression.range(),
    }
}

fn last_discarded_annotation_slice_range(expression: &Expr) -> ruff_text_size::TextRange {
    match expression {
        Expr::Tuple(tuple) => tuple
            .elts
            .last()
            .map_or(expression.range(), last_discarded_annotation_slice_range),
        Expr::Slice(slice) => slice
            .step
            .as_deref()
            .or(slice.upper.as_deref())
            .or(slice.lower.as_deref())
            .map_or(expression.range(), last_discarded_annotation_slice_range),
        _ => expression.range(),
    }
}

fn literal_truthiness(expression: &Expr) -> Option<bool> {
    match expression {
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(true)
        }
        Expr::NoneLiteral(_) => Some(false),
        Expr::BooleanLiteral(value) => Some(value.value),
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(value) => Some(value.as_u64() != Some(0)),
            Number::Float(value) => Some(*value != 0.0),
            Number::Complex { real, imag } => Some(*real != 0.0 || *imag != 0.0),
        },
        Expr::StringLiteral(value) => Some(!value.value.is_empty()),
        Expr::BytesLiteral(value) => Some(!value.value.is_empty()),
        Expr::FString(_) => match fold_constant(expression)? {
            Constant::String(value) => Some(!value.is_empty()),
            _ => None,
        },
        Expr::Tuple(value) => Some(!value.elts.is_empty()),
        _ => None,
    }
}

fn jump_constant_truthiness(expression: &Expr) -> Option<bool> {
    match expression {
        Expr::EllipsisLiteral(_) => Some(true),
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(true)
        }
        Expr::NoneLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::FString(_) => literal_truthiness(expression),
        _ => None,
    }
}

fn early_condition_truthiness(expression: &Expr) -> Option<bool> {
    if matches!(expression, Expr::Tuple(_)) {
        None
    } else {
        literal_truthiness(expression)
    }
}

fn is_wildcard_pattern(pattern: &Pattern) -> bool {
    matches!(
        pattern,
        Pattern::MatchAs(pattern) if pattern.pattern.is_none() && pattern.name.is_none()
    )
}

fn suite_contains_loop_break(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::Break(_) => true,
        Stmt::If(statement) => {
            suite_contains_loop_break(&statement.body)
                || statement
                    .elif_else_clauses
                    .iter()
                    .any(|clause| suite_contains_loop_break(&clause.body))
        }
        Stmt::Try(statement) => {
            suite_contains_loop_break(&statement.body)
                || statement
                    .handlers
                    .iter()
                    .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                    .any(|handler| suite_contains_loop_break(&handler.body))
                || suite_contains_loop_break(&statement.orelse)
                || suite_contains_loop_break(&statement.finalbody)
        }
        Stmt::With(statement) => suite_contains_loop_break(&statement.body),
        Stmt::Match(statement) => statement
            .cases
            .iter()
            .any(|case| suite_contains_loop_break(&case.body)),
        Stmt::For(statement) => suite_contains_loop_break(&statement.orelse),
        Stmt::While(statement) => suite_contains_loop_break(&statement.orelse),
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => false,
        _ => false,
    })
}

fn big_integer_digits(token: &str) -> Vec<u16> {
    let token: String = token
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (digits, radix) = if let Some(digits) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        (digits, 16)
    } else if let Some(digits) = token
        .strip_prefix("0o")
        .or_else(|| token.strip_prefix("0O"))
    {
        (digits, 8)
    } else if let Some(digits) = token
        .strip_prefix("0b")
        .or_else(|| token.strip_prefix("0B"))
    {
        (digits, 2)
    } else {
        (token.as_str(), 10)
    };

    let mut value = vec![0_u16];
    for digit in digits.chars().filter_map(|digit| digit.to_digit(radix)) {
        let mut carry = digit;
        for limb in &mut value {
            let next = u32::from(*limb) * radix + carry;
            *limb = (next & 0x7fff) as u16;
            carry = next >> 15;
        }
        while carry != 0 {
            value.push((carry & 0x7fff) as u16);
            carry >>= 15;
        }
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    value
}

fn binary_operator(operator: Operator, inplace: bool) -> u32 {
    let base = match operator {
        Operator::Add => 0,
        Operator::BitAnd => 1,
        Operator::FloorDiv => 2,
        Operator::LShift => 3,
        Operator::MatMult => 4,
        Operator::Mult => 5,
        Operator::Mod => 6,
        Operator::BitOr => 7,
        Operator::Pow => 8,
        Operator::RShift => 9,
        Operator::Sub => 10,
        Operator::Div => 11,
        Operator::BitXor => 12,
    };
    if inplace { base + 13 } else { base }
}

fn comparison_operator(operator: CmpOp) -> (Opcode, u32) {
    match operator {
        CmpOp::Lt => (COMPARE_OP, 2),
        CmpOp::LtE => (COMPARE_OP, 42),
        CmpOp::Eq => (COMPARE_OP, 72),
        CmpOp::NotEq => (COMPARE_OP, 103),
        CmpOp::Gt => (COMPARE_OP, 132),
        CmpOp::GtE => (COMPARE_OP, 172),
        CmpOp::Is => (IS_OP, 0),
        CmpOp::IsNot => (IS_OP, 1),
        CmpOp::In => (CONTAINS_OP, 0),
        CmpOp::NotIn => (CONTAINS_OP, 1),
    }
}

fn comparison_operator_boolean(operator: CmpOp) -> (Opcode, u32) {
    let (opcode, argument) = comparison_operator(operator);
    if matches!(
        operator,
        CmpOp::Lt | CmpOp::LtE | CmpOp::Eq | CmpOp::NotEq | CmpOp::Gt | CmpOp::GtE
    ) {
        (opcode, argument | 16)
    } else {
        (opcode, argument)
    }
}

fn to_u32(value: usize, what: &str) -> Result<u32, CompileError> {
    value
        .try_into()
        .map_err(|_| CompileError::Unsupported(format!("{what} exceeds CPython's u32 limit")))
}

fn unsupported(feature: &str) -> CompileError {
    CompileError::Unsupported(format!("Python 3.14 {feature} is not implemented yet"))
}

fn statement_name(statement: &Stmt) -> &'static str {
    match statement {
        Stmt::AnnAssign(_) => "annotated assignment",
        Stmt::Assert(_) => "assert statement",
        Stmt::ClassDef(_) => "class definition",
        Stmt::Delete(_) => "del statement",
        Stmt::For(_) => "for statement",
        Stmt::Global(_) => "global statement",
        Stmt::Import(_) => "import statement",
        Stmt::ImportFrom(_) => "from import statement",
        Stmt::IpyEscapeCommand(_) => "IPython escape command",
        Stmt::Match(_) => "match statement",
        Stmt::Nonlocal(_) => "nonlocal statement",
        Stmt::Raise(_) => "raise statement",
        Stmt::Try(_) => "try statement",
        Stmt::TypeAlias(_) => "type alias statement",
        Stmt::With(_) => "with statement",
        _ => "statement",
    }
}

fn expression_name(expression: &Expr) -> &'static str {
    match expression {
        Expr::Await(_) => "await expression",
        Expr::DictComp(_) => "dictionary comprehension",
        Expr::FString(_) => "f-string",
        Expr::Generator(_) => "generator expression",
        Expr::IpyEscapeCommand(_) => "IPython escape command",
        Expr::Lambda(_) => "lambda expression",
        Expr::ListComp(_) => "list comprehension",
        Expr::Named(_) => "named expression",
        Expr::SetComp(_) => "set comprehension",
        Expr::Slice(_) => "slice expression",
        Expr::Starred(_) => "starred expression",
        Expr::TString(_) => "template string",
        Expr::Yield(_) => "yield expression",
        Expr::YieldFrom(_) => "yield from expression",
        Expr::Attribute(_) => "attribute store or delete",
        Expr::List(_) => "list store or delete",
        Expr::Name(_) => "name store or delete",
        Expr::Subscript(_) => "subscript store or delete",
        Expr::Tuple(_) => "tuple store or delete",
        _ => "expression",
    }
}

fn unparse_annotation(expression: &Expr) -> String {
    if let Expr::StringLiteral(string) = expression {
        return python_string_repr(string.value.to_str());
    }
    let indentation = Indentation::default();
    let mut annotation = Generator::new(&indentation, LineEnding::Lf)
        .with_mode(CodegenMode::AstUnparse)
        .expr(expression);
    let bytes = annotation.as_bytes();
    let mut stack = Vec::<(u8, usize)>::new();
    let mut parenthesis_pairs = HashSet::new();
    let mut bracket_pairs = Vec::new();
    let mut quote = None::<(u8, bool)>;
    let mut index = 0_usize;
    while index < bytes.len() {
        if let Some((delimiter, triple)) = quote {
            if bytes[index] == b'\\' {
                index = (index + 2).min(bytes.len());
                continue;
            }
            if bytes[index] == delimiter
                && (!triple
                    || bytes.get(index + 1) == Some(&delimiter)
                        && bytes.get(index + 2) == Some(&delimiter))
            {
                index += if triple { 3 } else { 1 };
                quote = None;
                continue;
            }
            index += 1;
            continue;
        }
        match bytes[index] {
            delimiter @ (b'\'' | b'"') => {
                let triple = bytes.get(index + 1) == Some(&delimiter)
                    && bytes.get(index + 2) == Some(&delimiter);
                quote = Some((delimiter, triple));
                index += if triple { 3 } else { 1 };
                continue;
            }
            delimiter @ (b'(' | b'[' | b'{') => stack.push((delimiter, index)),
            b')' | b']' | b'}' => {
                let closing = bytes[index];
                let expected = match closing {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if let Some((opening, opening_index)) = stack.pop()
                    && opening == expected
                {
                    if opening == b'(' {
                        parenthesis_pairs.insert((opening_index, index));
                    } else if opening == b'[' {
                        bracket_pairs.push((opening_index, index));
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    let mut removals = bracket_pairs
        .into_iter()
        .filter_map(|(opening, closing)| {
            let is_subscript = annotation.as_bytes()[..opening]
                .iter()
                .rev()
                .find(|byte| !byte.is_ascii_whitespace())
                .is_some_and(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b')' | b']' | b'}' | b'\'' | b'"')
                });
            (is_subscript
                && closing.saturating_sub(opening) > 3
                && parenthesis_pairs.contains(&(opening + 1, closing.saturating_sub(1))))
            .then_some([opening + 1, closing - 1])
        })
        .flatten()
        .collect::<Vec<_>>();
    removals.sort_unstable_by(|left, right| right.cmp(left));
    for index in removals {
        annotation.remove(index);
    }
    annotation
}

fn python_string_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut repr = String::with_capacity(value.len() + 2);
    repr.push(quote);
    for character in value.chars() {
        match character {
            '\\' => repr.push_str("\\\\"),
            '\n' => repr.push_str("\\n"),
            '\r' => repr.push_str("\\r"),
            '\t' => repr.push_str("\\t"),
            '\x08' => repr.push_str("\\b"),
            '\x0c' => repr.push_str("\\f"),
            character if character == quote => {
                repr.push('\\');
                repr.push(character);
            }
            character if character.is_control() => {
                use std::fmt::Write;
                write!(repr, "\\x{:02x}", u32::from(character)).unwrap();
            }
            character => repr.push(character),
        }
    }
    repr.push(quote);
    repr
}

fn clean_doc(docstring: &str) -> String {
    let mut expanded = String::with_capacity(docstring.len());
    let mut column = 0_usize;
    for character in docstring.chars() {
        match character {
            '\t' => {
                let spaces = 8 - column % 8;
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                expanded.push(character);
                column = 0;
            }
            _ => {
                expanded.push(character);
                column += 1;
            }
        }
    }

    let lines: Vec<_> = expanded.split_inclusive('\n').collect();
    let margin = lines
        .iter()
        .skip(1)
        .filter_map(|line| {
            let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
            line.as_bytes()
                .get(indentation)
                .is_some_and(|byte| *byte != b'\n')
                .then_some(indentation)
        })
        .min()
        .unwrap_or(0);

    let Some((first, remaining)) = lines.split_first() else {
        return expanded;
    };
    let mut cleaned = String::with_capacity(expanded.len());
    cleaned.push_str(first.trim_start_matches(' '));
    for line in remaining {
        let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
        cleaned.push_str(&line[indentation.min(margin)..]);
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::parse_module;

    use super::{Compiler, clean_doc};

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
    fn cleans_docstrings_like_cpython() {
        assert_eq!(
            clean_doc("  first\n        second\n      third\n"),
            "first\n  second\nthird\n"
        );
        assert_eq!(clean_doc("\tfirst\n\t\tsecond"), "first\nsecond");
    }
}
