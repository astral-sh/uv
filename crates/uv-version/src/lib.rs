/// Return the application version.
///
/// This should be in sync with uv's version based on the Crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;
