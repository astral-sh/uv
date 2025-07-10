# Dependency specifiers (PEP 508) in Rust

[![Crates.io](https://img.shields.io/crates/v/pep508_rs.svg?logo=rust&style=flat-square)](https://crates.io/crates/pep508_rs)
[![PyPI](https://img.shields.io/pypi/v/pep508_rs.svg?logo=python&style=flat-square)](https://pypi.org/project/pep508_rs)

A library for
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/),
previously known as [PEP 508](https://peps.python.org/pep-0508/).

## Usage

```rust
use std::str::FromStr;
use pep508_rs::Requirement;

let marker = r#"requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8""#;
let dependency_specification = Requirement::from_str(marker).unwrap();
assert_eq!(dependency_specification.name, "requests");
assert_eq!(dependency_specification.extras, Some(vec!["security".to_string(), "tests".to_string()]));
```

## Markers

Markers allow you to install dependencies only in specific environments (python version, operating
system, architecture, etc.) or when a specific feature is activated. E.g., you can say
`importlib-metadata ; python_version < "3.8"` or `itsdangerous (>=1.1.0) ; extra == 'security'`.
Unfortunately, the marker grammar has some oversights (e.g.
<https://github.com/pypa/packaging.python.org/pull/1181>) and the design of comparisons (PEP 440
comparisons with lexicographic fallback) leads to confusing outcomes. This implementation tries to
carefully validate everything and emit warnings whenever bogus comparisons with unintended semantics
are made.
