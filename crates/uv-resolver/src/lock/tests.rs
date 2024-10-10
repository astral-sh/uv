use super::*;

#[test]
fn missing_dependency_source_unambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn missing_dependency_version_unambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn missing_dependency_source_version_unambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn missing_dependency_source_ambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
version = "0.1.0"
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn missing_dependency_version_ambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
source =  { registry = "https://pypi.org/simple" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn missing_dependency_source_version_ambiguous() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "a"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "a"
version = "0.1.1"
source = { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package]]
name = "b"
version = "0.1.0"
source =  { registry = "https://pypi.org/simple" }
sdist = { url = "https://example.com", hash = "sha256:37dd54208da7e1cd875388217d5e00ebd4179249f90fb72437e91a35459a0ad3", size = 0 }

[[package.dependencies]]
name = "a"
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn hash_optional_missing() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }]
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn hash_optional_present() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { registry = "https://pypi.org/simple" }
wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn hash_required_present() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { path = "file:///foo/bar" }
wheels = [{ url = "file:///foo/bar/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn source_direct_no_subdir() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn source_direct_has_subdir() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { url = "https://burntsushi.net", subdirectory = "wat/foo/bar" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn source_directory() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { directory = "path/to/dir" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}

#[test]
fn source_editable() {
    let data = r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "anyio"
version = "4.3.0"
source = { editable = "path/to/dir" }
"#;
    let result: Result<Lock, _> = toml::from_str(data);
    insta::assert_debug_snapshot!(result);
}
