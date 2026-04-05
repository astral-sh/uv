//! Generate minimal Python wheels and source distributions in memory.
//!
//! Packse scenario packages are trivial: they contain only metadata and a stub
//! `__init__.py`. We generate them directly without invoking a Python build backend.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::GzEncoder;
use indoc::formatdoc;
use sha2::{Digest, Sha256};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Generate a wheel (`.whl`) as an in-memory ZIP archive.
///
/// Returns `(filename, bytes)`.
pub fn generate_wheel(
    name: &str,
    version: &str,
    requires: &[String],
    extras: &BTreeMap<String, Vec<String>>,
    requires_python: Option<&str>,
    tag: &str,
) -> (String, Vec<u8>) {
    let normalized = name.replace('-', "_");
    let dist_info = format!("{normalized}-{version}.dist-info");

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    // __init__.py
    let init_py = format!("__version__ = \"{version}\"\n");
    zip.start_file(format!("{normalized}/__init__.py"), opts)
        .unwrap();
    zip.write_all(init_py.as_bytes()).unwrap();

    // METADATA
    let metadata = build_metadata(name, version, requires, extras, requires_python);
    zip.start_file(format!("{dist_info}/METADATA"), opts)
        .unwrap();
    zip.write_all(metadata.as_bytes()).unwrap();

    // WHEEL
    let wheel_info = format!(
        "Wheel-Version: 1.0\n\
         Generator: uv-test\n\
         Root-Is-Purelib: true\n\
         Tag: {tag}\n"
    );
    zip.start_file(format!("{dist_info}/WHEEL"), opts).unwrap();
    zip.write_all(wheel_info.as_bytes()).unwrap();

    // RECORD (empty – not validated for our test purposes)
    zip.start_file(format!("{dist_info}/RECORD"), opts).unwrap();
    zip.write_all(b"").unwrap();

    let bytes = zip.finish().unwrap().into_inner();
    let filename = format!("{normalized}-{version}-{tag}.whl");
    (filename, bytes)
}

/// Generate a source distribution (`.tar.gz`) as an in-memory tarball.
///
/// The sdist contains a `pyproject.toml` using `hatchling` as build backend,
/// `PKG-INFO` with full metadata, and a stub module.
///
/// Returns `(filename, bytes)`.
pub fn generate_sdist(
    name: &str,
    version: &str,
    requires: &[String],
    extras: &BTreeMap<String, Vec<String>>,
    requires_python: Option<&str>,
) -> (String, Vec<u8>) {
    let normalized = name.replace('-', "_");
    let prefix = format!("{normalized}-{version}");

    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut tar = tar::Builder::new(encoder);

    // pyproject.toml
    let pyproject = build_pyproject_toml(name, version, requires, extras, requires_python);
    append_tar_file(
        &mut tar,
        &format!("{prefix}/pyproject.toml"),
        pyproject.as_bytes(),
    );

    // PKG-INFO (same format as METADATA — allows metadata extraction without building)
    let pkg_info = build_metadata(name, version, requires, extras, requires_python);
    append_tar_file(&mut tar, &format!("{prefix}/PKG-INFO"), pkg_info.as_bytes());

    // src/{module}/__init__.py
    let init_py = format!("__version__ = \"{version}\"\n");
    append_tar_file(
        &mut tar,
        &format!("{prefix}/src/{normalized}/__init__.py"),
        init_py.as_bytes(),
    );

    let encoder = tar.into_inner().unwrap();
    let bytes = encoder.finish().unwrap();
    let filename = format!("{normalized}-{version}.tar.gz");
    (filename, bytes)
}

/// Build PEP 566 / PEP 643 metadata content.
fn build_metadata(
    name: &str,
    version: &str,
    requires: &[String],
    extras: &BTreeMap<String, Vec<String>>,
    requires_python: Option<&str>,
) -> String {
    use std::fmt::Write as _;

    let mut meta = String::new();
    meta.push_str("Metadata-Version: 2.3\n");
    writeln!(&mut meta, "Name: {name}").unwrap();
    writeln!(&mut meta, "Version: {version}").unwrap();
    if let Some(rp) = requires_python {
        writeln!(&mut meta, "Requires-Python: {rp}").unwrap();
    }

    // Extras
    for extra_name in extras.keys() {
        writeln!(&mut meta, "Provides-Extra: {extra_name}").unwrap();
    }

    // Dependencies
    for dep in requires {
        writeln!(&mut meta, "Requires-Dist: {dep}").unwrap();
    }
    for (extra_name, extra_deps) in extras {
        for dep in extra_deps {
            // Append the extra condition to each dependency
            if let Some((req, markers)) = dep.split_once(';') {
                // Already has a marker — wrap in parens to preserve precedence
                writeln!(
                    &mut meta,
                    "Requires-Dist: {req} ; ({}) and extra == \"{extra_name}\"",
                    markers.trim()
                )
                .unwrap();
            } else {
                writeln!(
                    &mut meta,
                    "Requires-Dist: {dep} ; extra == \"{extra_name}\""
                )
                .unwrap();
            }
        }
    }

    meta
}

