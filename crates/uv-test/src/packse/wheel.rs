//! Generate minimal Python wheels and source distributions in memory.
//!
//! Packse scenario packages are trivial: they contain only metadata and a stub
//! `__init__.py`. We generate them directly without invoking a Python build backend.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::GzEncoder;
use indoc::formatdoc;
use sha2::{Digest, Sha256};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Generate a wheel (`.whl`) as an in-memory ZIP archive.
///
/// Returns `(filename, bytes)`.
pub fn generate_wheel(
    name: &PackageName,
    version: &Version,
    requires: &[Requirement],
    extras: &BTreeMap<ExtraName, Vec<Requirement>>,
    requires_python: Option<&VersionSpecifiers>,
    tag: &str,
) -> (String, Vec<u8>) {
    let normalized = name.as_dist_info_name();
    let dist_info = format!("{normalized}-{version}.dist-info");

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let init_py = format!("__version__ = \"{version}\"\n");
    zip.start_file(format!("{normalized}/__init__.py"), opts)
        .expect("failed to start wheel module file");
    zip.write_all(init_py.as_bytes())
        .expect("failed to write wheel module file");

    let metadata = build_metadata(name, version, requires, extras, requires_python);
    zip.start_file(format!("{dist_info}/METADATA"), opts)
        .expect("failed to start wheel metadata file");
    zip.write_all(metadata.as_bytes())
        .expect("failed to write wheel metadata file");

    let wheel_info = format!(
        "Wheel-Version: 1.0\n\
         Generator: uv-test\n\
         Root-Is-Purelib: true\n\
         Tag: {tag}\n"
    );
    zip.start_file(format!("{dist_info}/WHEEL"), opts)
        .expect("failed to start WHEEL metadata file");
    zip.write_all(wheel_info.as_bytes())
        .expect("failed to write WHEEL metadata file");

    zip.start_file(format!("{dist_info}/RECORD"), opts)
        .expect("failed to start RECORD file");
    zip.write_all(b"").expect("failed to write RECORD file");

    let bytes = zip
        .finish()
        .expect("failed to finish in-memory wheel")
        .into_inner();
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
    name: &PackageName,
    version: &Version,
    requires: &[Requirement],
    extras: &BTreeMap<ExtraName, Vec<Requirement>>,
    requires_python: Option<&VersionSpecifiers>,
) -> (String, Vec<u8>) {
    let normalized = name.as_dist_info_name();
    let prefix = format!("{normalized}-{version}");

    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut tar = tar::Builder::new(encoder);

    let pyproject = build_pyproject_toml(name, version, requires, extras, requires_python);
    append_tar_file(
        &mut tar,
        &format!("{prefix}/pyproject.toml"),
        pyproject.as_bytes(),
    );

    let pkg_info = build_metadata(name, version, requires, extras, requires_python);
    append_tar_file(&mut tar, &format!("{prefix}/PKG-INFO"), pkg_info.as_bytes());

    let init_py = format!("__version__ = \"{version}\"\n");
    append_tar_file(
        &mut tar,
        &format!("{prefix}/src/{normalized}/__init__.py"),
        init_py.as_bytes(),
    );

    let encoder = tar
        .into_inner()
        .expect("failed to finish in-memory source archive");
    let bytes = encoder
        .finish()
        .expect("failed to finish in-memory gzip stream");
    let filename = format!("{normalized}-{version}.tar.gz");
    (filename, bytes)
}

/// Build PEP 566 / PEP 643 metadata content.
fn build_metadata(
    name: &PackageName,
    version: &Version,
    requires: &[Requirement],
    extras: &BTreeMap<ExtraName, Vec<Requirement>>,
    requires_python: Option<&VersionSpecifiers>,
) -> String {
    let mut metadata = String::new();
    writeln!(&mut metadata, "Metadata-Version: 2.3")
        .expect("writing metadata into a string should succeed");
    writeln!(&mut metadata, "Name: {name}").expect("writing metadata into a string should succeed");
    writeln!(&mut metadata, "Version: {version}")
        .expect("writing metadata into a string should succeed");
    if let Some(requires_python) = requires_python {
        writeln!(&mut metadata, "Requires-Python: {requires_python}")
            .expect("writing metadata into a string should succeed");
    }

    for extra_name in extras.keys() {
        writeln!(&mut metadata, "Provides-Extra: {extra_name}")
            .expect("writing metadata into a string should succeed");
    }

    for requirement in requires {
        writeln!(&mut metadata, "Requires-Dist: {requirement}")
            .expect("writing metadata into a string should succeed");
    }
    for (extra_name, extra_requirements) in extras {
        for requirement in extra_requirements {
            let requirement = requirement.clone().with_extra_marker(extra_name);
            writeln!(&mut metadata, "Requires-Dist: {requirement}")
                .expect("writing metadata into a string should succeed");
        }
    }

    metadata
}

