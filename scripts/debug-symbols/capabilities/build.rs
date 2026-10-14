use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=native.c");
    let mut build = cc::Build::new();
    build.file("native.c");
    let compiler = build.try_get_compiler()?;
    println!("cargo:warning=C compiler: {:?}", compiler.to_command());
    build.try_compile("capability_native")?;
    match std::env::var("CARGO_CFG_TARGET_OS")?.as_str() {
        "windows" => println!("cargo:rustc-link-arg=/EXPORT:capability_checkpoint"),
        "linux" => {
            println!("cargo:rustc-link-arg=-Wl,--export-dynamic-symbol=capability_checkpoint")
        }
        "macos" => println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_capability_checkpoint"),
        other => return Err(format!("Unsupported debugger fixture platform: {other}").into()),
    }
    Ok(())
}
