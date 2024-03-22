#[derive(Debug, PartialEq)]
pub struct PythonDownloadMetadata {
    name: ImplementationName,
    arch: Arch,
    os: Os,
    libc: Libc,
    major: i64,
    minor: i64,
    patch: i64,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Arch {
    Arm64,
    I686,
    Ppc64Le,
    S390X,
    Windows,
    X86_64,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Libc {
    Gnu,
    Musl,
    None,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ImplementationName {
    Cpython,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Os {
    Darwin,
    Linux,
    Shared,
    Windows,
}
include!("python_versions.inc");

impl PythonDownloadMetadata {
    /// Return the [`PythonDownloadMetadata`] corresponding to the key, if it exists.
    pub fn from_key(key: &str) -> Option<&PythonDownloadMetadata> {
        for (ikey, value) in PYTHON_DOWNLOADS {
            if *ikey == key {
                return Some(value);
            }
        }
        None
    }
}
