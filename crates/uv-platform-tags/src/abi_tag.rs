use std::fmt::Formatter;
use std::str::FromStr;

/// A tag to represent the ABI compatibility of a Python distribution.
///
/// This is the second segment in the wheel filename, following the language tag. For example,
/// in `cp39-none-manylinux_2_24_x86_64.whl`, the ABI tag is `none`.
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
pub enum AbiTag {
    /// Ex) `none`
    None,
    /// Ex) `abi3`
    Abi3,
    /// Ex) `cp39m`, `cp310t`
    CPython {
        gil_disabled: bool,
        python_version: (u8, u8),
    },
    /// Ex) `pypy39_pp73`
    PyPy {
        python_version: Option<(u8, u8)>,
        implementation_version: (u8, u8),
    },
    /// Ex) `graalpy240_310_native`
    GraalPy {
        python_version: (u8, u8),
        implementation_version: (u8, u8),
    },
    /// Ex) `pyston_23_x86_64_linux_gnu`
    Pyston { implementation_version: (u8, u8) },
}

impl AbiTag {
    /// Return a pretty string representation of the ABI tag.
    pub fn pretty(self) -> Option<String> {
        match self {
            AbiTag::None => None,
            AbiTag::Abi3 => None,
            AbiTag::CPython { python_version, .. } => {
                Some(format!("CPython {}.{}", python_version.0, python_version.1))
            }
            AbiTag::PyPy {
                implementation_version,
                ..
            } => Some(format!(
                "PyPy {}.{}",
                implementation_version.0, implementation_version.1
            )),
            AbiTag::GraalPy {
                implementation_version,
                ..
            } => Some(format!(
                "GraalPy {}.{}",
                implementation_version.0, implementation_version.1
            )),
            AbiTag::Pyston { .. } => Some("Pyston".to_string()),
        }
    }
}

impl std::fmt::Display for AbiTag {
    /// Format an [`AbiTag`] as a string.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Abi3 => write!(f, "abi3"),
            Self::CPython {
                gil_disabled,
                python_version: (major, minor),
            } => {
                if *minor <= 7 {
                    write!(f, "cp{major}{minor}m")
                } else if *gil_disabled {
                    // https://peps.python.org/pep-0703/#build-configuration-changes
                    // Python 3.13+ only, but it makes more sense to just rely on the sysconfig var.
                    write!(f, "cp{major}{minor}t")
                } else {
                    write!(f, "cp{major}{minor}")
                }
            }
            Self::PyPy {
                python_version: Some((py_major, py_minor)),
                implementation_version: (impl_major, impl_minor),
            } => {
                write!(f, "pypy{py_major}{py_minor}_pp{impl_major}{impl_minor}")
            }
            Self::PyPy {
                python_version: None,
                implementation_version: (impl_major, impl_minor),
            } => {
                write!(f, "pypy_{impl_major}{impl_minor}")
            }
            Self::GraalPy {
                python_version: (py_major, py_minor),
                implementation_version: (impl_major, impl_minor),
            } => {
                write!(
                    f,
                    "graalpy{impl_major}{impl_minor}_{py_major}{py_minor}_native"
                )
            }
            Self::Pyston {
                implementation_version: (impl_major, impl_minor),
            } => {
                write!(f, "pyston_{impl_major}{impl_minor}_x86_64_linux_gnu")
            }
        }
    }
}

impl FromStr for AbiTag {
    type Err = ParseAbiTagError;

