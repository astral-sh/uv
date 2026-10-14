unsafe extern "C" {
    fn native_answer(input: u64) -> u64;
}

#[inline(never)]
pub fn answer(input: u64) -> u64 {
    // SAFETY: The linked C function accepts any u64 and accesses no pointers.
    unsafe { native_answer(input) }
}
