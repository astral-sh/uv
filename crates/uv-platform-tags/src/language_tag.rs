use std::fmt::Formatter;
use std::str::FromStr;

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
pub enum LanguageTag {
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
    /// Ex) `pyston38`
    Pyston { python_version: (u8, u8) },
}

impl LanguageTag {
    /// Return a pretty string representation of the language tag.
    pub fn pretty(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Python { major, minor } => {
                if let Some(minor) = minor {
                    Some(format!("Python {major}.{minor}"))
                } else {
                    Some(format!("Python {major}"))
                }
            }
            Self::CPython {
                python_version: (major, minor),
            } => Some(format!("CPython {major}.{minor}")),
            Self::PyPy {
                python_version: (major, minor),
            } => Some(format!("PyPy {major}.{minor}")),
            Self::GraalPy {
                python_version: (major, minor),
            } => Some(format!("GraalPy {major}.{minor}")),
            Self::Pyston {
                python_version: (major, minor),
            } => Some(format!("Pyston {major}.{minor}")),
        }
    }
}

impl std::fmt::Display for LanguageTag {
    /// Format a [`LanguageTag`] as a string.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Python { major, minor } => {
                if let Some(minor) = minor {
                    write!(f, "py{major}{minor}")
                } else {
                    write!(f, "py{major}")
                }
            }
            Self::CPython {
                python_version: (major, minor),
            } => {
                write!(f, "cp{major}{minor}")
            }
            Self::PyPy {
                python_version: (major, minor),
            } => {
                write!(f, "pp{major}{minor}")
            }
            Self::GraalPy {
                python_version: (major, minor),
            } => {
                write!(f, "graalpy{major}{minor}")
            }
            Self::Pyston {
                python_version: (major, minor),
            } => {
                write!(f, "pyston{major}{minor}")
            }
        }
    }
}

impl FromStr for LanguageTag {
    type Err = ParseLanguageTagError;

