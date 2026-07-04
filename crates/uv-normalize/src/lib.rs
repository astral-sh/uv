use std::error::Error;
use std::fmt::{Display, Formatter};

pub use dist_info_name::DistInfoName;
pub use extra_name::{DefaultExtras, ExtraName};
pub use group_name::{DEV_DEPENDENCIES, DefaultGroups, GroupName, PipGroupName};
pub use package_name::PackageName;

use uv_small_str::SmallString;

mod dist_info_name;
mod extra_name;
mod group_name;
mod package_name;

/// Validate and normalize an unowned package or extra name.
pub(crate) fn validate_and_normalize_ref(
    name: impl AsRef<str>,
) -> Result<SmallString, InvalidNameError> {
    let name = name.as_ref();
    if is_normalized(name)? {
        Ok(SmallString::from(name))
    } else {
        Ok(SmallString::from(normalize(name)?))
    }
}

/// Normalize an unowned package or extra name.
fn normalize(name: &str) -> Result<String, InvalidNameError> {
    // An empty string is not a valid package, extra, or group name.
    if name.is_empty() {
        return Err(InvalidNameError(name.to_string()));
    }

    let mut normalized = String::with_capacity(name.len());

    let mut last = None;
    for char in name.bytes() {
        match char {
            b'A'..=b'Z' => {
                normalized.push(char.to_ascii_lowercase() as char);
            }
            b'a'..=b'z' | b'0'..=b'9' => {
                normalized.push(char as char);
            }
            b'-' | b'_' | b'.' => {
                match last {
                    // Names can't start with punctuation.
                    None => return Err(InvalidNameError(name.to_string())),
                    Some(b'-' | b'_' | b'.') => {}
                    Some(_) => normalized.push('-'),
                }
            }
            _ => return Err(InvalidNameError(name.to_string())),
        }
        last = Some(char);
    }

    // Names can't end with punctuation.
    if matches!(last, Some(b'-' | b'_' | b'.')) {
        return Err(InvalidNameError(name.to_string()));
    }

    Ok(normalized)
}

/// Returns `true` if the name is already normalized.
fn is_normalized(name: impl AsRef<str>) -> Result<bool, InvalidNameError> {
    let name = name.as_ref();

    // An empty string is not a valid package, extra, or group name.
    if name.is_empty() {
        return Err(InvalidNameError(name.to_string()));
    }

    is_normalized_bytes(name.as_bytes()).map_err(|()| InvalidNameError(name.to_string()))
}

/// Returns `true` if the bytes contain an already-normalized name.
///
/// The caller is responsible for rejecting an empty name.
#[cfg_attr(
    any(target_arch = "x86", target_arch = "x86_64"),
    expect(unsafe_code, reason = "use SSE2 to validate package names")
)]
fn is_normalized_bytes(name: &[u8]) -> Result<bool, ()> {
    #[cfg(target_arch = "x86_64")]
    if name.len() >= 8 {
        // SAFETY: SSE2 is part of the x86-64 baseline. The implementation only reads within
        // `name` and uses unaligned loads.
        return unsafe { is_normalized_sse2(name) };
    }

    #[cfg(target_arch = "x86")]
    if name.len() >= 8 && std::arch::is_x86_feature_detected!("sse2") {
        // SAFETY: SSE2 support was checked above. The implementation only reads within `name` and
        // uses unaligned loads.
        return unsafe { is_normalized_sse2(name) };
    }

    is_normalized_scalar(name, None)
}

/// Returns `true` if the remaining bytes contain an already-normalized name.
fn is_normalized_scalar(name: &[u8], mut last: Option<u8>) -> Result<bool, ()> {
    for &char in name {
        match char {
            b'A'..=b'Z' => {
                // Uppercase characters need to be converted to lowercase.
                return Ok(false);
            }
            b'a'..=b'z' | b'0'..=b'9' => {}
            b'_' | b'.' => {
                // `_` and `.` are normalized to `-`.
                return Ok(false);
            }
            b'-' => {
                match last {
                    // Names can't start with punctuation.
                    None => return Err(()),
                    Some(b'-') => {
                        // Runs of `-` are normalized to a single `-`.
                        return Ok(false);
                    }
                    Some(_) => {}
                }
            }
            _ => return Err(()),
        }
        last = Some(char);
    }

    // Names can't end with punctuation.
    if matches!(last, Some(b'-' | b'_' | b'.')) {
        return Err(());
    }

    Ok(true)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[expect(
    unsafe_code,
    clippy::cast_ptr_alignment,
    reason = "SSE2 unaligned loads accept byte-aligned pointers"
)]
#[target_feature(enable = "sse2")]
unsafe fn is_normalized_sse2(name: &[u8]) -> Result<bool, ()> {
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
        // SAFETY: The loop condition guarantees that 16 bytes are available from `offset`, and
        // `_mm_loadu_si128` permits an unaligned pointer.
        let bytes = unsafe { _mm_loadu_si128(name.as_ptr().add(offset).cast::<__m128i>()) };
        if !is_normalized_chunk(bytes, 0xffff, last) {
            // Let the scalar implementation distinguish names that need normalization from names
            // that are invalid, preserving which condition it encounters first.
            return is_normalized_scalar(&name[offset..], last);
        }

        offset += 16;
        last = name.get(offset - 1).copied();
    }

    if offset + 8 <= name.len() {
        // SAFETY: The condition guarantees that 8 bytes are available from `offset`, and
        // `_mm_loadl_epi64` permits an unaligned pointer.
        let bytes = unsafe { _mm_loadl_epi64(name.as_ptr().add(offset).cast::<__m128i>()) };
        if !is_normalized_chunk(bytes, 0xff, last) {
            return is_normalized_scalar(&name[offset..], last);
        }
        offset += 8;
        last = name.get(offset - 1).copied();
    }

    if offset + 4 <= name.len() {
        // SAFETY: The condition guarantees that 4 bytes are available from `offset`, and
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

/// Invalid [`PackageName`] or [`ExtraName`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidNameError(String);

impl Display for InvalidNameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Not a valid package or extra name: \"{}\". Names must start and end with a letter or \
            digit and may only contain -, _, ., and alphanumeric characters.",
            self.0
        )
    }
}

