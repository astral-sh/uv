/// User-configured mirrors for managed Python downloads.
#[derive(Debug, Default, Clone, Copy)]
pub struct PythonDownloadMirrors<'a> {
    /// Mirror for CPython distributions from `python-build-standalone`.
    pub cpython: Option<&'a str>,
    /// Mirror for PyPy distributions.
    pub pypy: Option<&'a str>,
    /// Mirror for GraalPy distributions.
    pub graalpy: Option<&'a str>,
    /// Mirror for Pyodide distributions.
    pub pyodide: Option<&'a str>,
}
