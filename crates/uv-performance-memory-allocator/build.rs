use std::env;
use std::error::Error;
use std::path::PathBuf;

use fs_err as fs;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=mimalloc");

    if env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let source_root = manifest_dir.join("mimalloc");
    let static_source = source_root.join("src").join("static.c");
    let target_env = env::var("CARGO_CFG_TARGET_ENV")?;
    let wrapper_extension = if target_env == "msvc" { "cc" } else { "c" };
    let wrapper =
        PathBuf::from(env::var("OUT_DIR")?).join(format!("mimalloc-static.{wrapper_extension}"));
    let include = static_source.to_string_lossy().replace('\\', "/");

    fs::write(
        &wrapper,
        format!(
            r#"#include "{include}"

#ifdef __cplusplus
extern "C" {{
#endif

long uv_mimalloc_default_purge_delay(void) {{
  return MI_DEFAULT_PURGE_DELAY;
}}

long uv_mimalloc_default_arena_purge_mult(void) {{
  return MI_DEFAULT_ARENA_PURGE_MULT;
}}

int uv_mimalloc_large_pages_enabled(void) {{
  return MI_ENABLE_LARGE_PAGES;
}}

#ifdef __cplusplus
}}
#endif
"#
        ),
    )?;

    let mut build = cc::Build::new();
    build
        .include(source_root.join("include"))
        .include(source_root.join("src"))
        .file(wrapper)
        .define("MI_ENABLE_LARGE_PAGES", "0")
        .define("MI_DEFAULT_PURGE_DELAY", "10")
        .define("MI_DEFAULT_ARENA_PURGE_MULT", "10");

    if target_env == "msvc" {
        // Mimalloc expects the MSVC/clang-cl build to use the C++ atomics path.
        build
            .cpp(true)
            .std("c++17")
            .flag_if_supported("/Zc:__cplusplus");
    }

    let cargo_debug = matches!(env::var("DEBUG").as_deref(), Ok("true") | Ok("1"));
    build.define("MI_DEBUG", "0");
    if !cargo_debug {
        build.define("MI_BUILD_RELEASE", None);
        build.define("NDEBUG", None);
    }

    build.compile("mimalloc");

    for library in ["psapi", "shell32", "user32", "advapi32", "bcrypt"] {
        println!("cargo:rustc-link-lib={library}");
    }

    Ok(())
}
