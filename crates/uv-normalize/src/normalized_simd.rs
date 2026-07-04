//! SIMD fast path for recognizing already-normalized package names.

#![expect(
    unsafe_code,
    reason = "loading architecture-specific SIMD vectors requires unsafe intrinsics"
)]

use super::is_normalized_scalar;

#[cfg(target_arch = "aarch64")]
pub(crate) const MIN_LEN: usize = 8;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) const MIN_LEN: usize = 4;

pub(crate) fn is_normalized(name: &[u8]) -> Result<bool, ()> {
    debug_assert!(name.len() >= MIN_LEN);
    if name.first() == Some(&b'-') || name.last() == Some(&b'-') {
        return is_normalized_scalar(name, None);
    }
    architecture::is_normalized(name)
}

#[cfg(target_arch = "aarch64")]
mod architecture {
    use std::arch::aarch64::{
        vand_u8, vandq_u8, vceq_u8, vceqq_u8, vcge_u8, vcgeq_u8, vcle_u8, vcleq_u8, vdup_n_u8,
        vdupq_n_u8, vext_u8, vextq_u8, vld1_u8, vld1q_u8, vminv_u8, vminvq_u8, vmvn_u8, vmvnq_u8,
        vorr_u8, vorrq_u8,
    };

    use super::is_normalized_scalar;

    pub(super) fn is_normalized(name: &[u8]) -> Result<bool, ()> {
        // SAFETY: Advanced SIMD is available on all AArch64 targets. The inner function only
        // loads chunks that fit within `name`.
        unsafe { is_normalized_inner(name) }
    }

    #[target_feature(enable = "neon")]
    unsafe fn is_normalized_inner(name: &[u8]) -> Result<bool, ()> {
        if name.len() < 16 {
            if !unsafe { is_normalized_chunk8(name.as_ptr(), 0) } {
                return is_normalized_scalar(name, None);
            }
            if name.len() > 8 {
                let index = name.len() - 8;
                let last = name[index - 1];
                if !unsafe { is_normalized_chunk8(name.as_ptr().add(index), last) } {
                    return is_normalized_scalar(&name[index..], Some(last));
                }
            }
            return Ok(true);
        }

        let full_chunks_end = name.len() / 16 * 16;
        for index in (0..full_chunks_end).step_by(16) {
            let last = index.checked_sub(1).map(|index| name[index]);
            if !unsafe { is_normalized_chunk16(name.as_ptr().add(index), last.unwrap_or(0)) } {
                return is_normalized_scalar(&name[index..], last);
            }
        }
        if !name.len().is_multiple_of(16) {
            let index = name.len() - 16;
            let last = name[index - 1];
            if !unsafe { is_normalized_chunk16(name.as_ptr().add(index), last) } {
                return is_normalized_scalar(&name[index..], Some(last));
            }
        }
        Ok(true)
    }

    #[target_feature(enable = "neon")]
    unsafe fn is_normalized_chunk8(name: *const u8, last: u8) -> bool {
        // SAFETY: The caller guarantees that `name` has eight readable bytes.
        let value = unsafe { vld1_u8(name) };
        let lowercase = vand_u8(
            vcge_u8(value, vdup_n_u8(b'a')),
            vcle_u8(value, vdup_n_u8(b'z')),
        );
        let digit = vand_u8(
            vcge_u8(value, vdup_n_u8(b'0')),
            vcle_u8(value, vdup_n_u8(b'9')),
        );
        let dash = vceq_u8(value, vdup_n_u8(b'-'));
        let valid = vorr_u8(vorr_u8(lowercase, digit), dash);
        let shifted = vext_u8::<7>(vdup_n_u8(last), value);
        let repeated_dash = vand_u8(dash, vceq_u8(shifted, vdup_n_u8(b'-')));
        vminv_u8(vand_u8(valid, vmvn_u8(repeated_dash))) == u8::MAX
    }