/// Build a minimal `pyproject.toml` for an sdist using hatchling.
fn build_pyproject_toml(
    name: &PackageName,
    version: &Version,
    requires: &[Requirement],
    extras: &BTreeMap<ExtraName, Vec<Requirement>>,
    requires_python: Option<&VersionSpecifiers>,
) -> String {
    let normalized = name.as_dist_info_name();
    let dependencies = if requires.is_empty() {
        "dependencies = []\n".to_string()
    } else {
        let mut dependencies = String::from("dependencies = [\n");
        for requirement in requires {
            writeln!(&mut dependencies, "    \"{requirement}\",")
                .expect("writing dependencies into a string should succeed");
        }
        dependencies.push_str("]\n");
        dependencies
    };
    let requires_python = requires_python
        .map(|requires_python| format!("requires-python = \"{requires_python}\"\n"))
        .unwrap_or_default();
    let optional_dependencies = if extras.is_empty() {
        String::new()
    } else {
        let mut optional_dependencies = String::from("\n[project.optional-dependencies]\n");
        for (extra_name, extra_requirements) in extras {
            writeln!(&mut optional_dependencies, "{extra_name} = [")
                .expect("writing optional dependencies into a string should succeed");
            for requirement in extra_requirements {
                writeln!(&mut optional_dependencies, "    \"{requirement}\",")
                    .expect("writing optional dependencies into a string should succeed");
            }
            optional_dependencies.push_str("]\n");
        }
        optional_dependencies
    };

    formatdoc! {
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
        {dependencies}{requires_python}{optional_dependencies}
        "#
    }
}

/// Append a file entry to a tar archive from a byte slice.
fn append_tar_file(tar: &mut tar::Builder<GzEncoder<Vec<u8>>>, path: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, data)
        .expect("failed to append file to in-memory source archive");
}

/// Compute the SHA-256 hex digest of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn generate_simple_wheel() {
        let requires = vec![Requirement::from_str("dep>=1.0").expect("valid requirement")];
        let requires_python =
            VersionSpecifiers::from_str(">=3.12").expect("valid version specifier");
        let (filename, bytes) = generate_wheel(
            &PackageName::from_str("my-package").expect("valid package name"),
            &Version::from_str("1.0.0").expect("valid version"),
            &requires,
            &BTreeMap::new(),
            Some(&requires_python),
            "py3-none-any",
        );
        assert_eq!(filename, "my_package-1.0.0-py3-none-any.whl");

        let reader = Cursor::new(&bytes);
        let archive = zip::ZipArchive::new(reader).expect("wheel should be a valid zip");
        let names: Vec<_> = archive.file_names().collect();
        assert!(names.contains(&"my_package/__init__.py"));
        assert!(names.contains(&"my_package-1.0.0.dist-info/METADATA"));
        assert!(names.contains(&"my_package-1.0.0.dist-info/WHEEL"));
    }

    #[test]
    fn generate_simple_sdist() {
        let requires = vec![Requirement::from_str("dep>=1.0").expect("valid requirement")];
        let requires_python =
            VersionSpecifiers::from_str(">=3.12").expect("valid version specifier");
        let (filename, bytes) = generate_sdist(
            &PackageName::from_str("my-package").expect("valid package name"),
            &Version::from_str("1.0.0").expect("valid version"),
            &requires,
            &BTreeMap::new(),
            Some(&requires_python),
        );
        assert_eq!(filename, "my_package-1.0.0.tar.gz");

        let decoder = flate2::read::GzDecoder::new(Cursor::new(&bytes));
        let mut archive = tar::Archive::new(decoder);
        let mut names = Vec::new();
        for entry in archive.entries().expect("sdist archive should be readable") {
            let entry = entry.expect("sdist archive entry should be readable");
            names.push(
                entry
                    .path()
                    .expect("sdist archive entry should have a path")
                    .to_string_lossy()
                    .to_string(),
            );
        }

        assert!(names.contains(&"my_package-1.0.0/pyproject.toml".to_string()));
        assert!(names.contains(&"my_package-1.0.0/PKG-INFO".to_string()));
        assert!(names.contains(&"my_package-1.0.0/src/my_package/__init__.py".to_string()));
    }

    #[test]
    fn extra_deps_with_markers_include_the_extra_marker() {
        let mut extras = BTreeMap::new();
        extras.insert(
            ExtraName::from_str("binary").expect("valid extra name"),
            vec![
                Requirement::from_str("dep-binary; implementation_name != 'pypy'")
                    .expect("valid requirement"),
            ],
        );
        let metadata = build_metadata(
            &PackageName::from_str("pkg").expect("valid package name"),
            &Version::from_str("1.0.0").expect("valid version"),
            &[],
            &extras,
            None,
        );
        assert!(
            metadata.contains(
                "Requires-Dist: dep-binary ; implementation_name != 'pypy' and extra == 'binary'"
            ),
            "metadata should retain the existing marker and add the extra marker:\n{metadata}"
        );
    }

    #[test]
    fn extra_deps_with_or_markers_preserve_precedence() {
        let mut extras = BTreeMap::new();
        extras.insert(
            ExtraName::from_str("compat").expect("valid extra name"),
            vec![
                Requirement::from_str("dep; sys_platform == 'linux' or sys_platform == 'darwin'")
                    .expect("valid requirement"),
            ],
        );
        let metadata = build_metadata(
            &PackageName::from_str("pkg").expect("valid package name"),
            &Version::from_str("1.0.0").expect("valid version"),
            &[],
            &extras,
            None,
        );
        assert!(
            metadata.contains(
                "Requires-Dist: dep ; (sys_platform == 'darwin' and extra == 'compat') or (sys_platform == 'linux' and extra == 'compat')"
            ),
            "metadata should preserve the original or-marker semantics:\n{metadata}"
        );
    }
}
