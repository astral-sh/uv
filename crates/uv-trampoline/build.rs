// This embeds a "manifest" - a special XML document - into our built binary.
// The main things it does is tell Windows that we want to use the magic
// utf8 codepage, so we can use the *A versions of Windows API functions and
// don't have to mess with utf-16.
use embed_manifest::{embed_manifest, new_manifest};

use uv_static::EnvVars;

fn main() {
    if std::env::var_os(EnvVars::CARGO_CFG_WINDOWS).is_some() {
        let manifest =
            new_manifest("uv.Trampoline").remove_dependency("Microsoft.Windows.Common-Controls");
        embed_manifest(manifest).expect("unable to embed manifest");
        println!("cargo::rustc-link-lib=ucrt"); // https://github.com/rust-lang/rust/issues/143172
        println!("cargo:rerun-if-changed=build.rs");
    }
}
