# uv-python-bytecode

`uv-python-bytecode` is an experimental Rust compiler from Python source to CPython 3.14.5
bytecode. It uses Ruff's parser and does not embed or link a Python runtime. CPython 3.14.5 is only
needed to execute generated `.pyc` files and to serve as the compatibility oracle in tests.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the pipeline, ownership boundaries, assembler contracts,
generated-target workflow, and required correctness gate.

The backend currently implements:

- CPython 3.14.5 code objects, inline caches, extended arguments, jumps, source-location tables,
  marshal references and interning, and timestamp-invalidated `.pyc` files;
- literals, collections and unpacking, f-strings, names, attributes, subscripts, slices, unary and
  binary operations, comparisons, boolean operations, conditional expressions, calls, and named
  expressions;
- assignments, deletion, imports, assertions, raises, `if`, `while`, synchronous `for`, `break`,
  `continue`, and loop `else` suites;
- functions, full parameter layouts, defaults, decorators, lambdas, globals, nonlocals, nested
  closures, deferred function annotations, and future-style module annotations;
- classes with bases, metaclass keywords, decorators, methods, class namespace metadata, class
  dictionary cells, and static-attribute metadata.

## CPython corpus comparison

The `compare_cpython_corpus` example discovers every `.py` file under Ruff's linter, formatter,
parser, and ty semantic test resources. For each CPython-accepted file it compares
`marshal.dumps(compile(...))` directly with this crate's marshal output:

```console
cargo run -p uv-python-bytecode --example compare_cpython_corpus -- \
  --python /path/to/cpython-3.14.5 \
  --expected-files 2451 \
  --require-all
```

The comparator refuses any interpreter whose implementation, three-part version, or magic number
does not exactly match this backend. It also fails on a missing corpus root or an empty corpus.
Pass `--require-all` to return a failure status unless every CPython-accepted file matches exactly.
CPython-rejected fixtures are reported separately and are outside the bytecode-compatibility gate.
Additional roots can be supplied explicitly, and `--python`, `--limit`, and `--examples` control
the oracle executable and report size. `--expected-files` pins the discovered corpus size. Pass
`--dump-mismatches DIR` to save mismatch inputs and outputs for inspection.

The frozen compatibility gate is pinned to CPython 3.14.5 with magic number `2b0e0d0a` and 2,451
Ruff and ty files. Its
current result is:

```text
exact:               2,230
CPython rejected:      221
byte mismatch:           0
unsupported:             0
Ruff parse mismatch:     0
compiler panic:          0
non-UTF-8:               0
oracle failure:          0
```

This exact-compatibility result is specific to the pinned oracle version and frozen corpus; it is
not a claim about arbitrary CPython versions or Python inputs. A successful `--require-all` run is
the authoritative compatibility gate.

## Updating the CPython target

Opcode IDs, inline-cache widths, opcode flags and predicates, operand values, stack metadata, code
flags, local-kind bits, magic number, marshal format tags, and target provenance are generated from
CPython tag `v3.14.5` at commit
`5607950ef232dad16d75c0cf53101d9649d89115`. Regenerate the checked-in module from a checkout at
that exact commit, then verify that it is current:

```console
python3 scripts/generate_cpython_target.py --cpython /path/to/cpython
python3 scripts/generate_cpython_target.py --cpython /path/to/cpython --check
```

The generator rejects a checkout at any other commit and records hashes for every CPython source
file that contributes target or marshal-format metadata. Instruction metadata is emitted under
`src/target`; the independently versioned marshal protocol constants are emitted under
`src/marshal`.

The Ruff dependencies are pinned to a specific Git revision because Ruff's parser crates are not
published independently.
