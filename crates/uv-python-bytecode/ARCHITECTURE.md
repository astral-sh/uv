# uv-python-bytecode architecture

`uv-python-bytecode` compiles Python source to CPython code objects without embedding or starting a
Python runtime. Ruff supplies the parser and AST; the rest of the compiler, assembler, marshal
writer, and `.pyc` wrapper are implemented in Rust.

This document describes the boundaries that must remain stable for byte-for-byte compatibility.
It is intentionally specific to the current backend rather than a design for a generic Python
virtual machine.

## Compatibility contract

The only supported target is:

- implementation: `cpython`;
- release: `v3.14.5`;
- source commit: `5607950ef232dad16d75c0cf53101d9649d89115`;
- magic number: `2b0e0d0a`.

`CPYTHON_TARGET` in `src/lib.rs` exposes this identity from the generated target module. The target
includes more than the instruction set: code-object fields, flags, inline caches, line and
exception tables, marshal object identity, and object ordering are all part of the compatibility
surface.

The oracle comparison is:

```python
marshal.dumps(compile(source, filename, "exec", dont_inherit=True, optimize=0))
```

Matching executable behavior is therefore insufficient. The emitted marshal bytes must match the
oracle, including metadata and reference layout.

## Pipeline and ownership

The main path is:

```text
source
  -> normalization and Ruff parsing
  -> scope planning and AST lowering
  -> symbolic instruction stream and assembler passes
  -> CodeObject
  -> marshal object graph and writer
  -> optional timestamp-based .pyc header
```

Each stage has one primary owner:

| Concern                                                                      | Owner              |
| ---------------------------------------------------------------------------- | ------------------ |
| Public API, input normalization, parser setup, `.pyc` header                 | `src/lib.rs`       |
| Scope analysis, AST lowering, constants/names/locals, stack-depth accounting | `src/compiler.rs`  |
| Labels, control-flow rewrites, instruction layout, line and exception tables | `src/assembler/`   |
| CPython IDs, widths, flags, stack metadata, version and provenance           | `src/target/`      |
| Marshal equality/identity keys, reference graph, binary encoding             | `src/marshal/`     |

Code should stay on the side of this boundary that owns the relevant CPython behavior. In
particular, numeric target data does not belong in the compiler or assembler, and control-flow
rewrites do not belong in the AST lowering code.

### Parsing

`compile` in `src/lib.rs` normalizes CRLF and bare CR newlines, removes one leading UTF-8 BOM, and
parses a module with Ruff's `PY314` grammar. Ruff returns syntax and source ranges; it does not
provide CPython's symbol table, compiler, runtime, or marshal implementation.

### Scope planning and lowering

`src/compiler.rs` performs the CPython-specific analysis that lowering needs:

- `LocalCollector` and `ReferenceCollector` collect bindings, explicit globals/nonlocals, and
    references.
- `FunctionPlan` recursively resolves function locals, cells, free variables, annotation free
    variables, and nested-function requirements before a function body is lowered.
- Module, class, lambda, comprehension, generator, and annotation-scope helpers handle the
    additional name-resolution rules for those scopes.

`Compiler` then lowers the Ruff AST into symbolic `Instruction` values. At this point jumps still
refer to `Label`s rather than byte offsets. The compiler also owns constant and name tables, local
layout, exception-region declarations, source locations, and the logical operand-stack depth.
Nested functions, classes, comprehensions, and annotation thunks are recursively compiled into
`Constant::Code` values.

Before assembly, `Compiler::finish_inner`:

1. materializes deferred constants that must precede the implicit return;
1. emits an implicit return or generator exception handler when required;
1. verifies that the logical stack depth is zero;
1. resolves the remaining deferred constant and name indices;
1. performs the compiler-owned constant-pop cleanup and removes unused constants;
1. computes CPython local-kind bytes; and
1. passes the symbolic stream to `Assembler::finish_code`.

The resulting `CodeObject` is the complete handwritten representation consumed by the marshal
layer.

### Assembler

The assembler stores a linear sequence of labels and symbolic instructions plus exception-region
descriptions. An instruction carries its opcode, value or label operand, source location, optional
post-instruction stack depth, and provenance flags used by later passes.

