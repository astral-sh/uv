use std::{path::PathBuf, str::FromStr};

use assert_fs::{prelude::*, TempDir};
use test_log::test;
use uv_pep440::{Prerelease, PrereleaseKind, VersionSpecifiers};

use crate::{
    discovery::{PythonRequest, VersionRequest},
    implementation::ImplementationName,
};

use super::{Error, PythonVariant};

#[test]
fn interpreter_request_from_str() {
    assert_eq!(PythonRequest::parse("any"), PythonRequest::Any);
    assert_eq!(PythonRequest::parse("default"), PythonRequest::Default);
    assert_eq!(
        PythonRequest::parse("3.12"),
        PythonRequest::Version(VersionRequest::from_str("3.12").unwrap())
    );
    assert_eq!(
        PythonRequest::parse(">=3.12"),
        PythonRequest::Version(VersionRequest::from_str(">=3.12").unwrap())
    );
    assert_eq!(
        PythonRequest::parse(">=3.12,<3.13"),
        PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
    );
    assert_eq!(
        PythonRequest::parse(">=3.12,<3.13"),
        PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
    );

    assert_eq!(
        PythonRequest::parse("3.13.0a1"),
        PythonRequest::Version(VersionRequest::from_str("3.13.0a1").unwrap())
    );
    assert_eq!(
        PythonRequest::parse("3.13.0b5"),
        PythonRequest::Version(VersionRequest::from_str("3.13.0b5").unwrap())
    );
    assert_eq!(
        PythonRequest::parse("3.13.0rc1"),
        PythonRequest::Version(VersionRequest::from_str("3.13.0rc1").unwrap())
    );
    assert_eq!(
        PythonRequest::parse("3.13.1rc1"),
        PythonRequest::ExecutableName("3.13.1rc1".to_string()),
        "Pre-release version requests require a patch version of zero"
    );
    assert_eq!(
        PythonRequest::parse("3rc1"),
        PythonRequest::ExecutableName("3rc1".to_string()),
        "Pre-release version requests require a minor version"
    );

    assert_eq!(
        PythonRequest::parse("cpython"),
        PythonRequest::Implementation(ImplementationName::CPython)
    );
    assert_eq!(
        PythonRequest::parse("cpython3.12.2"),
        PythonRequest::ImplementationVersion(
            ImplementationName::CPython,
            VersionRequest::from_str("3.12.2").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("pypy"),
        PythonRequest::Implementation(ImplementationName::PyPy)
    );
    assert_eq!(
        PythonRequest::parse("pp"),
        PythonRequest::Implementation(ImplementationName::PyPy)
    );
    assert_eq!(
        PythonRequest::parse("graalpy"),
        PythonRequest::Implementation(ImplementationName::GraalPy)
    );
    assert_eq!(
        PythonRequest::parse("gp"),
        PythonRequest::Implementation(ImplementationName::GraalPy)
    );
    assert_eq!(
        PythonRequest::parse("cp"),
        PythonRequest::Implementation(ImplementationName::CPython)
    );
    assert_eq!(
        PythonRequest::parse("pypy3.10"),
        PythonRequest::ImplementationVersion(
            ImplementationName::PyPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("pp310"),
        PythonRequest::ImplementationVersion(
            ImplementationName::PyPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("graalpy3.10"),
        PythonRequest::ImplementationVersion(
            ImplementationName::GraalPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("gp310"),
        PythonRequest::ImplementationVersion(
            ImplementationName::GraalPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("cp38"),
        PythonRequest::ImplementationVersion(
            ImplementationName::CPython,
            VersionRequest::from_str("3.8").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("pypy@3.10"),
        PythonRequest::ImplementationVersion(
            ImplementationName::PyPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("pypy310"),
        PythonRequest::ImplementationVersion(
            ImplementationName::PyPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("graalpy@3.10"),
        PythonRequest::ImplementationVersion(
            ImplementationName::GraalPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );
    assert_eq!(
        PythonRequest::parse("graalpy310"),
        PythonRequest::ImplementationVersion(
            ImplementationName::GraalPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
    );

    let tempdir = TempDir::new().unwrap();
    assert_eq!(
        PythonRequest::parse(tempdir.path().to_str().unwrap()),
        PythonRequest::Directory(tempdir.path().to_path_buf()),
        "An existing directory is treated as a directory"
    );
    assert_eq!(
        PythonRequest::parse(tempdir.child("foo").path().to_str().unwrap()),
        PythonRequest::File(tempdir.child("foo").path().to_path_buf()),
        "A path that does not exist is treated as a file"
    );
    tempdir.child("bar").touch().unwrap();
    assert_eq!(
        PythonRequest::parse(tempdir.child("bar").path().to_str().unwrap()),
        PythonRequest::File(tempdir.child("bar").path().to_path_buf()),
        "An existing file is treated as a file"
    );
    assert_eq!(
        PythonRequest::parse("./foo"),
        PythonRequest::File(PathBuf::from_str("./foo").unwrap()),
        "A string with a file system separator is treated as a file"
    );
    assert_eq!(
        PythonRequest::parse("3.13t"),
        PythonRequest::Version(VersionRequest::from_str("3.13t").unwrap())
    );
}

#[test]
fn interpreter_request_to_canonical_string() {
    assert_eq!(PythonRequest::Default.to_canonical_string(), "default");
    assert_eq!(PythonRequest::Any.to_canonical_string(), "any");
    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str("3.12").unwrap()).to_canonical_string(),
        "3.12"
    );
    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str(">=3.12").unwrap()).to_canonical_string(),
        ">=3.12"
    );
    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str(">=3.12,<3.13").unwrap())
            .to_canonical_string(),
        ">=3.12, <3.13"
    );

    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str("3.13.0a1").unwrap()).to_canonical_string(),
        "3.13a1"
    );

    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str("3.13.0b5").unwrap()).to_canonical_string(),
        "3.13b5"
    );

    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str("3.13.0rc1").unwrap())
            .to_canonical_string(),
        "3.13rc1"
    );

    assert_eq!(
        PythonRequest::Version(VersionRequest::from_str("313rc4").unwrap()).to_canonical_string(),
        "3.13rc4"
    );

    assert_eq!(
        PythonRequest::ExecutableName("foo".to_string()).to_canonical_string(),
        "foo"
    );
    assert_eq!(
        PythonRequest::Implementation(ImplementationName::CPython).to_canonical_string(),
        "cpython"
    );
    assert_eq!(
        PythonRequest::ImplementationVersion(
            ImplementationName::CPython,
            VersionRequest::from_str("3.12.2").unwrap(),
        )
        .to_canonical_string(),
        "cpython@3.12.2"
    );
    assert_eq!(
        PythonRequest::Implementation(ImplementationName::PyPy).to_canonical_string(),
        "pypy"
    );
    assert_eq!(
        PythonRequest::ImplementationVersion(
            ImplementationName::PyPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
        .to_canonical_string(),
        "pypy@3.10"
    );
    assert_eq!(
        PythonRequest::Implementation(ImplementationName::GraalPy).to_canonical_string(),
        "graalpy"
    );
    assert_eq!(
        PythonRequest::ImplementationVersion(
            ImplementationName::GraalPy,
            VersionRequest::from_str("3.10").unwrap(),
        )
        .to_canonical_string(),
        "graalpy@3.10"
    );

    let tempdir = TempDir::new().unwrap();
    assert_eq!(
        PythonRequest::Directory(tempdir.path().to_path_buf()).to_canonical_string(),
        tempdir.path().to_str().unwrap(),
        "An existing directory is treated as a directory"
    );
    assert_eq!(
        PythonRequest::File(tempdir.child("foo").path().to_path_buf()).to_canonical_string(),
        tempdir.child("foo").path().to_str().unwrap(),
        "A path that does not exist is treated as a file"
    );
    tempdir.child("bar").touch().unwrap();
    assert_eq!(
        PythonRequest::File(tempdir.child("bar").path().to_path_buf()).to_canonical_string(),
        tempdir.child("bar").path().to_str().unwrap(),
        "An existing file is treated as a file"
    );
    assert_eq!(
        PythonRequest::File(PathBuf::from_str("./foo").unwrap()).to_canonical_string(),
        "./foo",
        "A string with a file system separator is treated as a file"
    );
}

