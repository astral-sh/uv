use pyo3::prelude::*;

#[pyfunction]
fn greeting() -> &'static str {
    "hello from maturin-cdylib"
}

#[pymodule]
fn maturin_cdylib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(greeting, m)?)?;
    Ok(())
}
