//! The sole purpose of this crate is to enable one of
//! `flate2/zlib-ng` (on most platforms) or `flate2/rust_backend`
//! (on s390x, powerpc64, etc. — anywhere libz-ng doesn't build)
//!
//! See `Cargo.toml`