`Assembler::finish_code` runs the following order. This is a compatibility contract, not a menu of
independent optimizations.

| Order | Pass                                                                       |
| ----: | -------------------------------------------------------------------------- |
|     1 | Remove initially unreachable instructions                                  |
|     2 | Optimize boolean conversions                                               |
|     3 | Thread forward jumps                                                       |
|     4 | Remove instructions made unreachable by threading                          |
|     5 | Remove redundant forward jumps, early pass                                 |
|     6 | Replace overwritten adjacent local stores                                  |
|     7 | Minimize runs of stack swaps                                               |
|     8 | Apply swaps statically to stores and pops                                  |
|     9 | Duplicate eligible exit blocks                                             |
|    10 | Convert potentially uninitialized local loads to checked loads             |
|    11 | Fuse adjacent local-load/store superinstructions                           |
|    12 | Move cold blocks to the end                                                |
|    13 | Remove repeated checked loads within basic blocks                          |
|    14 | Propagate source locations within blocks                                   |
|    15 | Remove redundant swaps before pops                                         |
|    16 | Remove redundant NOPs                                                      |
|    17 | Remove redundant forward jumps, late pass                                  |
|    18 | Convert safe owned local loads to borrowed loads                           |
|    19 | Resolve jump widths, then emit bytecode, line tables, and exception tables |

Several dependencies are explicit in the implementation:

- redundant-store and static-swap optimization must happen before superinstruction fusion;
- borrowed-load conversion runs after the control-flow graph has its final shape;
- instruction provenance flags must be deliberately preserved, combined, or cleared when a pass
    replaces an instruction; and
- source locations and exception ownership are observable output, even when an opcode becomes a
    `NOP` or is removed.

Final layout repeatedly computes label positions and jump arguments while accounting for inline
caches and `EXTENDED_ARG`. It stops when every instruction's width is stable, with an eight-pass
convergence limit. Only then can line and exception tables be encoded.

In tests and debug builds, stage validation checks:

- every label is bound at most once;
- every jump target and exception-region label is bound;
- forward and backward operands point in the declared direction;
- both reachability-removal stages leave no unreachable instruction; and
- exception-region starts precede their ends at final layout.

These checks diagnose structural corruption; the byte-for-byte oracle remains the semantic check.

### Marshal encoding and `.pyc` output

Marshal encoding is deliberately split into three responsibilities:

- `marshal/key.rs` defines when values compare equal and when CPython preserves object identity. It
    also defines deterministic sort keys for unordered constants such as frozen sets.
- `marshal/graph.rs` walks the complete code-object graph first. It counts repeated objects and
    records strings that CPython will intern.
- `marshal/writer.rs` uses that graph to assign reference indices and encode code objects,
    constants, strings, tuples, local-kind bytes, line tables, and exception tables in CPython's
    order.

The graph pass must precede writing because whether an object's first occurrence receives
`FLAG_REF` depends on later occurrences. Replacing the two-pass design with opportunistic
single-pass serialization changes bytes even when decoded values are equal.

`CompiledModule::to_timestamp_pyc` prefixes the marshalled code object with the generated magic
number, timestamp-invalidation flags, source modification time, and source size. The runtime is
needed only to execute this output or act as the comparison oracle.

## Generated target metadata

`src/target/mod.rs` is handwritten. It defines `Opcode` and keeps its constructor private so the
generated module is the sole numeric authority. Each opcode bundles its numeric ID with its inline
cache width.

`src/target/cpython_3_14_5.rs` is generated from the pinned CPython checkout. It contains:

- target tag, commit, version, implementation, and magic number;
- physical compiled-code opcodes and inline-cache widths;
- opcode metadata flags and control-flow predicates;
- typed operand values for intrinsics, function attributes, common constants, resume points,
  binary and comparison operations, conversions, and special-method loads;
- code flags and local-kind bits; and
- separate dynamic pop and push counts used by assembler dataflow.

`src/marshal/v5.rs` is generated separately from CPython's marshal sources. It contains only the
version and wire tags for marshal format 5; the generic graph, reference, ordering, and encoding
logic remains handwritten in `src/marshal`.

