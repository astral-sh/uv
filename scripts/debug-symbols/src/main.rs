use std::hint::black_box;

// SAFETY: The declaration matches the uint32_t signature in native.c.
unsafe extern "C" {
    fn native_frame(value: u32) -> u32;
}

// SAFETY: This fixture owns the rust_frame symbol and defines it exactly once.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn rust_frame(value: u32) -> u32 {
    // SAFETY: native_frame accepts every u32 value and has no pointer arguments.
    let result = unsafe { native_frame(value) };
    black_box(result).wrapping_add(7)
}

fn main() {
    println!("{}", rust_frame(black_box(17)));
}