#[test]
fn version_request_from_str() {
    assert_eq!(
        VersionRequest::from_str("3").unwrap(),
        VersionRequest::Major(3, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("3.12").unwrap(),
        VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("3.12.1").unwrap(),
        VersionRequest::MajorMinorPatch(3, 12, 1, PythonVariant::Default)
    );
    assert!(VersionRequest::from_str("1.foo.1").is_err());
    assert_eq!(
        VersionRequest::from_str("3").unwrap(),
        VersionRequest::Major(3, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("38").unwrap(),
        VersionRequest::MajorMinor(3, 8, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("312").unwrap(),
        VersionRequest::MajorMinor(3, 12, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("3100").unwrap(),
        VersionRequest::MajorMinor(3, 100, PythonVariant::Default)
    );
    assert_eq!(
        VersionRequest::from_str("3.13a1").unwrap(),
        VersionRequest::MajorMinorPrerelease(
            3,
            13,
            Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1
            },
            PythonVariant::Default
        )
    );
    assert_eq!(
        VersionRequest::from_str("313b1").unwrap(),
        VersionRequest::MajorMinorPrerelease(
            3,
            13,
            Prerelease {
                kind: PrereleaseKind::Beta,
                number: 1
            },
            PythonVariant::Default
        )
    );
    assert_eq!(
        VersionRequest::from_str("3.13.0b2").unwrap(),
        VersionRequest::MajorMinorPrerelease(
            3,
            13,
            Prerelease {
                kind: PrereleaseKind::Beta,
                number: 2
            },
            PythonVariant::Default
        )
    );
    assert_eq!(
        VersionRequest::from_str("3.13.0rc3").unwrap(),
        VersionRequest::MajorMinorPrerelease(
            3,
            13,
            Prerelease {
                kind: PrereleaseKind::Rc,
                number: 3
            },
            PythonVariant::Default
        )
    );
    assert!(
        matches!(
            VersionRequest::from_str("3rc1"),
            Err(Error::InvalidVersionRequest(_))
        ),
        "Pre-release version requests require a minor version"
    );
    assert!(
        matches!(
            VersionRequest::from_str("3.13.2rc1"),
            Err(Error::InvalidVersionRequest(_))
        ),
        "Pre-release version requests require a patch version of zero"
    );
    assert!(
        matches!(
            VersionRequest::from_str("3.12-dev"),
            Err(Error::InvalidVersionRequest(_))
        ),
        "Development version segments are not allowed"
    );
    assert!(
        matches!(
            VersionRequest::from_str("3.12+local"),
            Err(Error::InvalidVersionRequest(_))
        ),
        "Local version segments are not allowed"
    );
    assert!(
        matches!(
            VersionRequest::from_str("3.12.post0"),
            Err(Error::InvalidVersionRequest(_))
        ),
        "Post version segments are not allowed"
    );
    assert!(
        // Test for overflow
        matches!(
            VersionRequest::from_str("31000"),
            Err(Error::InvalidVersionRequest(_))
        )
    );
    assert_eq!(
        VersionRequest::from_str("3t").unwrap(),
        VersionRequest::Major(3, PythonVariant::Freethreaded)
    );
    assert_eq!(
        VersionRequest::from_str("313t").unwrap(),
        VersionRequest::MajorMinor(3, 13, PythonVariant::Freethreaded)
    );
    assert_eq!(
        VersionRequest::from_str("3.13t").unwrap(),
        VersionRequest::MajorMinor(3, 13, PythonVariant::Freethreaded)
    );
    assert_eq!(
        VersionRequest::from_str(">=3.13t").unwrap(),
        VersionRequest::Range(
            VersionSpecifiers::from_str(">=3.13").unwrap(),
            PythonVariant::Freethreaded
        )
    );
    assert_eq!(
        VersionRequest::from_str(">=3.13").unwrap(),
        VersionRequest::Range(
            VersionSpecifiers::from_str(">=3.13").unwrap(),
            PythonVariant::Default
        )
    );
    assert_eq!(
        VersionRequest::from_str(">=3.12,<3.14t").unwrap(),
        VersionRequest::Range(
            VersionSpecifiers::from_str(">=3.12,<3.14").unwrap(),
            PythonVariant::Freethreaded
        )
    );
    assert!(matches!(
        VersionRequest::from_str("3.13tt"),
        Err(Error::InvalidVersionRequest(_))
    ));
}

#[test]
fn executable_names_from_request() {
    fn case(request: &str, expected: &[&str]) {
        let (implementation, version) = match PythonRequest::parse(request) {
            PythonRequest::Any => (None, VersionRequest::Any),
            PythonRequest::Default => (None, VersionRequest::Default),
            PythonRequest::Version(version) => (None, version),
            PythonRequest::ImplementationVersion(implementation, version) => {
                (Some(implementation), version)
            }
            PythonRequest::Implementation(implementation) => {
                (Some(implementation), VersionRequest::Default)
            }
            result => {
                panic!("Test cases should request versions or implementations; got {result:?}")
            }
        };

        let result: Vec<_> = version
            .executable_names(implementation.as_ref())
            .into_iter()
            .map(|name| name.to_string())
            .collect();

        let expected: Vec<_> = expected
            .iter()
            .map(|name| format!("{name}{exe}", exe = std::env::consts::EXE_SUFFIX))
            .collect();

        assert_eq!(result, expected, "mismatch for case \"{request}\"");
    }

    case(
        "any",
        &[
            "python", "python3", "cpython", "pypy", "graalpy", "cpython3", "pypy3", "graalpy3",
        ],
    );

    case("default", &["python", "python3"]);

    case("3", &["python", "python3"]);

    case("4", &["python", "python4"]);

    case("3.13", &["python", "python3", "python3.13"]);

    case(
        "pypy@3.10",
        &[
            "python",
            "python3",
            "python3.10",
            "pypy",
            "pypy3",
            "pypy3.10",
        ],
    );

    case(
        "3.13t",
        &[
            "python",
            "python3",
            "python3.13",
            "pythont",
            "python3t",
            "python3.13t",
        ],
    );

    case(
        "3.13.2",
        &["python", "python3", "python3.13", "python3.13.2"],
    );

    case(
        "3.13rc2",
        &["python", "python3", "python3.13", "python3.13rc2"],
    );
}