    #[target_feature(enable = "neon")]
    unsafe fn is_normalized_chunk16(name: *const u8, last: u8) -> bool {
        // SAFETY: The caller guarantees that `name` has 16 readable bytes.
        let value = unsafe { vld1q_u8(name) };
        let lowercase = vandq_u8(
            vcgeq_u8(value, vdupq_n_u8(b'a')),
            vcleq_u8(value, vdupq_n_u8(b'z')),
        );
        let digit = vandq_u8(
            vcgeq_u8(value, vdupq_n_u8(b'0')),
            vcleq_u8(value, vdupq_n_u8(b'9')),
        );
        let dash = vceqq_u8(value, vdupq_n_u8(b'-'));
        let valid = vorrq_u8(vorrq_u8(lowercase, digit), dash);
        let shifted = vextq_u8::<15>(vdupq_n_u8(last), value);
        let repeated_dash = vandq_u8(dash, vceqq_u8(shifted, vdupq_n_u8(b'-')));
        vminvq_u8(vandq_u8(valid, vmvnq_u8(repeated_dash))) == u8::MAX
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod architecture {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{
        __m128i, _mm_and_si128, _mm_cmpeq_epi8, _mm_cmpgt_epi8, _mm_cvtsi32_si128, _mm_loadl_epi64,
        _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128, _mm_set1_epi8,
    };
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_cmpeq_epi8, _mm_cmpgt_epi8, _mm_cvtsi32_si128, _mm_loadl_epi64,
        _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128, _mm_set1_epi8,
    };

    use super::is_normalized_scalar;

    pub(super) fn is_normalized(name: &[u8]) -> Result<bool, ()> {
        // SAFETY: SSE2 is available on all x86-64 targets and checked by the caller on x86. The
        // inner function only loads chunks that fit within `name`.
        unsafe { is_normalized_inner(name) }
    }

    #[target_feature(enable = "sse2")]
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "SSE2 unaligned loads accept byte-aligned pointers"
    )]
    unsafe fn is_normalized_inner(name: &[u8]) -> Result<bool, ()> {
        let mut offset = 0;
        let mut last = None;

        let is_normalized_chunk = |bytes, expected_mask: u32, last| {
            let lowercase = _mm_and_si128(
                _mm_cmpgt_epi8(bytes, _mm_set1_epi8((b'a' - 1).cast_signed())),
                _mm_cmpgt_epi8(_mm_set1_epi8((b'z' + 1).cast_signed()), bytes),
            );
            let digit = _mm_and_si128(
                _mm_cmpgt_epi8(bytes, _mm_set1_epi8((b'0' - 1).cast_signed())),
                _mm_cmpgt_epi8(_mm_set1_epi8((b'9' + 1).cast_signed()), bytes),
            );
            let dash = _mm_cmpeq_epi8(bytes, _mm_set1_epi8(b'-'.cast_signed()));

            let dash_mask = _mm_movemask_epi8(dash).cast_unsigned();
            let normalized = _mm_or_si128(lowercase, _mm_or_si128(digit, dash));
            _mm_movemask_epi8(normalized).cast_unsigned() & expected_mask == expected_mask
                && dash_mask & (dash_mask << 1) == 0
                && (dash_mask & 1 == 0 || !matches!(last, None | Some(b'-')))
        };

        while offset + 16 <= name.len() {
            // SAFETY: The loop condition guarantees that 16 bytes are available from `offset`,
            // and `_mm_loadu_si128` permits an unaligned pointer.
            let bytes = unsafe { _mm_loadu_si128(name.as_ptr().add(offset).cast::<__m128i>()) };
            if !is_normalized_chunk(bytes, 0xffff, last) {
                return is_normalized_scalar(&name[offset..], last);
            }

            offset += 16;
            last = name.get(offset - 1).copied();
        }

        if offset + 8 <= name.len() {
            // SAFETY: The condition guarantees that eight bytes are available from `offset`, and
            // `_mm_loadl_epi64` permits an unaligned pointer.
            let bytes = unsafe { _mm_loadl_epi64(name.as_ptr().add(offset).cast::<__m128i>()) };
            if !is_normalized_chunk(bytes, 0xff, last) {
                return is_normalized_scalar(&name[offset..], last);
            }
            offset += 8;
            last = name.get(offset - 1).copied();
        }

        if offset + 4 <= name.len() {
            // SAFETY: The condition guarantees that four bytes are available from `offset`, and
            // `read_unaligned` permits a byte-aligned pointer.
            let word = unsafe { name.as_ptr().add(offset).cast::<i32>().read_unaligned() };
            let bytes = _mm_cvtsi32_si128(word);
            if !is_normalized_chunk(bytes, 0xf, last) {
                return is_normalized_scalar(&name[offset..], last);
            }
            offset += 4;
            last = name.get(offset - 1).copied();
        }

        is_normalized_scalar(&name[offset..], last)
    }
}
