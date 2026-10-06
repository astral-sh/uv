use crate::Error;

/// Borrow a complete region, rejecting overflow and reads outside the input.
pub(crate) fn slice(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8], Error> {
    let end = offset.checked_add(size).ok_or(Error::TooLarge)?;
    bytes
        .get(offset..end)
        .ok_or(Error::Malformed("range extends past its containing data"))
}

/// Read a fixed-width field without assuming its alignment in the input.
pub(crate) fn array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Error> {
    let mut value = [0; N];
    value.copy_from_slice(slice(bytes, offset, N)?);
    Ok(value)
}
