//! this is the single integration test, as documented by matklad
//! in <https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html>

pub(crate) mod common;

#[cfg(feature = "python")]
mod venv;
