use std::ffi::CStr;

use crate::Error;

/// Borrow a complete region, rejecting overflow and reads outside the input.
pub(crate) fn slice(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8], Error> {
    let end = offset.checked_add(size).ok_or(Error::TooLarge)?;
    bytes
        .get(offset..end)
        .ok_or(Error::Malformed("range extends past its containing data"))
}

pub(crate) fn c_string(bytes: &[u8], offset: usize) -> Result<&CStr, Error> {
    let bytes = bytes
        .get(offset..)
        .ok_or(Error::Malformed("invalid string offset"))?;

    CStr::from_bytes_until_nul(bytes).map_err(|_| Error::Malformed("unterminated string"))
}

/// Read a fixed-width field without assuming its alignment in the input.
pub(crate) fn array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Error> {
    let mut value = [0; N];
    value.copy_from_slice(slice(bytes, offset, N)?);
    Ok(value)
}

pub(crate) fn le32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(array(bytes, offset)?))
}

pub(crate) fn le64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(array(bytes, offset)?))
}
