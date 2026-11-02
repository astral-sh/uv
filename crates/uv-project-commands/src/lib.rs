//! Project command implementations.

use std::path::PathBuf;

use uv_scripts::Pep723Script;

pub use error::ProjectError;

pub mod add;
pub mod audit;
pub mod check;
mod edit;
mod error;
pub mod export;
pub mod format;
pub mod init;
pub mod lock;
pub mod remove;
pub mod run;
pub mod sync;
mod toolchain;
pub mod tree;
pub mod upgrade;
pub mod version;

mod reporters;

/// A Python file that may or may not include an existing PEP 723 script tag.
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
pub enum ScriptPath {
    /// The Python file already includes a PEP 723 script tag.
    Script(Pep723Script),
    /// The Python file does not include a PEP 723 script tag.
    Path(PathBuf),
}
