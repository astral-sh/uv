use fs_err::File;
use std::io::{Read, Seek};

/// Attempts to read the ZIP End of Central Directory (EOCD) from the end of the file and calculate
/// the length of the ZIP data. If successful, returns `Some(offset)`. If an IO or parse error
/// occurs, returns `None`.
pub(crate) fn read_zip_part(file_handle: &mut File) -> Option<u32> {
    // Read the entire end of central directory (EOCD) of the ZIP file, which is 22 bytes long.
    let mut eocd_buf: Vec<u8> = vec![0; 22];

    file_handle.seek(std::io::SeekFrom::End(-22)).ok()?;
    file_handle.read_exact(&mut eocd_buf).ok()?;

    // Check if the magic number 'PK\005\006' is at the start of the EOCD.
    if !eocd_buf.starts_with(b"PK\x05\x06") {
        return None;
    }

    // Size of the central directory (in bytes)
    let cd_size = u32::from_le_bytes(eocd_buf[12..16].try_into().ok()?);

    // Offset of the central directory (in bytes).
    // In other words, the number of bytes in the ZIP file at which the central directory starts.
    let cd_offset = u32::from_le_bytes(eocd_buf[16..20].try_into().ok()?);

    // If the file is actually a UV trampoline, the zip file size will not be large.
    // So we can reject it when length overflows.
    let zip_length = cd_offset.checked_add(cd_size)?.checked_add(22)?;

    Some(zip_length)
}
