# uv-python-bytecode

`uv-python-bytecode` is an experimental Rust compiler from Python source to CPython 3.14
bytecode. It uses Ruff's parser and does not embed or link a Python runtime. CPython 3.14 is only
needed to execute generated `.pyc` files and to serve as the compatibility oracle in tests.

The backend currently implements:

- CPython 3.14 code objects, inline caches, extended arguments, jumps, source-location tables,
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
cargo run -p uv-python-bytecode --example compare_cpython_corpus
```

Pass `--require-all` to return a failure status unless every CPython-accepted file matches exactly.
Additional roots can be supplied explicitly, and `--python`, `--limit`, and `--examples` control
the oracle executable and report size. Pass `--dump-mismatches DIR` to save mismatch inputs and
outputs for inspection.

The frozen compatibility gate is pinned to CPython 3.14.0rc1 and 2,451 Ruff and ty files. Its
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

The Ruff dependencies are pinned to a specific Git revision because Ruff's parser crates are not
published independently.
