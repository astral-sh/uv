# CLAUDE.md - AI Assistant Guide for uv

This document provides comprehensive guidance for AI assistants working with the uv codebase. uv is an extremely fast Python package and project manager written in Rust.

## Table of Contents

1. [Project Overview](#project-overview)
2. [Codebase Structure](#codebase-structure)
3. [Technology Stack](#technology-stack)
4. [Development Workflows](#development-workflows)
5. [Testing Guidelines](#testing-guidelines)
6. [Code Conventions](#code-conventions)
7. [Architecture Patterns](#architecture-patterns)
8. [Common Tasks](#common-tasks)
9. [CI/CD Pipeline](#cicd-pipeline)
10. [Important Constraints](#important-constraints)

## Project Overview

**uv** is a comprehensive Python toolchain that replaces pip, pip-tools, pipx, poetry, pyenv, twine, virtualenv, and more. It's 10-100x faster than pip and provides:

- Universal dependency resolution with lockfiles
- Python version management
- Project and workspace management
- Tool installation (like pipx)
- Build backend and publishing capabilities
- pip-compatible interface

**Key Facts:**
- **Language:** Rust (primary), Python (testing/bindings)
- **License:** Dual MIT/Apache-2.0
- **Organization:** Astral (creators of Ruff)
- **Repository Structure:** Monorepo with 63 crates
- **Platforms:** macOS, Linux, Windows (18 target platforms total)

## Codebase Structure

### Directory Layout

```
uv/
├── crates/                 # 63 Rust crates (core implementation)
│   ├── uv/                # Main binary and CLI orchestration
│   ├── uv-cli/            # Command-line argument parsing
│   ├── uv-resolver/       # PubGrub-based dependency resolution
│   ├── uv-distribution/   # Distribution fetching and building
│   ├── uv-installer/      # Package installation
│   ├── uv-python/         # Python interpreter detection
│   ├── uv-pep440/         # Version parsing (PEP 440)
│   ├── uv-pep508/         # Dependency specifiers (PEP 508)
│   ├── uv-client/         # PyPI HTTP client
│   ├── uv-cache/          # Global caching system
│   ├── uv-workspace/      # Workspace management
│   ├── uv-dev/            # Development tools CLI
│   └── ...                # 50+ additional specialized crates
├── docs/                  # MkDocs documentation
│   ├── getting-started/   # Installation and intro
│   ├── guides/            # Task-oriented guides
│   ├── concepts/          # Detailed explanations
│   ├── reference/         # Auto-generated CLI/settings docs
│   └── pip/               # pip-compatible interface docs
├── scripts/               # Development and testing utilities
│   ├── benchmark/         # Benchmarking infrastructure
│   ├── packages/          # Test package fixtures
│   ├── requirements/      # Test requirement files
│   └── scenarios/         # Test scenarios
├── ecosystem/             # Integration tests with real projects
│   ├── airflow/           # Apache Airflow
│   ├── transformers/      # Hugging Face Transformers
│   └── ...               # Other major Python projects
├── python/                # Python bindings and distribution
│   └── uv/               # Python package
├── .github/workflows/     # CI/CD configuration
└── [various config files] # Build, lint, format configs
```

### Key Crates by Layer

**CLI Layer:**
- `uv-cli`: Clap-based argument parsing
- `uv`: Main binary with command implementations in `/src/commands/`

**Resolution Layer:**
- `uv-resolver`: Dependency resolution using PubGrub
- `uv-distribution`: Fetching and building distributions
- `uv-distribution-types`: Type definitions for distributions

**Installation Layer:**
- `uv-installer`: Installation orchestration
- `uv-install-wheel`: Wheel file installation
- `uv-virtualenv`: Virtual environment creation

**Python Ecosystem:**
- `uv-pep440`: Version parsing and comparison
- `uv-pep508`: Dependency specifiers
- `uv-pep517`: Build interface
- `uv-platform-tags`: PEP 425 platform tags
- `uv-pypi-types`: PyPI API types

**Utilities:**
- `uv-cache`: Global cache infrastructure
- `uv-fs`: Filesystem utilities (use instead of std::fs)
- `uv-git`: Git operations
- `uv-auth`: Authentication (keyring, netrc, etc.)
- `uv-configuration`: Configuration management

## Technology Stack

### Languages & Frameworks

**Rust (Primary):**
- Edition: 2024
- Style Edition: 2024
- Toolchain: 1.90 (minimum: 1.88)
- Build System: Cargo workspace with 63 crates

**Python:**
- Versions: 3.8-3.14 (including pre-releases)
- Testing: Uses multiple Python versions
- Bindings: Maturin for Python package distribution

### Key Dependencies

**Core Libraries:**
- `tokio`: Async runtime (fs, io-util, macros, process, rt, signal, sync)
- `clap`: CLI parsing (derive, string, wrap_help)
- `reqwest`: HTTP client with middleware and retry
- `serde`, `serde_json`: Serialization
- `toml`, `toml_edit`: TOML parsing
- `pubgrub`: Dependency resolution (custom fork)

**Compression & Archives:**
- `async_zip`, `async-compression` (bzip2, gzip, xz, zstd)
- `flate2`: DEFLATE compression

**Concurrency:**
- `dashmap`: Concurrent hash maps
- `async-channel`: Async channels
- `boxcar`: Lock-free data structures

**Testing:**
- `insta`: Snapshot testing (primary testing approach)
- `assert_cmd`, `assert_fs`: CLI and filesystem testing
- `wiremock`: HTTP mocking

**Quality Tools:**
- `rustfmt`: Code formatting
- `clippy`: Linting (pedantic mode)
- `ruff`: Python formatting/linting
- `prettier`: Markdown/JSON/YAML formatting
- `typos`: Spell checking

## Development Workflows

### Initial Setup

```bash
# Install Rust toolchain (required)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install C compiler (Linux)
sudo apt install build-essential  # Debian/Ubuntu
sudo dnf install gcc              # Fedora

# Install Python versions for testing
cargo run python install

# Install development dependencies
cargo install cargo-nextest cargo-insta
```

### Common Development Commands

**Building:**
```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Profiling build (for benchmarking)
cargo build --profile profiling

# Run uv in development
cargo run -- <args>
cargo run -- venv
cargo run -- pip install requests
```

**Testing:**
```bash
# Run tests with nextest (recommended)
cargo nextest run

# Run specific test
cargo test --package uv --test <test_name>

# Run test with specific name pattern
cargo test --package uv --test <test_file> -- <test_name> --exact

# Review snapshot tests
cargo insta review

# Update JSON schema if tests fail
cargo dev generate-json-schema
```

**Code Quality:**
```bash
# Format code
cargo fmt

# Lint code
cargo clippy

# Check all quality at once (pre-commit)
cargo fmt --all --check
cargo clippy --workspace --all-features -- -D warnings
```

**Documentation:**
```bash
# Generate all auto-generated documentation
cargo dev generate-all

# Preview documentation locally
uvx --with-requirements docs/requirements.txt -- mkdocs serve -f mkdocs.public.yml

# Format markdown files
npx prettier --prose-wrap always --write "**/*.md"
```

**Development Tools (uv-dev):**
```bash
# Generate CLI reference documentation
cargo dev generate-cli-reference

# Generate environment variable reference
cargo dev generate-env-vars-reference

# Generate JSON schema
cargo dev generate-json-schema

# Validate wheel files
cargo dev validate-zip <path-to-wheel>

# Extract wheel metadata
cargo dev wheel-metadata <path-to-wheel>
```

### File Locations Reference

When making changes, here are the key files you'll likely need:

**Adding/Modifying Commands:**
- CLI definitions: `crates/uv-cli/src/lib.rs`
- Command implementations: `crates/uv/src/commands/<category>/`
- Settings: `crates/uv/src/settings.rs` (130 KB)

**Python Ecosystem Support:**
- Version parsing: `crates/uv-pep440/`
- Dependency specs: `crates/uv-pep508/`
- Platform tags: `crates/uv-platform-tags/`

**Resolution & Installation:**
- Resolver: `crates/uv-resolver/`
- Installer: `crates/uv-installer/`
- Distribution handling: `crates/uv-distribution/`

**Configuration:**
- Settings types: `crates/uv-settings/`
- Workspace config: `crates/uv-workspace/`
- Configuration parsing: `crates/uv-configuration/`

**Documentation:**
- Auto-generated: Generated by `cargo dev generate-all`
- Manual docs: `docs/` directory
- CLI help text: Embedded in `uv-cli` crate

## Testing Guidelines

### Test Organization

**Integration Tests:**
- Location: `crates/uv/tests/it/`
- 58 snapshot files (`.snap`)
- Uses `insta` for snapshot testing
- Major test files: `lock.rs`, `pip_compile.rs`, `pip_install.rs`, `sync.rs`

**Test Context:**
- Harness: `TestContext` in `crates/uv/tests/it/common/mod.rs`
- Snapshot macro: `uv_snapshot!`
- Packse integration for dependency scenarios

### Writing Tests

**Example Test:**
```rust
#[test]
fn test_add_requests() {
    let context = TestContext::new("3.12");
    uv_snapshot!(context.filters(), context.add().arg("requests"), @"");
}
```

**Running Specific Tests:**
```bash
# Run single test
cargo test --package uv --test <test_file> -- <test_name> --exact

# Review snapshots
cargo insta review

# Accept all snapshots
cargo insta accept
```

### Test Requirements

**Python Versions:**
Test suite requires Python 3.8-3.14 (managed via `.python-versions`):
```bash
# Install test Python versions
cargo run python install
```

**Environment Variables:**
- `UV_PYTHON_INSTALL_DIR`: Storage directory for Python installations (must be absolute)

### Test Features

Control which tests run via cargo features:
- `crates-io`, `git`, `pypi`, `r2`: External service tests
- `python`, `python-patch`, `python-eol`, `python-managed`: Python version tests
- `slow-tests`: Long-running tests
- `test-ecosystem`: Real-world package tests

## Code Conventions

### Rust Style

**Formatting (rustfmt.toml):**
- Edition: 2024
- Style edition: 2024
- Run: `cargo fmt`

**Linting (clippy.toml):**
- Pedantic warnings enabled
- Disallowed: `std::fs` (use `uv-fs` instead), `tokio::fs` (use `uv-fs` instead)
- Run: `cargo clippy`

**Workspace Lints:**
```rust
unsafe_code = "warn"          // Minimize unsafe code
unreachable_pub = "warn"      // No unnecessary public items
print_stdout = "warn"         // Use tracing instead
print_stderr = "warn"         // Use tracing instead
dbg_macro = "warn"           // Remove debug macros
use_self = "warn"            // Prefer Self over type name
```

**File System Operations:**
- ❌ NEVER use `std::fs` or `tokio::fs` directly
- ✅ ALWAYS use `uv_fs` crate (enforced by clippy)
- Rationale: Consistent error handling and Windows compatibility

**Error Handling:**
- Use `anyhow` for error propagation
- Use `miette` for user-facing errors
- Include context with `.context()` or `.with_context()`

### Python Style

**Formatting (ruff.toml):**
- Target version: py312
- Extended selects: `["I", "B"]` (isort, flake8-bugbear)
- Run: `ruff format` and `ruff check`

**Type Checking:**
- Tool: mypy
- Configured to ignore missing imports
- Files: `crates/uv-python/*.py`, `python/uv/*.py`

### Documentation Style

**General Conventions:**
- Line length: 100 characters (markdown)
- Use "e.g." and "i.e." wrapped in commas: ", e.g., "
- Em-dashes with spaces: "hello — world"
- Hyphenate compound words: "platform-specific"
- Backticks for: commands, code, package names, file paths

**Styling "uv":**
- ❌ NEVER: "Uv", "UV", "uV"
- ✅ ALWAYS: "uv" (lowercase)
- No backticks unless referring to the `uv` executable
- Environment variables: `UV_PYTHON` (uppercase with prefix)

**Terminology:**
- "lockfile" (not "lock file")
- "pre-release" (not "prerelease")
- "command-line" (hyphenated as adjective)

**CLI Output Formatting:**
- No periods unless multi-sentence
- May use second-person
- Colors:
  - Green: Success, commands
  - Red: Errors
  - Yellow: Warnings
  - Cyan: Hints, paths, literals

**Documentation Types:**
1. **Guides** (`docs/guides/`): Second-person, imperative, basic coverage
2. **Concepts** (`docs/concepts/`): Third-person, detailed explanations
3. **Reference** (`docs/reference/`): Third-person, exhaustive, auto-generated

**Auto-generated Documentation:**
Files are auto-generated by `cargo dev generate-all`:
- CLI reference: `docs/reference/cli.md`
- Settings reference: `docs/reference/settings.md`
- Environment variables: `docs/reference/environment.md`

⚠️ **NEVER manually edit auto-generated files!** Update source code and regenerate.

### EditorConfig Settings

```ini
[*]
charset = utf-8
end_of_line = lf
trim_trailing_whitespace = true
insert_final_newline = true

[*.{rs,py}]
indent_style = space
indent_size = 4

[*.{yml,yaml,json,toml,md}]
indent_style = space
indent_size = 2

[*.md]
max_line_length = 100
```

## Architecture Patterns

### Design Principles

1. **Modular Crate Design**: 63 focused crates with clear boundaries
2. **Trait-based Abstractions**: Shared traits in `uv-types` avoid circular dependencies
3. **Async-first**: Tokio-based async I/O for performance
4. **Global Cache**: Deduplication across projects via `uv-cache`
5. **Type Safety**: Strong typing with dedicated types for distributions, versions, requirements
6. **Workspace Support**: Cargo-style workspaces for monorepos

### Key Architectural Components

**Command Dispatch Pattern:**
```
uv-cli (parsing)
  → uv/src/commands/ (orchestration)
    → uv-resolver (resolution)
    → uv-installer (installation)
    → uv-cache (caching)
```

**Resolution Flow:**
```
Request → uv-client (fetch metadata)
        → uv-resolver (PubGrub resolution)
        → uv-distribution (build if needed)
        → uv-installer (install wheels)
```

**Configuration Hierarchy:**
```
CLI args > Environment vars > pyproject.toml > uv.toml > defaults
```

**Cache Organization:**
The global cache (`uv-cache`) provides:
- Wheel cache: Built distributions
- Source dist cache: Downloaded archives
- Git cache: Git repositories
- Simple API cache: PyPI metadata
- Interpreter cache: Python installations

### Module Boundaries

**Core Rule:** Lower-level crates should NOT depend on higher-level crates

**Dependency Levels:**
1. **Foundation**: `uv-fs`, `uv-normalize`, `uv-small-str`
2. **Python Types**: `uv-pep440`, `uv-pep508`, `uv-platform-tags`
3. **Client**: `uv-client`, `uv-auth`, `uv-cache`
4. **Resolution**: `uv-resolver`, `uv-distribution`
5. **Installation**: `uv-installer`, `uv-install-wheel`
6. **Orchestration**: `uv`, `uv-cli`

## Common Tasks

### Adding a New CLI Command

1. **Define the command** in `crates/uv-cli/src/lib.rs`:
   ```rust
   #[derive(Parser)]
   pub struct MyCommand {
       /// Help text for option
       #[arg(long)]
       pub my_option: bool,
   }
   ```

2. **Add to command enum** in same file:
   ```rust
   pub enum Commands {
       // ...
       MyCommand(MyCommand),
   }
   ```

3. **Implement the command** in `crates/uv/src/commands/my_command.rs`:
   ```rust
   pub(crate) async fn my_command(
       args: &MyCommand,
       printer: Printer,
   ) -> Result<ExitStatus> {
       // Implementation
   }
   ```

4. **Wire up in main** (`crates/uv/src/lib.rs`):
   ```rust
   Commands::MyCommand(args) => {
       commands::my_command::my_command(args, printer).await
   }
   ```

5. **Add tests** in `crates/uv/tests/it/`:
   ```rust
   #[test]
   fn test_my_command() {
       let context = TestContext::new("3.12");
       uv_snapshot!(context.filters(), context.my_command(), @"");
   }
   ```

6. **Regenerate documentation**:
   ```bash
   cargo dev generate-all
   ```

### Adding a New Configuration Option

1. **Add to settings** in `crates/uv-settings/src/settings.rs`
2. **Add to CLI args** in `crates/uv-cli/src/lib.rs`
3. **Add to pyproject.toml schema** in relevant settings file
4. **Update resolver/installer** to use the new setting
5. **Regenerate documentation**: `cargo dev generate-all`
6. **Add tests** for the new option

### Adding Support for a New PEP

1. **Create new crate** if it's a major feature: `crates/uv-pep###/`
2. **Add types** for the new specification
3. **Implement parsing** and validation
4. **Add to resolver/installer** as needed
5. **Add comprehensive tests** with edge cases
6. **Document** in `docs/concepts/`

### Debugging Tips

**Enable trace logging:**
```bash
RUST_LOG=trace cargo run -- <command>
RUST_LOG=uv=trace cargo run -- <command>  # uv crates only
```

**Profile concurrency:**
```bash
RUST_LOG=uv=info TRACING_DURATIONS_FILE=target/traces/output.ndjson \
  cargo run --features tracing-durations-export --profile profiling -- \
  pip compile requirements.in
```

**Inspect cache:**
```bash
cargo run -- cache dir
cargo run -- cache clean
```

**Run in Docker (for untrusted packages):**
```bash
docker build -t uv-builder -f crates/uv-dev/builder.dockerfile --load .
docker run --rm -it -v $(pwd):/app uv-builder /app/target/release/uv <command>
```

## CI/CD Pipeline

### GitHub Actions Workflows

**Main CI** (`.github/workflows/ci.yml`):
1. **determine_changes**: Skip if only docs changed
2. **lint**: rustfmt, prettier, ruff, mypy, shellcheck
3. **cargo-clippy**: Workspace-wide linting with all features
4. **test**: Multi-platform matrix (macOS, Ubuntu, Windows)

**Build & Release**:
- `build-binaries.yml`: 18 platform targets via cargo-dist
- `build-docker.yml`: Docker image builds
- `release.yml`: Automated release on version tag

**Publishing**:
- `publish-pypi.yml`: PyPI via Maturin
- `publish-docs.yml`: Docs to Cloudflare Pages

### Pre-commit Hooks

Configured in `.pre-commit-config.yaml`:
1. `validate-pyproject`: Validate pyproject.toml
2. `typos`: Spell checking
3. `cargo fmt`: Rust formatting
4. `cargo dev generate-all`: Auto-generate docs
5. `prettier`: YAML/JSON5 formatting
6. `ruff-format`, `ruff`: Python formatting/linting

### Release Process

**For Astral team members only:**

1. Run the release script:
   ```bash
   ./scripts/release.sh
   ```

2. Editorialize `CHANGELOG.md` for consistent styling

3. Open PR: "Bump version to X.Y.Z"

4. After merge, run release workflow with version (no `v` prefix):
   ```
   Version: 0.9.5  (NOT v0.9.5)
   ```

5. GitHub release created automatically after builds complete

## Important Constraints

### What to AVOID

❌ **Never manually edit auto-generated files:**
- `docs/reference/cli.md`
- `docs/reference/settings.md`
- `docs/reference/environment.md`
- Schema files
→ Always run `cargo dev generate-all` after changes

❌ **Never use std::fs or tokio::fs:**
- Always use `uv_fs` crate
- Enforced by clippy, will fail CI

❌ **Never use print!/println!/eprintln!:**
- Use `tracing::info!`, `tracing::warn!`, etc.
- Or use `printer.writeln()` for output
- Enforced by clippy warnings

❌ **Never use dbg! macro in committed code:**
- Use `tracing::debug!` instead
- Enforced by clippy warnings

❌ **Don't open PRs for features without discussion:**
- Features require team consensus first
- Check issues labeled `needs-design` or `needs-decision`

❌ **Don't ignore test failures:**
- All tests must pass before merge
- Snapshot tests must be reviewed with `cargo insta review`

### Security Considerations

⚠️ **Source distributions can execute arbitrary code:**
- Use Docker container for untrusted packages (see Debugging Tips)
- Be cautious when adding dependencies
- cargo-deny is run in CI for critical crates

**Authentication:**
- Keyring support via `uv-keyring`
- Netrc support via `uv-auth`
- Never log credentials or tokens
- Obfuscate tokens in output (see `uv auth token`)

## Additional Resources

**Documentation:**
- User docs: https://docs.astral.sh/uv
- Contributing guide: `CONTRIBUTING.md`
- Style guide: `STYLE.md`
- Benchmarks: `BENCHMARKS.md`
- Pip compatibility: `PIP_COMPATIBILITY.md`

**Community:**
- GitHub: https://github.com/astral-sh/uv
- Discord: https://discord.gg/astral-sh
- Issues: https://github.com/astral-sh/uv/issues

**Related Projects:**
- Ruff (Python linter): https://github.com/astral-sh/ruff
- PubGrub (resolver): https://github.com/pubgrub-rs/pubgrub

---

## Quick Reference Card

### Most Common Commands

```bash
# Development
cargo run -- <uv-args>           # Run uv
cargo nextest run                # Run tests
cargo insta review               # Review snapshots
cargo dev generate-all           # Regenerate docs
cargo fmt && cargo clippy        # Format & lint

# Testing specific areas
cargo test --package uv --test pip_install
cargo test --package uv-resolver
cargo test --package uv-pep440

# Documentation
uvx --with-requirements docs/requirements.txt -- mkdocs serve -f mkdocs.public.yml
npx prettier --prose-wrap always --write "**/*.md"

# Debugging
RUST_LOG=trace cargo run -- <command>
cargo run -- cache dir
cargo run -- cache clean
```

### File Locations Cheat Sheet

| Task | Location |
|------|----------|
| Add CLI command | `crates/uv-cli/src/lib.rs` |
| Implement command | `crates/uv/src/commands/<category>/` |
| Add configuration | `crates/uv-settings/`, `crates/uv-cli/` |
| Modify resolution | `crates/uv-resolver/` |
| Modify installation | `crates/uv-installer/` |
| Add tests | `crates/uv/tests/it/` |
| Update docs | `docs/` (manual), `cargo dev generate-all` (auto) |
| Fix typos | Run `typos` locally |
| Format code | `cargo fmt`, `ruff format`, `prettier` |

---

**Last Updated:** 2025-01-15 (based on uv v0.9.4)

This guide is maintained for AI assistants. For human contributors, see `CONTRIBUTING.md`.
