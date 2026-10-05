use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=native.c");

    let mut build = cc::Build::new();
    build.file("native.c");
    let compiler = build.try_get_compiler()?;
    println!("cargo:warning=C compiler: {:?}", compiler.to_command());
    build.try_compile("symbol_fixture_native")?;

    // Export both addresses so they can be read from a PE executable without its PDB.
    if std::env::var("CARGO_CFG_TARGET_ENV")? == "msvc" {
        println!("cargo:rustc-link-arg=/EXPORT:rust_frame");
        println!("cargo:rustc-link-arg=/EXPORT:native_frame");
    }
    Ok(())
}
