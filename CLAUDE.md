- Read CONTRIBUTING.md for guidelines on how to run tools
- ALWAYS attempt to add a test case for changed behavior
- PREFER integration tests, e.g., at `it/...` over unit tests
- When making changes for Windows from Unix, use `cargo xwin clippy` to check compilation
- NEVER perform builds with the release profile, unless asked or reproducing performance issues
- PREFER running specific tests over running the entire test suite
- AVOID using `panic!`, `unreachable!`, `.unwrap()`, unsafe code, and clippy rule ignores
- PREFER patterns like `if let` to handle fallibility
- ALWAYS write `SAFETY` comments following our usual style when writing `unsafe` code
- PREFER `#[expect()]` over `[allow()]` if clippy must be disabled
- PREFER let chains (`if let` combined with `&&`) over nested `if let` statements
- NEVER update all dependencies in the lockfile and ALWAYS use `cargo update --precise` to make
  lockfile changes
- NEVER assume clippy warnings are pre-existing, it is very rare that `main` has warnings
- ALWAYS read and copy the style of similar tests when adding new cases
- PREFER top-level imports over local imports or fully qualified names
- AVOID shortening variable names, e.g., use `version` instead of `ver`, and `requires_python`
  instead of `rp`
