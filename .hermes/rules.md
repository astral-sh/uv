# Hermes Agent Guidelines for uv

This folder helps Hermes agents (AI coding assistants) understand and safely contribute to this repository.

## Repository Overview

- **Purpose:** Fast, reliable Python package installer and resolver (Rust implementation)
- **Language:** Rust
- **Build tool:** cargo
- **Test command:** `cargo nextest run` (or `cargo test`)

## What Hermes Should Do

✓ Fix bugs from reported issues labeled `help wanted` or `bug`  
✓ Add or improve tests for uncovered code paths  
✓ Improve documentation and README  
✓ Refactor code for clarity (small scope)  
✓ Add type hints or improve error messages  

## What Hermes Should NOT Do

✗ Add new public APIs or change existing APIs without discussion  
✗ Major architectural changes without team consensus  
✗ Add external dependencies  
✗ Modify CI/CD workflows (`.github/workflows/**`)  
✗ Touch lock files (`Cargo.lock`)  
✗ Work on issues labeled `needs-decision` or `needs-design`  

## Setup Instructions

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install C compiler (Linux)
sudo apt install build-essential

# Clone and build
git clone https://github.com/astral-sh/uv.git
cd uv
cargo build --release
```

## Verification Commands

Before submitting a PR, Hermes must verify:

```bash
# Formatting
cargo fmt --check

# Linting
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Tests
cargo nextest run
cargo insta test --accept --test-runner nextest  # for snapshot tests
```

## Key Files to Understand

- `CONTRIBUTING.md` — contribution guidelines and AI policy
- `README.md` — project overview
- `Cargo.toml` — workspace manifest
- `crates/` — organized by subsystem
- `tests/` — integration tests

## Issue Labels to Target

Good for Hermes contributions:
- `help wanted` — explicitly good for community
- `good-first-issue` — lower barrier to entry
- `bug` — concrete, scoped work
- `documentation` — writing improvements

Avoid:
- `needs-decision` — needs team input
- `needs-design` — requires consensus first
- `blocked-by-upstream` — external blocker

## Quick Tips

1. Read recent merged PRs to understand patterns
2. Always run `cargo nextest run` locally before committing
3. Follow existing code style (Rust idioms)
4. Keep PRs focused — one issue per PR
5. Reference the issue number in your commit message
6. Check AI Policy: https://github.com/astral-sh/.github/blob/main/AI_POLICY.md

---

For more about Hermes Agent, see: https://hermes-agent.nousresearch.com
