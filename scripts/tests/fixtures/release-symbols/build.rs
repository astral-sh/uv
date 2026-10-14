fn main() {
    cc::Build::new().file("native.c").compile("symbol_probe");
    println!("cargo:rerun-if-changed=native.c");
}
