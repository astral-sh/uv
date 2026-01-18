# uv-mdtest Architecture

This document describes the internal architecture of the `uv-mdtest` crate, a markdown-based test
framework for uv CLI commands.

## Overview

The mdtest framework allows writing integration tests in markdown files. Tests are discovered by
`datatest-stable`, parsed into structured test definitions, and executed against the uv CLI with
output comparison.

```
┌─────────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Markdown File  │────▶│    Parser    │────▶│    Runner    │────▶│   Snapshot   │
│   (*.md)        │     │  (parser.rs) │     │  (runner.rs) │     │ (snapshot.rs)│
└─────────────────┘     └──────────────┘     └──────────────┘     └──────────────┘
                               │                    │                     │
                               ▼                    ▼                     ▼
                        ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
                        │MarkdownTest  │     │  TestResult  │     │   Update     │
                        │ TestConfig   │     │   Mismatch   │     │   Compare    │
                        └──────────────┘     └──────────────┘     └──────────────┘
```

## Module Structure

### `types.rs` - Data Structures

Defines the core data structures that represent parsed test files:

- **`MarkdownTestFile`**: A complete test file containing multiple tests
- **`MarkdownTest`**: A single test extracted from a markdown section
- **`EmbeddedFile`**: A file to create before running commands (from code blocks with `title="..."`)
- **`Command`**: A command to execute (from code blocks starting with `$ `)
- **`FileSnapshot`**: A file to verify after commands run (from code blocks with `snapshot=true`)
- **`TestConfig`**: Configuration from `mdtest.toml` blocks
- **`EnvironmentConfig`**: Python version, exclude-newer, timeouts, environment variables
- **`FilterConfig`**: Boolean flags for output filters (counts, exe_suffix, python_names, etc.)
- **`CodeBlockAttributes`**: Parsed attributes from code block info strings

### `parser.rs` - Markdown Parsing

Parses markdown files into `MarkdownTestFile` structures using `pulldown-cmark`.

**Key concepts:**

