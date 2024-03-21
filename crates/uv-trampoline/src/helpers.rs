use core::mem::size_of;

pub trait SizeOf {
    fn size_of(&self) -> u32;
}

impl<T: Sized> SizeOf for T {
    fn size_of(&self) -> u32 {
        size_of::<T>() as u32
    }
}