The generator reads these CPython files:

- `Include/opcode_ids.h`;
- `Include/opcode.h`;
- `Include/object.h`;
- `Include/ceval.h`;
- `Include/internal/pycore_opcode_metadata.h`;
- `Include/internal/pycore_opcode_utils.h`;
- `Include/internal/pycore_intrinsics.h`;
- `Include/internal/pycore_ceval.h`;
- `Include/cpython/code.h`;
- `Include/internal/pycore_code.h`;
- `Include/internal/pycore_magic_number.h`;
- `Include/patchlevel.h`;
- `Python/codegen.c`;
- `Python/flowgraph.c`;
- `Include/marshal.h`; and
- `Python/marshal.c`.

Run it from the repository root:

```console
python3 crates/uv-python-bytecode/scripts/generate_cpython_target.py \
  --cpython /path/to/cpython
python3 crates/uv-python-bytecode/scripts/generate_cpython_target.py \
  --cpython /path/to/cpython \
  --check
```

The checkout must be at the exact pinned commit. Generation verifies the commit, the release tag
when available, the expected version, opcode, operand, predicate, stack, and marshal-metadata
completeness, and cache references. Each output records a SHA-256 digest for its input sources and
is formatted before comparison. `--check` also rejects direct `Opcode::new` calls and duplicated
numeric opcode constants in the compiler and assembler.

Do not hand-edit the generated files. Internal compiler state that cannot appear in output should
be represented by typed Rust state or instruction flags, not by inventing a private bytecode
opcode.

## Correctness gate

Focused tests can use a CPython 3.14.5 executable through
`UV_PYTHON_BYTECODE_TEST_PYTHON`. The full gate compares the Ruff and ty corpora to CPython's
marshal output. From the repository root, with `RUFF` pointing to a Ruff checkout:

```console
UV_PYTHON_BYTECODE_TEST_PYTHON=/path/to/python3.14 \
  cargo test --locked -p uv-python-bytecode --all-targets

cargo run --locked -p uv-python-bytecode --example compare_cpython_corpus -- \
  --python /path/to/python3.14 \
  --expected-files 2451 \
  --require-all \
  "$RUFF/crates/ty_python_semantic/resources/corpus" \
  "$RUFF/crates/ruff_python_parser/resources/valid" \
  "$RUFF/crates/ruff_python_parser/resources/inline/ok" \
  "$RUFF/crates/ruff_linter/resources/test/fixtures" \
  "$RUFF/crates/ruff_python_formatter/resources/test/fixtures"
```

The comparator verifies the oracle's implementation, three-part version, and magic number before
running. It also pins the discovered file count. CPython-rejected fixtures are reported separately;
every CPython-accepted file must match exactly under `--require-all`.

The frozen baseline is 2,230 exact matches and 221 CPython-rejected files, with zero byte
mismatches, unsupported files, parse mismatches, compiler panics, non-UTF-8 files, or oracle
failures.

## Safe changes

For a behavior-preserving refactor:

1. Keep each change inside the ownership boundary above.
1. Preserve assembler pass order unless changing it is the purpose of the change.
1. Preserve instruction provenance, source locations, exception-region ownership, table ordering,
    and marshal identity rules; they are observable even when execution is unchanged.
1. Add a focused regression for any newly understood edge case.
1. Run formatting, the generated-metadata check, all-target tests against the exact oracle, and the
    full corpus gate.

For a new CPython target:

1. Select and record an exact release tag and commit.
1. Update the generator's pin, expected version, and output module; wire the new module through
    `src/target/mod.rs` and regenerate it.
1. Audit the matching CPython compiler, flow-graph optimizer, assembler, code-object, and marshal
    sources. Generated metadata covers data tables, not handwritten semantics.
1. Update Ruff's parser target, public target identity, documentation, tests, and oracle selection
    as required by the Python version.
1. Run the full gate and investigate every changed byte. Change the frozen corpus baseline only
    when the corpus or target intentionally changes.

Treat a green focused test as necessary but not sufficient. Any change to lowering, pass ordering,
layout, constants, names, locals, or marshal identity can be correct on small examples while still
changing bytes elsewhere in the corpus.