    /// Parse a [`LanguageTag`] from a string.
    #[allow(clippy::cast_possible_truncation)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// Parse a Python version from a string (e.g., convert `39` into `(3, 9)`).
        fn parse_python_version(
            version_str: &str,
            implementation: &'static str,
            full_tag: &str,
        ) -> Result<(u8, u8), ParseLanguageTagError> {
            let major = version_str
                .chars()
                .next()
                .ok_or_else(|| ParseLanguageTagError::MissingMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .to_digit(10)
                .ok_or_else(|| ParseLanguageTagError::InvalidMajorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })? as u8;
            let minor = version_str
                .get(1..)
                .ok_or_else(|| ParseLanguageTagError::MissingMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?
                .parse::<u8>()
                .map_err(|_| ParseLanguageTagError::InvalidMinorVersion {
                    implementation,
                    tag: full_tag.to_string(),
                })?;
            Ok((major, minor))
        }

        if s == "none" {
            Ok(Self::None)
        } else if let Some(py) = s.strip_prefix("py") {
            match py.len() {
                0 => {
                    return Err(ParseLanguageTagError::MissingMajorVersion {
                        implementation: "Python",
                        tag: s.to_string(),
                    });
                }
                1 => {
                    // Ex) `py3`
                    let major = py
                        .chars()
                        .next()
                        .ok_or_else(|| ParseLanguageTagError::MissingMajorVersion {
                            implementation: "Python",
                            tag: s.to_string(),
                        })?
                        .to_digit(10)
                        .ok_or_else(|| ParseLanguageTagError::InvalidMajorVersion {
                            implementation: "Python",
                            tag: s.to_string(),
                        })? as u8;
                    Ok(Self::Python { major, minor: None })
                }
                2 | 3 => {
                    // Ex) `py39`, `py310`
                    let (major, minor) = parse_python_version(py, "Python", s)?;
                    Ok(Self::Python {
                        major,
                        minor: Some(minor),
                    })
                }
                _ => {
                    if let Some(pyston) = py.strip_prefix("ston") {
                        // Ex) `pyston38`
                        let (major, minor) = parse_python_version(pyston, "Pyston", s)?;
                        Ok(Self::Pyston {
                            python_version: (major, minor),
                        })
                    } else {
                        Err(ParseLanguageTagError::UnknownFormat(s.to_string()))
                    }
                }
            }
        } else if let Some(cp) = s.strip_prefix("cp") {
            // Ex) `cp39`
            let (major, minor) = parse_python_version(cp, "CPython", s)?;
            Ok(Self::CPython {
                python_version: (major, minor),
            })
        } else if let Some(pp) = s.strip_prefix("pp") {
            // Ex) `pp39`
            let (major, minor) = parse_python_version(pp, "PyPy", s)?;
            Ok(Self::PyPy {
                python_version: (major, minor),
            })
        } else if let Some(graalpy) = s.strip_prefix("graalpy") {
            // Ex) `graalpy310`
            let (major, minor) = parse_python_version(graalpy, "GraalPy", s)?;
            Ok(Self::GraalPy {
                python_version: (major, minor),
            })
        } else {
            Err(ParseLanguageTagError::UnknownFormat(s.to_string()))
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseLanguageTagError {
    #[error("Unknown language tag format: {0}")]
    UnknownFormat(String),
    #[error("Missing major version in {implementation} language tag: {tag}")]
    MissingMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid major version in {implementation} language tag: {tag}")]
    InvalidMajorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Missing minor version in {implementation} language tag: {tag}")]
    MissingMinorVersion {
        implementation: &'static str,
        tag: String,
    },
    #[error("Invalid minor version in {implementation} language tag: {tag}")]
    InvalidMinorVersion {
        implementation: &'static str,
        tag: String,
    },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::language_tag::ParseLanguageTagError;
    use crate::LanguageTag;

    #[test]
    fn none() {
        assert_eq!(LanguageTag::from_str("none"), Ok(LanguageTag::None));
        assert_eq!(LanguageTag::None.to_string(), "none");
    }

    #[test]
    fn python_language() {
        let tag = LanguageTag::Python {
            major: 3,
            minor: None,
        };
        assert_eq!(LanguageTag::from_str("py3"), Ok(tag));
        assert_eq!(tag.to_string(), "py3");

        let tag = LanguageTag::Python {
            major: 3,
            minor: Some(9),
        };
        assert_eq!(LanguageTag::from_str("py39"), Ok(tag));
        assert_eq!(tag.to_string(), "py39");

        assert_eq!(
            LanguageTag::from_str("py"),
            Err(ParseLanguageTagError::MissingMajorVersion {
                implementation: "Python",
                tag: "py".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("pyX"),
            Err(ParseLanguageTagError::InvalidMajorVersion {
                implementation: "Python",
                tag: "pyX".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("py3X"),
            Err(ParseLanguageTagError::InvalidMinorVersion {
                implementation: "Python",
                tag: "py3X".to_string()
            })
        );
    }

    #[test]
    fn cpython_language() {
        let tag = LanguageTag::CPython {
            python_version: (3, 9),
        };
        assert_eq!(LanguageTag::from_str("cp39"), Ok(tag));
        assert_eq!(tag.to_string(), "cp39");

        assert_eq!(
            LanguageTag::from_str("cp"),
            Err(ParseLanguageTagError::MissingMajorVersion {
                implementation: "CPython",
                tag: "cp".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("cpX"),
            Err(ParseLanguageTagError::InvalidMajorVersion {
                implementation: "CPython",
                tag: "cpX".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("cp3X"),
            Err(ParseLanguageTagError::InvalidMinorVersion {
                implementation: "CPython",
                tag: "cp3X".to_string()
            })
        );
    }

    #[test]
    fn pypy_language() {
        let tag = LanguageTag::PyPy {
            python_version: (3, 9),
        };
        assert_eq!(LanguageTag::from_str("pp39"), Ok(tag));
        assert_eq!(tag.to_string(), "pp39");

        assert_eq!(
            LanguageTag::from_str("pp"),
            Err(ParseLanguageTagError::MissingMajorVersion {
                implementation: "PyPy",
                tag: "pp".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("ppX"),
            Err(ParseLanguageTagError::InvalidMajorVersion {
                implementation: "PyPy",
                tag: "ppX".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("pp3X"),
            Err(ParseLanguageTagError::InvalidMinorVersion {
                implementation: "PyPy",
                tag: "pp3X".to_string()
            })
        );
    }

    #[test]
    fn graalpy_language() {
        let tag = LanguageTag::GraalPy {
            python_version: (3, 10),
        };
        assert_eq!(LanguageTag::from_str("graalpy310"), Ok(tag));
        assert_eq!(tag.to_string(), "graalpy310");

        assert_eq!(
            LanguageTag::from_str("graalpy"),
            Err(ParseLanguageTagError::MissingMajorVersion {
                implementation: "GraalPy",
                tag: "graalpy".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("graalpyX"),
            Err(ParseLanguageTagError::InvalidMajorVersion {
                implementation: "GraalPy",
                tag: "graalpyX".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("graalpy3X"),
            Err(ParseLanguageTagError::InvalidMinorVersion {
                implementation: "GraalPy",
                tag: "graalpy3X".to_string()
            })
        );
    }

    #[test]
    fn pyston_language() {
        let tag = LanguageTag::Pyston {
            python_version: (3, 8),
        };
        assert_eq!(LanguageTag::from_str("pyston38"), Ok(tag));
        assert_eq!(tag.to_string(), "pyston38");

        assert_eq!(
            LanguageTag::from_str("pyston"),
            Err(ParseLanguageTagError::MissingMajorVersion {
                implementation: "Pyston",
                tag: "pyston".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("pystonX"),
            Err(ParseLanguageTagError::InvalidMajorVersion {
                implementation: "Pyston",
                tag: "pystonX".to_string()
            })
        );
        assert_eq!(
            LanguageTag::from_str("pyston3X"),
            Err(ParseLanguageTagError::InvalidMinorVersion {
                implementation: "Pyston",
                tag: "pyston3X".to_string()
            })
        );
    }

    #[test]
    fn unknown_language() {
        assert_eq!(
            LanguageTag::from_str("unknown"),
            Err(ParseLanguageTagError::UnknownFormat("unknown".to_string()))
        );
        assert_eq!(
            LanguageTag::from_str(""),
            Err(ParseLanguageTagError::UnknownFormat(String::new()))
        );
    }
}