impl Error for InvalidNameError {}

/// Path didn't end with `pyproject.toml`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidPipGroupPathError(String);

impl Display for InvalidPipGroupPathError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The `--group` path is required to end in 'pyproject.toml' for compatibility with pip; got: {}",
            self.0,
        )
    }
}
impl Error for InvalidPipGroupPathError {}

/// Possible errors from reading a [`PipGroupName`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidPipGroupError {
    Name(InvalidNameError),
    Path(InvalidPipGroupPathError),
}

impl Display for InvalidPipGroupError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(e) => e.fmt(f),
            Self::Path(e) => e.fmt(f),
        }
    }
}
impl Error for InvalidPipGroupError {}
impl From<InvalidNameError> for InvalidPipGroupError {
    fn from(value: InvalidNameError) -> Self {
        Self::Name(value)
    }
}
impl From<InvalidPipGroupPathError> for InvalidPipGroupError {
    fn from(value: InvalidPipGroupPathError) -> Self {
        Self::Path(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        let inputs = [
            "friendly-bard",
            "Friendly-Bard",
            "FRIENDLY-BARD",
            "friendly.bard",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert_eq!(
                validate_and_normalize_ref(input).unwrap().as_ref(),
                "friendly-bard"
            );
        }
    }

    #[test]
    fn check() {
        let inputs = ["friendly-bard", "friendlybard"];
        for input in inputs {
            assert!(is_normalized(input).unwrap(), "{input:?}");
        }

        let inputs = [
            "friendly.bard",
            "friendly.BARD",
            "friendly_bard",
            "friendly--bard",
            "friendly-.bard",
            "FrIeNdLy-._.-bArD",
        ];
        for input in inputs {
            assert!(!is_normalized(input).unwrap(), "{input:?}");
        }
    }

    #[test]
    fn unchanged() {
        // Unchanged
        let unchanged = ["friendly-bard", "1okay", "okay2"];
        for input in unchanged {
            assert_eq!(validate_and_normalize_ref(input).unwrap().as_ref(), input);
            assert!(is_normalized(input).unwrap());
        }
    }

    #[test]
    fn failures() {
        let failures = [
            "",
            " starts-with-space",
            "-starts-with-dash",
            "ends-with-dash-",
            "ends-with-space ",
            "includes!invalid-char",
            "space in middle",
            "alpha-α",
        ];
        for input in failures {
            assert!(validate_and_normalize_ref(input).is_err());
            assert!(is_normalized(input).is_err());
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    #[expect(unsafe_code, reason = "test the SSE2 implementation directly")]
    fn check_sse2_matches_scalar() {
        #[cfg(target_arch = "x86")]
        if !std::arch::is_x86_feature_detected!("sse2") {
            return;
        }

        fn assert_matches(input: &[u8]) {
            let scalar = is_normalized_scalar(input, None);
            // SAFETY: SSE2 is part of the x86-64 baseline and is checked at the start of the test
            // on x86. `is_normalized_sse2` only reads within `input` using unaligned loads.
            let simd = unsafe { is_normalized_sse2(input) };
            assert_eq!(simd, scalar, "input: {input:?}");
        }

        for length in 4..=80 {
            let input = vec![b'a'; length];
            assert_matches(&input);

            for index in 0..length {
                for byte in [b'z', b'0', b'9', b'-', b'_', b'.', b'A', b'Z', b'!', 0x80] {
                    let mut input = vec![b'a'; length];
                    input[index] = byte;
                    assert_matches(&input);
                }

                if index + 1 < length {
                    let mut input = vec![b'a'; length];
                    input[index] = b'-';
                    input[index + 1] = b'-';
                    assert_matches(&input);
                }
            }
        }

        for input in [
            &b"A!aaaaaaaaaaaaaa"[..],
            &b"!Aaaaaaaaaaaaaaa"[..],
            &b"a--!aaaaaaaaaaaa"[..],
            &b"a!--aaaaaaaaaaaa"[..],
            &b"aaaaaaaaaaaaaaa--"[..],
            &b"aaaaaaaaaaaaaaa!-"[..],
        ] {
            assert_matches(input);
        }
    }
}
