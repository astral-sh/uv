/// Represents the application version.
/// This should be in sync with uv's version based on the crate version.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_version() {
        // Check Version Value
        assert_eq!(version(), env!("CARGO_PKG_VERSION").to_string());
    }
}
