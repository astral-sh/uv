//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/util/errors.rs>
use std::fmt::{self, Write};

use super::truncate_with_ellipsis;

#[derive(Debug)]
pub(crate) struct HttpNotSuccessful {
    pub(crate) code: u32,
    pub(crate) url: String,
    pub(crate) ip: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl HttpNotSuccessful {
    fn render(&self) -> String {
        let mut result = String::new();
        let body = std::str::from_utf8(&self.body).map_or_else(
            |_| format!("[{} non-utf8 bytes]", self.body.len()),
            |s| truncate_with_ellipsis(s, 512),
        );

        write!(
            result,
            "failed to get successful HTTP response from `{}`",
            self.url
        )
        .unwrap();
        if let Some(ip) = &self.ip {
            write!(result, " ({ip})").unwrap();
        }
        writeln!(result, ", got {}", self.code).unwrap();
        write!(result, "body:\n{body}").unwrap();
        result
    }
}

impl fmt::Display for HttpNotSuccessful {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for HttpNotSuccessful {}
