- Read CONTRIBUTING.md for guidelines on how to run tools
- ALWAYS ensure that new tests use the same style as existing tests for all parts of the test
- ALWAYS check whether the behavior of a new test is already covered by an existing test
- PREFER integration tests, e.g., at `it/...` over unit tests
- PREFER running specific tests over running the entire test suite
- PREFER `insta` snapshots following patterns in nearby tests over substring assertions
- When making changes for Windows from Unix, use `cargo xwin clippy` to check compilation
- NEVER perform builds with the release profile, unless asked or reproducing performance issues
- AVOID using `panic!`, `unreachable!`, `.unwrap()`, unsafe code, and clippy rule ignores
- PREFER patterns like `if let` to handle fallibility
- PREFER exhaustive `match` expressions without wildcard (`_`) arms over `matches!`, so new enum
  variants require explicit handling
- ALWAYS write `SAFETY` comments following our usual style when writing `unsafe` code
- PREFER `#[expect()]` over `[allow()]` if clippy must be disabled
- PREFER let chains (`if let` combined with `&&`) over nested `if let` statements
- NEVER update all dependencies in the lockfile and ALWAYS use `cargo update --precise` to make
  lockfile changes
- NEVER assume clippy warnings or test failures are pre-existing, it is very rare that `main` has
  warnings
- ALWAYS use `.github/automations-dispatch.json` to trigger privileged workflows from GitHub webhook
  events instead of adding `pull_request_target` workflows
- NEVER suppress the `dangerous-triggers` security lint; extend the automation dispatcher in a
  separate pull request if it does not support the required event
- PREFER top-level imports over local imports or fully qualified names
- AVOID shortening variable names, e.g., use `version` instead of `ver`, and `requires_python`
  instead of `rp`
- PREFER [`TypeName`] references when writing Rust doc comments
