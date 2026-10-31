//! SIMD comparison of package names after normalization.

#![expect(
    unsafe_code,
    reason = "loading architecture-specific SIMD vectors requires unsafe intrinsics"
)]

pub(crate) const MIN_LEN: usize = 4;

pub(crate) fn matches(actual: &[u8], expected: &[u8]) -> bool {
    debug_assert_eq!(actual.len(), expected.len());
    debug_assert!(actual.len() >= MIN_LEN);
    architecture::matches(actual, expected)
}

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod architecture {
    use std::arch::aarch64::{
        uint8x8_t, uint8x16_t, vadd_u8, vaddq_u8, vand_u8, vandq_u8, vceq_u8, vceqq_u8, vcge_u8,
        vcgeq_u8, vcle_u8, vcleq_u8, vcreate_u8, vdup_n_u8, vdupq_n_u8, vld1_u8, vld1q_u8,
        vminv_u8, vminvq_u8, vorr_u8, vorrq_u8,
    };

    pub(super) fn matches(actual: &[u8], expected: &[u8]) -> bool {
        // SAFETY: Advanced SIMD is available on all AArch64 targets. The inner function only
        // loads chunks that fit within both equal-length slices.
        unsafe { matches_inner(actual, expected) }
    }

    #[target_feature(enable = "neon")]
    unsafe fn matches_inner(actual: &[u8], expected: &[u8]) -> bool {
        if actual.len() < 8 {
            if !unsafe { matches4(actual.as_ptr(), expected.as_ptr()) } {
                return false;
            }
            if actual.len() > 4 {
                let index = actual.len() - 4;
                return unsafe {
                    matches4(actual.as_ptr().add(index), expected.as_ptr().add(index))
                };
            }
            return true;
        }

        if actual.len() < 16 {
            if !unsafe { matches8(actual.as_ptr(), expected.as_ptr()) } {
                return false;
            }
            if actual.len() > 8 {
                let index = actual.len() - 8;
                return unsafe {
                    matches8(actual.as_ptr().add(index), expected.as_ptr().add(index))
                };
            }
            return true;
        }

        let full_chunks_end = actual.len() / 16 * 16;
        for index in (0..full_chunks_end).step_by(16) {
            if !unsafe { matches16(actual.as_ptr().add(index), expected.as_ptr().add(index)) } {
                return false;
            }
        }
        if !actual.len().is_multiple_of(16) {
            let index = actual.len() - 16;
            if !unsafe { matches16(actual.as_ptr().add(index), expected.as_ptr().add(index)) } {
                return false;
            }
        }
        true
    }

    #[target_feature(enable = "neon")]
    unsafe fn matches4(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have four readable bytes.
        let actual = u64::from(unsafe { actual.cast::<u32>().read_unaligned() });
        let expected = u64::from(unsafe { expected.cast::<u32>().read_unaligned() });
        vminv_u8(unsafe { match_mask8(vcreate_u8(actual), vcreate_u8(expected)) }) == u8::MAX
    }

    #[target_feature(enable = "neon")]
    unsafe fn matches8(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have eight readable bytes.
        let actual = unsafe { vld1_u8(actual) };
        let expected = unsafe { vld1_u8(expected) };
        vminv_u8(unsafe { match_mask8(actual, expected) }) == u8::MAX
    }

    #[target_feature(enable = "neon")]
    unsafe fn matches16(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have 16 readable bytes.
        let actual = unsafe { vld1q_u8(actual) };
        let expected = unsafe { vld1q_u8(expected) };
        vminvq_u8(unsafe { match_mask16(actual, expected) }) == u8::MAX
    }

    #[target_feature(enable = "neon")]
    unsafe fn match_mask8(actual: uint8x8_t, expected: uint8x8_t) -> uint8x8_t {
        let uppercase = vand_u8(
            vcge_u8(actual, vdup_n_u8(b'A')),
            vcle_u8(actual, vdup_n_u8(b'Z')),
        );
        let lowercase = vadd_u8(actual, vand_u8(uppercase, vdup_n_u8(0x20)));
        let separator = vorr_u8(
            vceq_u8(actual, vdup_n_u8(b'_')),
            vceq_u8(actual, vdup_n_u8(b'.')),
        );
        vorr_u8(
            vand_u8(separator, vceq_u8(expected, vdup_n_u8(b'-'))),
            vand_u8(
                vceq_u8(separator, vdup_n_u8(0)),
                vceq_u8(lowercase, expected),
            ),
        )
    }

    #[target_feature(enable = "neon")]
    unsafe fn match_mask16(actual: uint8x16_t, expected: uint8x16_t) -> uint8x16_t {
        let uppercase = vandq_u8(
            vcgeq_u8(actual, vdupq_n_u8(b'A')),
            vcleq_u8(actual, vdupq_n_u8(b'Z')),
        );
        let lowercase = vaddq_u8(actual, vandq_u8(uppercase, vdupq_n_u8(0x20)));
        let separator = vorrq_u8(
            vceqq_u8(actual, vdupq_n_u8(b'_')),
            vceqq_u8(actual, vdupq_n_u8(b'.')),
        );
        vorrq_u8(
            vandq_u8(separator, vceqq_u8(expected, vdupq_n_u8(b'-'))),
            vandq_u8(
                vceqq_u8(separator, vdupq_n_u8(0)),
                vceqq_u8(lowercase, expected),
            ),
        )
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod architecture {
    use arch::{
        __m128i, _mm_add_epi8, _mm_and_si128, _mm_andnot_si128, _mm_cmpeq_epi8, _mm_cmpgt_epi8,
        _mm_cvtsi32_si128, _mm_loadl_epi64, _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128,
        _mm_set1_epi8,
    };
    #[cfg(target_arch = "x86")]
    use std::arch::x86 as arch;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64 as arch;

    pub(super) fn matches(actual: &[u8], expected: &[u8]) -> bool {
        // SAFETY: SSE2 is available on all x86-64 targets and checked by the caller on x86. The
        // inner function only loads chunks that fit within both equal-length slices.
        unsafe { matches_inner(actual, expected) }
    }

    #[target_feature(enable = "sse2")]
    unsafe fn matches_inner(actual: &[u8], expected: &[u8]) -> bool {
        if actual.len() < 8 {
            if !unsafe { matches4(actual.as_ptr(), expected.as_ptr()) } {
                return false;
            }
            if actual.len() > 4 {
                let index = actual.len() - 4;
                return unsafe {
                    matches4(actual.as_ptr().add(index), expected.as_ptr().add(index))
                };
            }
            return true;
        }

        if actual.len() < 16 {
            if !unsafe { matches8(actual.as_ptr(), expected.as_ptr()) } {
                return false;
            }
            if actual.len() > 8 {
                let index = actual.len() - 8;
                return unsafe {
                    matches8(actual.as_ptr().add(index), expected.as_ptr().add(index))
                };
            }
            return true;
        }

        let full_chunks_end = actual.len() / 16 * 16;
        for index in (0..full_chunks_end).step_by(16) {
            if !unsafe { matches16(actual.as_ptr().add(index), expected.as_ptr().add(index)) } {
                return false;
            }
        }
        if !actual.len().is_multiple_of(16) {
            let index = actual.len() - 16;
            if !unsafe { matches16(actual.as_ptr().add(index), expected.as_ptr().add(index)) } {
                return false;
            }
        }
        true
    }

    #[target_feature(enable = "sse2")]
    unsafe fn matches4(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have four readable bytes.
        let actual = unsafe { actual.cast::<u32>().read_unaligned() };
        let expected = unsafe { expected.cast::<u32>().read_unaligned() };
        unsafe {
            matches_vector(
                _mm_cvtsi32_si128(actual.cast_signed()),
                _mm_cvtsi32_si128(expected.cast_signed()),
            )
        }
    }

    #[target_feature(enable = "sse2")]
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "_mm_loadl_epi64 accepts unaligned pointers"
    )]
    unsafe fn matches8(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have eight readable bytes.
        let actual = unsafe { _mm_loadl_epi64(actual.cast::<__m128i>()) };
        let expected = unsafe { _mm_loadl_epi64(expected.cast::<__m128i>()) };
        unsafe { matches_vector(actual, expected) }
    }

    #[target_feature(enable = "sse2")]
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "_mm_loadu_si128 accepts unaligned pointers"
    )]
    unsafe fn matches16(actual: *const u8, expected: *const u8) -> bool {
        // SAFETY: The caller guarantees that both pointers have 16 readable bytes.
        let actual = unsafe { _mm_loadu_si128(actual.cast::<__m128i>()) };
        let expected = unsafe { _mm_loadu_si128(expected.cast::<__m128i>()) };
        unsafe { matches_vector(actual, expected) }
    }

    #[target_feature(enable = "sse2")]
    unsafe fn matches_vector(actual: __m128i, expected: __m128i) -> bool {
        let uppercase = _mm_and_si128(
            _mm_cmpgt_epi8(actual, _mm_set1_epi8((b'A' - 1).cast_signed())),
            _mm_cmpgt_epi8(_mm_set1_epi8((b'Z' + 1).cast_signed()), actual),
        );
        let lowercase = _mm_add_epi8(actual, _mm_and_si128(uppercase, _mm_set1_epi8(0x20)));
        let separator = _mm_or_si128(
            _mm_cmpeq_epi8(actual, _mm_set1_epi8(b'_'.cast_signed())),
            _mm_cmpeq_epi8(actual, _mm_set1_epi8(b'.'.cast_signed())),
        );
        let separator_matches = _mm_and_si128(
            separator,
            _mm_cmpeq_epi8(expected, _mm_set1_epi8(b'-'.cast_signed())),
        );
        let regular_matches = _mm_andnot_si128(separator, _mm_cmpeq_epi8(lowercase, expected));
        let matches = _mm_or_si128(separator_matches, regular_matches);
        _mm_movemask_epi8(matches) == 0xffff
    }
}