/// Build a minimal `pyproject.toml` for an sdist using hatchling.
fn build_pyproject_toml(
    name: &str,
    version: &str,
    requires: &[String],
    extras: &BTreeMap<String, Vec<String>>,
    requires_python: Option<&str>,
) -> String {
    use std::fmt::Write as _;

    let normalized = name.replace('-', "_");
    let mut out = formatdoc! {
        r#"
        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.hatch.build.targets.wheel]
        packages = ["src/{normalized}"]

        [tool.hatch.build.targets.sdist]
        only-include = ["src/{normalized}"]

        [project]
        name = "{name}"
        version = "{version}"
        "#
    };

    if requires.is_empty() {
        out.push_str("dependencies = []\n");
    } else {
        out.push_str("dependencies = [\n");
        for dependency in requires {
            writeln!(&mut out, "    \"{dependency}\",").unwrap();
        }
        out.push_str("]\n");
    }

    if let Some(requires_python) = requires_python {
        writeln!(&mut out, "requires-python = \"{requires_python}\"").unwrap();
    }

    if !extras.is_empty() {
        out.push_str("\n[project.optional-dependencies]\n");
        for (extra_name, extra_dependencies) in extras {
            writeln!(&mut out, "{extra_name} = [").unwrap();
            for dependency in extra_dependencies {
                writeln!(&mut out, "    \"{dependency}\",").unwrap();
            }
            out.push_str("]\n");
        }
    }

    out
}

/// Append a file entry to a tar archive from a byte slice.
fn append_tar_file(tar: &mut tar::Builder<GzEncoder<Vec<u8>>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, data).unwrap();
}

/// Compute the SHA-256 hex digest of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_simple_wheel() {
        let (filename, bytes) = generate_wheel(
            "my-package",
            "1.0.0",
            &["dep>=1.0".to_string()],
            &BTreeMap::new(),
            Some(">=3.12"),
            "py3-none-any",
        );
        assert_eq!(filename, "my_package-1.0.0-py3-none-any.whl");

        // Verify it's a valid ZIP with expected files.
        let reader = Cursor::new(&bytes);
        let archive = zip::ZipArchive::new(reader).unwrap();
        let names: Vec<_> = archive.file_names().collect();
        assert!(names.contains(&"my_package/__init__.py"));
        assert!(names.contains(&"my_package-1.0.0.dist-info/METADATA"));
        assert!(names.contains(&"my_package-1.0.0.dist-info/WHEEL"));
    }

    #[test]
    fn generate_simple_sdist() {
        let (filename, bytes) = generate_sdist(
            "my-package",
            "1.0.0",
            &["dep>=1.0".to_string()],
            &BTreeMap::new(),
            Some(">=3.12"),
        );
        assert_eq!(filename, "my_package-1.0.0.tar.gz");

        // Verify it's a readable tar.gz with expected files.
        let decoder = flate2::read::GzDecoder::new(Cursor::new(&bytes));
        let mut archive = tar::Archive::new(decoder);
        let mut names = Vec::new();
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            names.push(entry.path().unwrap().to_string_lossy().to_string());
        }

        assert!(names.contains(&"my_package-1.0.0/pyproject.toml".to_string()));
        assert!(names.contains(&"my_package-1.0.0/PKG-INFO".to_string()));
        assert!(names.contains(&"my_package-1.0.0/src/my_package/__init__.py".to_string()));
    }

    #[test]
    fn extra_deps_with_markers_are_parenthesized() {
        let mut extras = BTreeMap::new();
        extras.insert(
            "binary".to_string(),
            vec!["dep-binary; implementation_name != 'pypy'".to_string()],
        );
        let metadata = build_metadata("pkg", "1.0.0", &[], &extras, None);
        assert!(
            metadata.contains(
                "Requires-Dist: dep-binary ; (implementation_name != 'pypy') and extra == \"binary\""
            ),
            "metadata should parenthesize existing markers:\n{metadata}"
        );
    }

    #[test]
    fn extra_deps_with_or_markers_preserve_precedence() {
        let mut extras = BTreeMap::new();
        extras.insert(
            "compat".to_string(),
            vec!["dep; sys_platform == 'linux' or sys_platform == 'darwin'".to_string()],
        );
        let metadata = build_metadata("pkg", "1.0.0", &[], &extras, None);
        assert!(
            metadata.contains(
                "Requires-Dist: dep ; (sys_platform == 'linux' or sys_platform == 'darwin') and extra == \"compat\""
            ),
            "metadata should parenthesize or-containing markers:\n{metadata}"
        );
    }
}