1. **Header hierarchy**: Headers (`#`, `##`, `###`) organize tests. Each leaf section (containing
   code blocks) becomes a test. Test names are derived from the header hierarchy (e.g., "Lock -
   Basic locking").

2. **Code block classification**: Code blocks are classified by their attributes:
   - `title="mdtest.toml"` → Configuration block (not written to disk)
   - `title="filename"` → Embedded file to create
   - `title="filename" snapshot=true` → File snapshot to verify
   - Starts with `$ ` → Command to execute

3. **Configuration inheritance**: File-level config (before any headers) applies to all tests.
   Section-level config merges with file-level config for that section only.

4. **Section independence**: Each section is an independent test with its own files. Files do not
   carry over between sections.

**Parser state machine:**

```
                   ┌─────────────────┐
                   │  ParserState    │
                   ├─────────────────┤
                   │ header_stack    │  ← Stack of (level, title) for test names
                   │ file_config     │  ← Config from blocks before any header
                   │ section_config  │  ← Override config for current section
                   │ current_files   │  ← Files for current section
                   │ current_commands│  ← Commands for current section
                   │ current_snapshots│ ← Snapshots for current section
                   │ tests           │  ← Completed tests
                   └─────────────────┘
```

When a new header is encountered, `flush_section()` creates a test from the current section state
(if it has commands or snapshots) and resets for the new section.

### `runner.rs` - Test Execution

Executes parsed tests and compares output.

**Two execution modes:**

1. **`run_test()`**: Standalone execution with `RunConfig`. Sets up environment, executes commands,
   compares output.

2. **`run_test_with_command_builder()`**: Integration with external test frameworks. Takes a closure
   that builds `Command` objects, allowing the caller to control how commands are set up (used by
   `TestContext` integration).

**Execution flow:**

```
1. Create embedded files in temp directory
2. For each command:
   a. Build Command (via builder or RunConfig)
   b. Execute and capture output
   c. Format output in uv_snapshot format
   d. Apply filters (regex replacements)
   e. Compare with expected output
   f. Return mismatch if different
3. For each file snapshot:
   a. Read file from temp directory
   b. Apply filters
   c. Compare with expected content
   d. Return mismatch if different
4. Return TestResult (passed/failed + mismatch details)
```

**Output format:**

Commands produce output in the `uv_snapshot` format:

```
success: true
exit_code: 0
----- stdout -----
<stdout content>
----- stderr -----
<stderr content>
```

### `snapshot.rs` - Snapshot Management

Handles comparing and updating snapshots in markdown files.

**`SnapshotMode`:**

- `Compare`: Compare output and fail on mismatch (default)
- `Update`: Update markdown files in place when output differs

Mode is determined by environment variables:

- `UV_UPDATE_SNAPSHOTS=1`
- `INSTA_UPDATE=1` or `INSTA_UPDATE=always`

**`update_snapshot()`:**

When updating, the function:

1. Reads the markdown file
2. Finds the code block at the mismatch line number
3. Replaces the content between the opening and closing ``` delimiters
4. Writes the updated file back

**`format_mismatch()`:**

Generates a human-readable diff for failed tests showing expected vs actual with `-`/`+` markers.

## Integration with uv Test Infrastructure

The test entry point (`crates/uv/tests/mdtest.rs`) integrates mdtest with uv's existing test
infrastructure:

```rust
// For each test in the markdown file:
let mut context = TestContext::new(python_version);

// Apply environment options
if let Some(exclude_newer) = &test.config.environment.exclude_newer {
    context = context.with_exclude_newer(exclude_newer);
}

// Apply filter options
if filters_config.counts {
    context = context.with_filtered_counts();
}

// Build commands using TestContext
let command_builder = |cmd_str: &str| -> Command {
    if cmd_name == "uv" {
        context.command()  // Uses TestContext's command setup
    } else {
        // Non-uv commands still get TestContext environment
    }
};

// Run test with the command builder
run_test_with_command_builder(test, context.temp_dir.path(), &filters, command_builder)
```

This provides:

- **Proper Python interpreter selection** via `TestContext`
- **Standard output filters** from `TestContext::filters()`
- **Isolated temp directories** per test
- **Consistent environment variables** (cache dir, exclude-newer, etc.)

## Markdown Format Reference

### File Creation

````markdown
```toml title="pyproject.toml"
[project]
name = "example"
```
````

### Command Execution

````markdown
```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
```
````

### File Snapshots

````markdown
```toml title="uv.lock" snapshot=true
version = 1
requires-python = ">=3.12"
```
````

### Configuration

````markdown
```toml title="mdtest.toml"
[environment]
python-version = "3.12"
exclude-newer = "2024-03-25T00:00:00Z"

[filters]
counts = true
```
````

## Design Decisions

| Decision                | Choice                           | Rationale                                               |
| ----------------------- | -------------------------------- | ------------------------------------------------------- |
| Parser library          | `pulldown-cmark`                 | Same as ty's mdtest, mature and fast                    |
| Test discovery          | `datatest-stable`                | Generates one test per file, integrates with cargo test |
| Section independence    | Each section is isolated         | Simplifies reasoning about tests, no hidden state       |
| Config inheritance      | File-level → Section-level merge | Reduces duplication while keeping tests explicit        |
| TestContext integration | Command builder pattern          | Reuses existing filter and environment setup            |
| Output format           | `uv_snapshot` format             | Consistent with existing integration tests              |

## Relationship to ty's mdtest and insta

This framework draws inspiration from two existing tools: **ty's mdtest** (for type-checker testing)
and **insta** (for snapshot testing). Understanding these influences helps explain our design
choices.

### ty's mdtest

ty (the Python type checker in the ruff project) uses a markdown-based test framework for testing
type inference and diagnostics. Key characteristics:

**Similarities we adopted:**

- **Markdown as test format**: Tests are written in `.md` files with code blocks
- **Header hierarchy for test organization**: Sections become individual tests
- **Configuration via TOML blocks**: Test settings embedded in the markdown
- **`datatest-stable` for test discovery**: Each file becomes a test case

**Key differences:**

| Aspect               | ty's mdtest                                        | uv-mdtest                            |
| -------------------- | -------------------------------------------------- | ------------------------------------ |
| Parser               | Hand-written character-by-character parser         | `pulldown-cmark` library             |
| File specification   | Backtick syntax: `` `foo.py`: ``                   | Attribute syntax: `title="foo.py"`   |
| Assertions           | Inline comments: `# revealed: int`, `# error: ...` | Command output comparison            |
| Auto-generated names | Yes, for `.py`/`.pyi` without explicit path        | No, all files need explicit `title=` |
| Merged snippets      | Multiple code blocks concatenate into one file     | Each code block is separate          |
| Testing target       | Type inference (runs type checker on code)         | CLI commands (executes processes)    |

**Why we differ:**

1. **Parser choice**: We use `pulldown-cmark` because our format is simpler (no inline assertions)
   and the library is well-tested. ty's hand-written parser handles complex inline comment parsing
   that we don't need.

2. **File specification syntax**: We use `title="..."` attributes because they're standard markdown
   code block syntax and work well with syntax highlighters. ty's backtick syntax is more compact
   but less standard.

3. **No merged snippets**: ty merges multiple anonymous code blocks into one file for readable
   documentation. We don't need this because our tests focus on file creation + command execution,
   not explaining code concepts.

4. **No inline assertions**: ty's `# revealed: int` comments are perfect for type-checking where you
   want to assert types at specific lines. For CLI testing, we compare entire command outputs.

### insta

insta is the de facto standard for snapshot testing in Rust. Key characteristics:

**What we learned:**

- **`INSTA_UPDATE` environment variable**: We honor this for snapshot updates (compatibility)
- **Separate expected vs actual comparison**: Clear distinction in our `Mismatch` type
- **Review workflow importance**: Snapshot testing needs good diff output

**Key differences:**

| Aspect            | insta                                        | uv-mdtest                              |
| ----------------- | -------------------------------------------- | -------------------------------------- |
| Snapshot storage  | Separate `.snap` files                       | Inline in markdown files               |
| Review tool       | `cargo-insta` interactive CLI                | Direct file update or test failure     |
| Assertion macros  | `assert_snapshot!`, `assert_debug_snapshot!` | Implicit via command output comparison |
| Redactions        | Built-in redaction system                    | Regex filters from `TestContext`       |
| Pending snapshots | `.snap.new` files for review                 | No pending state, update or fail       |

**Why we differ:**

1. **Inline snapshots**: We store expected output directly in markdown because the tests are
   documentation. Readers see input files, commands, and expected output together. Separate `.snap`
   files would break this narrative flow.

2. **No `cargo-insta`**: Our update mechanism modifies markdown files directly. This is simpler but
   less interactive. For complex reviews, users can use git diff.

3. **Regex filters vs redactions**: insta's redactions are powerful but complex. We reuse uv's
   existing `TestContext` filter system, which already handles paths, timestamps, and
   platform-specific output.

4. **No pending state**: insta's `.snap.new` workflow is great for large projects with many
   contributors. Our simpler "update or fail" approach fits uv's workflow where test authors
   typically run tests locally before committing.

### What we took from each

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              uv-mdtest                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   From ty's mdtest:                    From insta:                          │
│   ├── Markdown test format             ├── INSTA_UPDATE compatibility       │
│   ├── Header-based test organization   ├── Snapshot update workflow         │
│   ├── TOML configuration blocks        ├── Clear expected/actual separation │
│   ├── datatest-stable integration      └── Diff formatting for failures     │
│   └── Section independence                                                  │
│                                                                             │
│   Unique to uv-mdtest:                                                      │
│   ├── Command execution (not type-checking)                                 │
│   ├── uv_snapshot output format                                             │
│   ├── TestContext integration for filters                                   │
│   ├── title="..." attribute syntax                                          │
│   └── File snapshots with snapshot=true                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Design philosophy

The key insight from both tools is that **tests should be readable as documentation**. ty's mdtest
achieves this for type-checking by letting you write Python code with inline assertions. We achieve
this for CLI testing by showing the complete flow: create files → run commands → verify output.

We deliberately kept the format simpler than ty's because:

1. CLI output comparison is inherently simpler than type assertion matching
2. We can leverage `TestContext`'s existing filter infrastructure
3. Simpler format means easier adoption and fewer edge cases

We avoided insta's complexity because:

1. Markdown files are already version-controlled documentation
2. Inline snapshots maintain the narrative flow
3. uv's test infrastructure already solves the hard problems (filtering, environment setup)

## Performance Characteristics

Understanding the compile-time and runtime behavior is important for a test framework that will
contain many tests.

### Compile Time

| Scenario               | Time  | What happens                             |
| ---------------------- | ----- | ---------------------------------------- |
| Markdown file change   | ~0.2s | **No recompile** - files read at runtime |
| `mdtest.rs` change     | ~0.6s | Relinks test binary only                 |
| `uv-mdtest` lib change | ~0.8s | Recompiles library + relinks             |
| Full clean build       | ~18s  | Same as any uv test binary               |

**Key architectural decision**: Markdown files are parsed at runtime, not compile time. This means:

- Adding new test files requires no recompilation
- Editing test content requires no recompilation
- Only changes to the test harness (`mdtest.rs`) or library (`uv-mdtest`) trigger rebuilds

This is different from ty's mdtest, where test files are discovered at compile time via
`datatest-stable`. We use the same discovery mechanism, but the actual test content (commands,
expected output) is read when the test runs.

### Runtime and Concurrency

| Configuration        | 31 sections | Notes                         |
| -------------------- | ----------- | ----------------------------- |
| Per-file (old)       | ~6s         | Sections run sequentially     |
| Per-section (now)    | ~1.7s       | Full parallelism with nextest |
| Per-command overhead | ~0.5s       | Process spawn + uv execution  |

**Concurrency model (per-section parallelism)**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    nextest (parallel across sections)                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Process 1             Process 2             Process 3             ...     │
│   ┌─────────────┐      ┌─────────────┐      ┌─────────────┐                │
│   │lock.md::    │      │lock.md::    │      │sync.md::    │                │
│   │Basic locking│      │With deps    │      │Basic sync   │                │
│   └─────────────┘      └─────────────┘      └─────────────┘                │
│                                                                             │
│   Critical path: longest individual section (~1s)                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

We use [libtest-mimic](https://docs.rs/libtest-mimic/latest/libtest_mimic/) to generate one test per
section. This allows nextest to parallelize fully across all sections, with each section running in
its own process.

**Implementation**:

1. At test startup, scan all `.md` files with `walkdir`
2. Parse each file and extract all sections
3. Generate one `Trial` per section: `Trial::test("lock.md::Basic locking", || ...)`
4. Nextest runs each section in a separate process

**Test naming**: Tests are named `<file>::<section hierarchy>`, for example:

- `lock.md::Lock - Basic locking`
- `mdtest-features.md::MDTest Features - Section independence - First section`

**Filtering examples**:

```bash
# Run all tests in a file
cargo nextest run -E 'test(/lock.md/)'

# Run a specific section
cargo nextest run -E 'test(/Basic locking/)'

# Run all mdtest tests
cargo nextest run -p uv --test mdtest
```

**Why per-section parallelism**:

1. **Full parallelism**: Nextest runs each section in a separate process
2. **Better scaling**: Adding sections doesn't increase critical path
3. **Fine-grained filtering**: Can run specific sections by name
4. **Consistent with uv's nextest usage**: Large test suites benefit from maximum parallelism

### Dependencies

```
Test harness:
├── libtest-mimic      (custom test harness for per-section parallelism)
└── walkdir            (file discovery)

uv-mdtest library:
├── pulldown-cmark     (~0.4s compile time, markdown parsing)
├── regex, serde, toml (configuration and filtering)
└── fs-err, thiserror  (error handling)

Reused from test infrastructure:
└── TestContext        (all filter and environment setup)
```

The dependency footprint is minimal because we deliberately reuse existing infrastructure rather
than building new filtering or environment management.

### Scaling Considerations

With per-section parallelism, scaling is straightforward:

| Sections | Parallel time (est.) | Notes                                     |
| -------- | -------------------- | ----------------------------------------- |
| 31       | ~1.7s                | Current state                             |
| 100      | ~2-3s                | Limited by longest section + overhead     |
| 1000     | ~5-10s               | Nextest handles parallelism automatically |

**Key insight**: With per-section parallelism, the critical path is the longest individual section
(~1s), not the longest file. Adding more sections has minimal impact on total runtime.

**Recommendations for organizing tests**:

1. **One file per feature area**: `lock.md`, `sync.md`, `add.md`
2. **Keep sections focused**: Each section should test one scenario
3. **Use nextest filtering**: `cargo nextest run -E 'test(/lock/)'`
4. **CI job splitting**: Nextest partitions can distribute sections across jobs
