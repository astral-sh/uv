//! Compile Python source to CPython bytecode without starting a Python interpreter.
//!
//! This crate currently targets CPython 3.14.5 exclusively. The bytecode, code object,
//! marshal, and `.pyc` formats are all implementation details that can change between
//! Python releases, so future targets should be implemented as separate backends.

mod assembler;
mod compiler;
mod marshal;
mod target;

use std::error::Error;
use std::fmt;

use compiler::{CodeObject, Compiler};

/// The exact CPython implementation targeted by this compiler backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpythonTarget {
    /// The value of `sys.implementation.name`.
    pub implementation: &'static str,
    /// The value of `sys.version_info[:3]`.
    pub version: (u8, u8, u8),
    /// The value of `importlib.util.MAGIC_NUMBER`.
    pub magic_number: [u8; 4],
    /// The upstream CPython release tag.
    pub tag: &'static str,
    /// The upstream CPython source commit.
    pub commit: &'static str,
}

impl CpythonTarget {
    /// Return the dotted form of [`Self::version`].
    pub fn version_string(self) -> String {
        let (major, minor, micro) = self.version;
        format!("{major}.{minor}.{micro}")
    }

    /// Return the lowercase hexadecimal form of [`Self::magic_number`].
    pub fn magic_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut output = String::with_capacity(self.magic_number.len() * 2);
        for byte in self.magic_number {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

/// The sole CPython target currently implemented by this crate.
pub const CPYTHON_TARGET: CpythonTarget = CpythonTarget {
    implementation: target::TARGET_IMPLEMENTATION,
    version: target::TARGET_VERSION,
    magic_number: target::TARGET_MAGIC_NUMBER,
    tag: target::TARGET_TAG,
    commit: target::TARGET_COMMIT,
};

/// A compiled CPython 3.14.5 module code object.
#[derive(Clone, Debug)]
pub struct CompiledModule {
    code: CodeObject,
}

impl CompiledModule {
    /// Return the module's raw CPython instruction stream.
    pub fn bytecode(&self) -> &[u8] {
        &self.code.bytecode
    }

    /// Marshal the module code object using CPython 3.14.5's format.
    pub fn marshal(&self) -> Vec<u8> {
        marshal::encode_code(&self.code)
    }

    /// Build a timestamp-invalidated CPython 3.14.5 `.pyc` file.
    ///
    /// `source_mtime` is the source file's modification time in whole seconds since the
    /// Unix epoch. `source_size` is the source file's byte length, truncated to 32 bits as
    /// required by CPython's cache format.
    pub fn to_timestamp_pyc(&self, source_mtime: u32, source_size: u32) -> Vec<u8> {
        let marshalled = self.marshal();
        let mut output = Vec::with_capacity(16 + marshalled.len());
        output.extend_from_slice(&CPYTHON_TARGET.magic_number);
        output.extend_from_slice(&0_u32.to_le_bytes());
        output.extend_from_slice(&source_mtime.to_le_bytes());
        output.extend_from_slice(&source_size.to_le_bytes());
        output.extend_from_slice(&marshalled);
        output
    }
}

/// A source parsing or bytecode generation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    /// Ruff rejected the Python source.
    Parse(String),
    /// The source uses valid Python syntax that this backend does not support yet.
    Unsupported(String),
    /// The compiler violated one of its own invariants.
    Internal(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "failed to parse Python source: {message}"),
            Self::Unsupported(message) => formatter.write_str(message),
            Self::Internal(message) => write!(formatter, "internal compiler error: {message}"),
        }
    }
}

impl Error for CompileError {}

/// Compile a Python module for CPython 3.14.5.
pub fn compile(source: &str, filename: &str) -> Result<CompiledModule, CompileError> {
    let normalized;
    let source = if source.contains('\r') {
        normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        source
    };
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let options = ruff_python_parser::ParseOptions::from(ruff_python_parser::Mode::Module)
        .with_target_version(ruff_python_ast::PythonVersion::PY314);
    let parsed = ruff_python_parser::parse(source, options)
        .map_err(|error| CompileError::Parse(error.to_string()))?
        .try_into_module()
        .ok_or_else(|| CompileError::Internal("parser did not return a module".to_string()))?;
    let code = Compiler::module(filename, source).compile_module(parsed.suite())?;
    Ok(CompiledModule { code })
}

/// Compile a Python module directly to a timestamp-invalidated CPython 3.14.5 `.pyc` file.
pub fn compile_to_pyc(
    source: &str,
    filename: &str,
    source_mtime: u32,
) -> Result<Vec<u8>, CompileError> {
    let source_size = u32::try_from(source.len()).map_err(|_| {
        CompileError::Unsupported("Python source exceeds the 4 GiB `.pyc` limit".to_string())
    })?;
    Ok(compile(source, filename)?.to_timestamp_pyc(source_mtime, source_size))
}
