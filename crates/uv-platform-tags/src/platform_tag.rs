/// A tag to represent the language and implementation of the Python interpreter.
///
/// This is the first segment in the wheel filename. For example, in `cp39-none-manylinux_2_24_x86_64.whl`,
/// the language tag is `cp39`.
#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub enum PlatformTag {
    /// Ex) `none`
    None,
    /// Ex) `py3`, `py39`
    Python { major: u8, minor: Option<u8> },
    /// Ex) `cp39`
    CPython { python_version: (u8, u8) },
    /// Ex) `pp39`
    PyPy { python_version: (u8, u8) },
    /// Ex) `graalpy310`
    GraalPy { python_version: (u8, u8) },
    /// Ex) `pt38`
    Pyston { python_version: (u8, u8) },
}
