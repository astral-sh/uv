//! Implementation of PEP 639 cross-language restricted globs and a related directory traversal
//! prefilter.
//!
//! The goal is globs that are portable between languages and operating systems.

mod glob_dir_filter;
mod portable_glob;

pub use glob_dir_filter::GlobDirFilter;
pub use portable_glob::{check_portable_glob, parse_portable_glob, PortableGlobError};
