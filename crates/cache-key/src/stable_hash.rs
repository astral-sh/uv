use std::hash::Hasher;

use seahash::SeaHasher;

/// A trait for types that can be hashed in a stable way across versions and platforms.
pub trait StableHash {
    fn stable_hash(&self, state: &mut StableHasher);

    fn stable_hash_slice(data: &[Self], state: &mut StableHasher)
    where
        Self: Sized,
    {
        for piece in data {
            piece.stable_hash(state);
        }
    }
}

#[derive(Clone, Default)]
pub struct StableHasher {
    inner: SeaHasher,
}

impl StableHasher {
    pub fn new() -> Self {
        Self {
            inner: SeaHasher::new(),
        }
    }

    pub fn finish(self) -> u64 {
        self.inner.finish()
    }
}

impl Hasher for StableHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.inner.finish()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.inner.write(bytes);
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.inner.write_u8(i);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.inner.write_u16(i);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.inner.write_u32(i);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.inner.write_u64(i);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.inner.write_u128(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.inner.write_usize(i);
    }

    #[inline]
    fn write_i8(&mut self, i: i8) {
        self.inner.write_i8(i);
    }

    #[inline]
    fn write_i16(&mut self, i: i16) {
        self.inner.write_i16(i);
    }

    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.inner.write_i32(i);
    }

    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.inner.write_i64(i);
    }

    #[inline]
    fn write_i128(&mut self, i: i128) {
        self.inner.write_i128(i);
    }

    #[inline]
    fn write_isize(&mut self, i: isize) {
        self.inner.write_isize(i);
    }
}
