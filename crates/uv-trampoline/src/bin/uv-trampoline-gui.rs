#![no_main]
#![windows_subsystem = "windows"]

// build.rs passes a custom linker flag to make this the entrypoint to the executable
#[no_mangle]
pub extern "C" fn entry() -> ! {
    uv_trampoline::bounce::bounce(true)
}
