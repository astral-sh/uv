use core::mem::size_of;

pub trait SizeOf {
    fn size_of(&self) -> u32;
}

impl<T: Sized> SizeOf for T {
    fn size_of(&self) -> u32 {
        size_of::<T>() as u32
    }
}

// CStr literal: c!("...")
#[macro_export]
macro_rules! c {
    ($s:literal) => {
        std::ffi::CStr::from_bytes_with_nul_unchecked(concat!($s, "\0").as_bytes())
    };
}
