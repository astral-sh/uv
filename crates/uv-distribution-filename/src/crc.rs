use std::sync::LazyLock;

use uv_static::EnvVars;

#[derive(Debug)]
pub enum CRCMode {
    /// Fail on CRC mismatch.
    Enforce,
    /// Warn on CRC mismatch, but continue.
    Lax,
    /// Skip CRC checks.
    None,
}

/// Lazily initialize CRC mode from `UV_CRC_MODE`.
pub static CURRENT_CRC_MODE: LazyLock<CRCMode> =
    LazyLock::new(|| match std::env::var(EnvVars::UV_CRC_MODE).as_deref() {
        Ok("enforce") => CRCMode::Enforce,
        Ok("lax") => CRCMode::Lax,
        _ => CRCMode::None,
    });