    /// Parse an [`AbiTag`] from a string.
    #[allow(clippy::cast_possible_truncation)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// Parse a Python version from a string (e.g., convert `39` into `(3, 9)`).
        fn parse_python_version(
            version_str: &str,
            implementation: &'static str,
            full_tag: &str,
        ) -> Result<(u8, u8), ParseAbiTagError> {
            let major = version_str
                .as_bytes()
                .first()
                .ok_or_else(|| ParseAbiTagError::MissingMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .checked_sub(b'0')
                .and_then(|d| if d < 10 { Some(d) } else { None })
                .ok_or_else(|| ParseAbiTagError::InvalidMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?;
            let minor = version_str
                .get(1..)
                .ok_or_else(|| ParseAbiTagError::MissingMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .parse::<u8>()
                .map_err(|_| ParseAbiTagError::InvalidMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?;
            Ok((major, minor))
        }

        /// Parse an implementation version from a string (e.g., convert `37` into `(3, 7)`).
        fn parse_impl_version(
            version_str: &str,
            implementation: &'static str,
            full_tag: &str,
        ) -> Result<(u8, u8), ParseAbiTagError> {
            let major = version_str
                .as_bytes()
                .first()
                .ok_or_else(|| ParseAbiTagError::MissingImplMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .checked_sub(b'0')
                .and_then(|d| if d < 10 { Some(d) } else { None })
                .ok_or_else(|| ParseAbiTagError::InvalidImplMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?;
            let minor = version_str
                .get(1..)
                .ok_or_else(|| ParseAbiTagError::MissingImplMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .parse::<u8>()
                .map_err(|_| ParseAbiTagError::InvalidImplMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?;
            Ok((major, minor))
        }

        if s == "none" {
            Ok(Self::None)
        } else if s == "abi3" {
            Ok(Self::Abi3)
        } else if let Some(cp) = s.strip_prefix("cp") {
            // Ex) `cp39m`, `cp310t`
            let version_end = cp.find(|c: char| !c.is_ascii_digit()).unwrap_or(cp.len());
            let version_str = &cp[..version_end];
            let (major, minor) = parse_python_version(version_str, "CPython", s)?;
            let gil_disabled = cp.ends_with('t');
            Ok(Self::CPython {
                gil_disabled,
                python_version: (major, minor),
            })
        } else if let Some(rest) = s.strip_prefix("pypy") {
            if let Some(rest) = rest.strip_prefix('_') {
                // Ex) `pypy_73`
                let (impl_major, impl_minor) = parse_impl_version(rest, "PyPy", s)?;
                Ok(Self::PyPy {
                    python_version: None,
                    implementation_version: (impl_major, impl_minor),
                })
            } else {
                // Ex) `pypy39_pp73`
                let (version_str, rest) =
                    rest.split_once('_')
                        .ok_or_else(|| ParseAbiTagError::InvalidFormat {
                            implementation: "PyPy",
                            tag: s.to_string(),
                        })?;
                let (major, minor) = parse_python_version(version_str, "PyPy", s)?;
                let rest =
                    rest.strip_prefix("pp")
                        .ok_or_else(|| ParseAbiTagError::InvalidFormat {
                            implementation: "PyPy",
                            tag: s.to_string(),
                        })?;
                let (impl_major, impl_minor) = parse_impl_version(rest, "PyPy", s)?;
                Ok(Self::PyPy {
                    python_version: Some((major, minor)),
                    implementation_version: (impl_major, impl_minor),
                })
            }
        } else if let Some(rest) = s.strip_prefix("graalpy") {
            // Ex) `graalpy240_310_native`
            let (impl_ver_str, rest) =
                rest.split_once('_')
                    .ok_or_else(|| ParseAbiTagError::InvalidFormat {
                        implementation: "GraalPy",
                        tag: s.to_string(),
                    })?;
            let (impl_major, impl_minor) = parse_impl_version(impl_ver_str, "GraalPy", s)?;
            let (py_ver_str, _) =
                rest.split_once('_')
                    .ok_or_else(|| ParseAbiTagError::InvalidFormat {
                        implementation: "GraalPy",
                        tag: s.to_string(),
                    })?;
            let (major, minor) = parse_python_version(py_ver_str, "GraalPy", s)?;
            Ok(Self::GraalPy {
                python_version: (major, minor),
                implementation_version: (impl_major, impl_minor),
            })
        } else if let Some(rest) = s.strip_prefix("pyston") {
            // Ex) `pyston_23_x86_64_linux_gnu`
            let rest = rest
                .strip_prefix("_")
                .ok_or_else(|| ParseAbiTagError::InvalidFormat {
                    implementation: "Pyston",
                    tag: s.to_string(),
                })?;
            let rest = rest.strip_suffix("_x86_64_linux_gnu").ok_or_else(|| {
                ParseAbiTagError::InvalidFormat {
                    implementation: "Pyston",
                    tag: s.to_string(),
                }
            })?;
            let (impl_major, impl_minor) = parse_impl_version(rest, "Pyston", s)?;
            Ok(Self::Pyston {
                implementation_version: (impl_major, impl_minor),
            })
        } else {
            Err(ParseAbiTagError::UnknownFormat(s.to_string()))
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseAbiTagError {
    #[error("Unknown ABI tag format: {0}")]
    UnknownFormat(String),
    #[error("Missing major version in {implementation} ABI tag: {tag}")]
    MissingMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid major version in {implementation} ABI tag: {tag}")]
    InvalidMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Missing minor version in {implementation} ABI tag: {tag}")]
    MissingMinorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid minor version in {implementation} ABI tag: {tag}")]
    InvalidMinorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid {implementation} ABI tag format: {tag}")]
    InvalidFormat {
        implementation: &'static str,
        tag: String,
    },
    #[error("Missing implementation major version in {implementation} ABI tag: {tag}")]
    MissingImplMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid implementation major version in {implementation} ABI tag: {tag}")]
    InvalidImplMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Missing implementation minor version in {implementation} ABI tag: {tag}")]
    MissingImplMinorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid implementation minor version in {implementation} ABI tag: {tag}")]
    InvalidImplMinorVersion {
        implementation: &'static str,
        tag: String,
    },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::abi_tag::{AbiTag, ParseAbiTagError};

    #[test]
    fn none_abi() {
        assert_eq!(AbiTag::from_str("none"), Ok(AbiTag::None));
        assert_eq!(AbiTag::None.to_string(), "none");
    }

    #[test]
    fn abi3() {
        assert_eq!(AbiTag::from_str("abi3"), Ok(AbiTag::Abi3));
        assert_eq!(AbiTag::Abi3.to_string(), "abi3");
    }

    #[test]
    fn cpython_abi() {
        let tag = AbiTag::CPython {
            gil_disabled: false,
            python_version: (3, 9),
        };
        assert_eq!(AbiTag::from_str("cp39"), Ok(tag));
        assert_eq!(tag.to_string(), "cp39");

        let tag = AbiTag::CPython {
            gil_disabled: false,
            python_version: (3, 7),
        };
        assert_eq!(AbiTag::from_str("cp37m"), Ok(tag));
        assert_eq!(tag.to_string(), "cp37m");

        let tag = AbiTag::CPython {
            gil_disabled: true,
            python_version: (3, 13),
        };
        assert_eq!(AbiTag::from_str("cp313t"), Ok(tag));
        assert_eq!(tag.to_string(), "cp313t");

        assert_eq!(
            AbiTag::from_str("cpXY"),
            Err(ParseAbiTagError::MissingMajorVersion {
                implementation: "CPython",
                tag: "cpXY".to_string()
            })
        );
    }

    #[test]
    fn pypy_abi() {
        let tag = AbiTag::PyPy {
            python_version: Some((3, 9)),
            implementation_version: (7, 3),
        };
        assert_eq!(AbiTag::from_str("pypy39_pp73"), Ok(tag));
        assert_eq!(tag.to_string(), "pypy39_pp73");

        let tag = AbiTag::PyPy {
            python_version: None,
            implementation_version: (7, 3),
        };
        assert_eq!(AbiTag::from_str("pypy_73").as_ref(), Ok(&tag));
        assert_eq!(tag.to_string(), "pypy_73");

        assert_eq!(
            AbiTag::from_str("pypy39"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "PyPy",
                tag: "pypy39".to_string()
            })
        );
        assert_eq!(
            AbiTag::from_str("pypy39_73"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "PyPy",
                tag: "pypy39_73".to_string()
            })
        );
        assert_eq!(
            AbiTag::from_str("pypy39_ppXY"),
            Err(ParseAbiTagError::InvalidImplMajorVersion {
                implementation: "PyPy",
                tag: "pypy39_ppXY".to_string()
            })
        );
    }

    #[test]
    fn graalpy_abi() {
        let tag = AbiTag::GraalPy {
            python_version: (3, 10),
            implementation_version: (2, 40),
        };
        assert_eq!(AbiTag::from_str("graalpy240_310_native"), Ok(tag));
        assert_eq!(tag.to_string(), "graalpy240_310_native");

        assert_eq!(
            AbiTag::from_str("graalpy310"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "GraalPy",
                tag: "graalpy310".to_string()
            })
        );
        assert_eq!(
            AbiTag::from_str("graalpy310_240"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "GraalPy",
                tag: "graalpy310_240".to_string()
            })
        );
        assert_eq!(
            AbiTag::from_str("graalpy310_graalpyXY"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "GraalPy",
                tag: "graalpy310_graalpyXY".to_string()
            })
        );
    }

    #[test]
    fn pyston_abi() {
        let tag = AbiTag::Pyston {
            implementation_version: (2, 3),
        };
        assert_eq!(AbiTag::from_str("pyston_23_x86_64_linux_gnu"), Ok(tag));
        assert_eq!(tag.to_string(), "pyston_23_x86_64_linux_gnu");

        assert_eq!(
            AbiTag::from_str("pyston23_x86_64_linux_gnu"),
            Err(ParseAbiTagError::InvalidFormat {
                implementation: "Pyston",
                tag: "pyston23_x86_64_linux_gnu".to_string()
            })
        );
        assert_eq!(
            AbiTag::from_str("pyston_XY_x86_64_linux_gnu"),
            Err(ParseAbiTagError::InvalidImplMajorVersion {
                implementation: "Pyston",
                tag: "pyston_XY_x86_64_linux_gnu".to_string()
            })
        );
    }

    #[test]
    fn unknown_abi() {
        assert_eq!(
            AbiTag::from_str("unknown"),
            Err(ParseAbiTagError::UnknownFormat("unknown".to_string()))
        );
        assert_eq!(
            AbiTag::from_str(""),
            Err(ParseAbiTagError::UnknownFormat(String::new()))
        );
    }
}
