//! Project command implementations.

use std::path::PathBuf;

use uv_scripts::Pep723Script;

pub mod project;

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
