use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_pattern, walk_stmt};
use ruff_python_ast::{
    BoolOp, CmpOp, ConversionFlag, Expr, ExprBinOp, ExprContext, FStringPart, Identifier,
    InterpolatedStringElement, Keyword, Number, Operator, Pattern, Singleton, Stmt, StmtClassDef,
    StmtFunctionDef, Suite, TypeParam, UnaryOp,
};
use ruff_python_codegen::{Generator, Indentation, Mode as CodegenMode};
use ruff_source_file::{LineEnding, LineIndex};
use ruff_text_size::{Ranged, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::CompileError;
use crate::assembler::{AssembledCode, Assembler, InstructionId, Label, SourceLocation};
use crate::target::code_flags::{
    CO_ASYNC_GENERATOR, CO_COROUTINE, CO_FUTURE_ANNOTATIONS, CO_FUTURE_BARRY_AS_BDFL, CO_GENERATOR,
    CO_HAS_DOCSTRING, CO_METHOD, CO_NESTED, CO_NEWLOCALS, CO_OPTIMIZED, CO_VARARGS, CO_VARKEYWORDS,
};
use crate::target::local_kinds::{
    CO_FAST_ARG_KW, CO_FAST_ARG_POS, CO_FAST_ARG_VAR, CO_FAST_CELL, CO_FAST_FREE, CO_FAST_HIDDEN,
    CO_FAST_LOCAL,
};
// The generated opcode module is the compiler lowering vocabulary; spelling out this nearly
// complete import would obscure which imports carry higher-level meaning.
#[allow(clippy::wildcard_imports)]
use crate::target::opcodes::*;
use crate::target::operands::{
    BinaryIntrinsic, BinaryOperation, CommonConstant, ComparisonOperation, Conversion,
    FunctionAttribute, ResumeLocation, SpecialMethod, UnaryIntrinsic,
};
use crate::target::{Opcode, is_unconditional_jump};

mod constant;
mod definition;
mod expression;
mod orchestration;
mod scope;
mod source;
mod statement;
mod symbol;
#[cfg(test)]
mod tests;

// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#[allow(clippy::wildcard_imports)]
use self::{
    constant::*, definition::*, expression::*, scope::*, source::*, statement::*, symbol::*,
};

// CPython initially emits owned local loads, then strength-reduces safe loads
// to borrowed references after the control-flow graph has reached its final
// shape. The assembler mirrors that dataflow pass.
const SUPPORTED_FUTURE_FLAGS: u32 = CO_FUTURE_BARRY_AS_BDFL | CO_FUTURE_ANNOTATIONS;
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
        globals: NameSet,
        nonlocals: NameSet,
        free_indices: FxHashMap<NameId, u32>,
        bound_names: NameSet,
    },
    Function {
        indices: FxHashMap<NameId, u32>,
        free_indices: FxHashMap<NameId, u32>,
        cells: NameSet,
        globals: NameSet,
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

#[derive(Debug)]
struct ExceptionHandlerContext {
    name: Option<String>,
    loop_depth: usize,
}

#[derive(Debug)]
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

type RegionExclusions = Vec<(Label, Label)>;
type RegionExclusionStack = Vec<RegionExclusions>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IteratorCleanup {
    None,
    Sync,
    Async,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionKind {
    List,
    Tuple,
    Set,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefinitionDecorators {
    Compile,
    AlreadyCompiled,
}

#[derive(Debug)]
struct CompilationContext {
    filename: String,
    source: Arc<str>,
    line_index: LineIndex,
    scopes: OnceLock<ScopeArena>,
}

#[derive(Debug)]
struct CodeUnit {
    scope: Scope,
    scope_id: ScopeId,
    generic_target_qualified_name: Option<String>,
    child_qualified_name_parent: Option<String>,
    class_scope_is_nested: bool,
    module_annotation_index: u32,
    annotation_classdict_index: Option<u32>,
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

#[derive(Debug)]
struct CodeUnitSpec {
    scope: Scope,
    scope_id: ScopeId,
    name: String,
    qualified_name: String,
    private_name: Option<String>,
    arg_count: u32,
    positional_only_arg_count: u32,
    keyword_only_arg_count: u32,
    flags: u32,
    first_line_number: u32,
}

#[derive(Debug, Default)]
struct OutputState {
    constants: Vec<Constant>,
    deferred_constants_before_return: Vec<(InstructionId, Constant)>,
    deferred_constants: Vec<(Option<InstructionId>, Constant)>,
    deferred_names: Vec<(InstructionId, String)>,
    names: Vec<String>,
    name_indices: FxHashMap<NameId, u32>,
    interned_constant_strings: HashSet<String>,
}

#[derive(Debug, Default)]
struct SymbolState {
    locals: Vec<String>,
    fast_local_count: usize,
    cell_names: NameSet,
    free_names: Vec<String>,
    temporary_indices: FxHashMap<NameId, u32>,
    hidden_names: NameSet,
    active_temporaries: NameSet,
    initialized_locals: NameSet,
    owned_load_locals: NameSet,
    imported_scope_names: NameSet,
    type_parameter_names: NameSet,
    module_globals: NameSet,
}

#[derive(Debug, Default)]
struct ControlFlowState {
    defer_async_comprehension_restore: bool,
    reorder_async_comprehension_cleanup_throw: bool,
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
    generator_region_start: Option<Label>,
    generator_region_exclusions: Vec<(Label, Label)>,
    emitted_fallthrough_return: bool,
    loops: Vec<LoopContext>,
    depth: i32,
    max_depth: u32,
}

#[derive(Debug)]
pub(crate) struct Compiler {
    context: Arc<CompilationContext>,
    unit: CodeUnit,
    output: OutputState,
    symbols: SymbolState,
    control: ControlFlowState,
    assembler: Assembler,
}
