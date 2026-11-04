//! Embedded signatures use big-endian integers.
//! Format definitions: <https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/cs_blobs.h>.

use std::ffi::CStr;

use sha2::{Digest, Sha256};

use crate::Error;
use crate::bytes::{array, c_string, slice};

const SUPERBLOB: u32 = 0xfade_0cc0;
const CODE_DIRECTORY: u32 = 0xfade_0c02;
const DIRECTORY_OFFSET: usize = 20;
const DIRECTORY_VERSION: u32 = 0x20400;
const DIRECTORY_HEADER_SIZE: usize = 88;
const CS_ADHOC: u32 = 2;
const CS_ADHOC_LINKER_SIGNED: u32 = 0x20002;
const PAGE_SHIFT: u8 = 12;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
// Hash size, SHA-256 type, platform, and log2 page size.
const HASH_FORMAT: [u8; 4] = [32, 2, 0, PAGE_SHIFT];
const SHA256_SIZE: usize = HASH_FORMAT[0] as usize;

/// An ad-hoc signature with a single SHA-256 `CodeDirectory`.
pub(crate) struct Signature<'a> {
    identifier: &'a CStr,
    code_limit: usize,
    hash_offset: usize,
    size: usize,
}

impl<'a> Signature<'a> {
    pub(crate) fn prepare(
        existing: Option<&'a [u8]>,
        identifier: &'a CStr,
        code_limit: usize,
    ) -> Result<Self, Error> {
        let identifier = match existing {
            Some(existing) => read_identifier(existing, code_limit)?,
            None => identifier,
        };
        let hash_offset = DIRECTORY_HEADER_SIZE
            .checked_add(identifier.to_bytes_with_nul().len())
            .ok_or(Error::TooLarge)?;
        let hashes_size = code_limit
            .div_ceil(PAGE_SIZE)
            .checked_mul(SHA256_SIZE)
            .ok_or(Error::TooLarge)?;
        let size = DIRECTORY_OFFSET
            .checked_add(hash_offset)
            .and_then(|size| size.checked_add(hashes_size))
            .ok_or(Error::TooLarge)?;
        Ok(Self {
            identifier,
            code_limit,
            hash_offset,
            size,
        })
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// Fill the reserved signature region after the image headers have been written.
    pub(crate) fn write(self, image: &mut [u8], text_size: u64) -> Result<(), Error> {
        let (source, signature) = image.split_at_mut(self.code_limit);
        let (index, directory) = signature[..self.size].split_at_mut(DIRECTORY_OFFSET);
        index.copy_from_slice(
            [
                SUPERBLOB,
                u32::try_from(self.size)?,
                1,
                0,
                u32::try_from(DIRECTORY_OFFSET)?,
            ]
            .map(u32::to_be_bytes)
            .as_flattened(),
        );
        let directory_size = u32::try_from(directory.len())?;
        directory[..36].copy_from_slice(
            [
                CODE_DIRECTORY,
                directory_size,
                DIRECTORY_VERSION,
                CS_ADHOC,
                u32::try_from(self.hash_offset)?,
                u32::try_from(DIRECTORY_HEADER_SIZE)?,
                0,
                u32::try_from(source.len().div_ceil(PAGE_SIZE))?,
                u32::try_from(source.len())?,
            ]
            .map(u32::to_be_bytes)
            .as_flattened(),
        );
        directory[36..40].copy_from_slice(&HASH_FORMAT);
        // __TEXT starts at file offset zero.
        directory[72..80].copy_from_slice(&text_size.to_be_bytes());
        directory[DIRECTORY_HEADER_SIZE..self.hash_offset]
            .copy_from_slice(self.identifier.to_bytes_with_nul());

        let hashes = directory[self.hash_offset..]
            .as_chunks_mut::<SHA256_SIZE>()
            .0;
        for (page, destination) in source.chunks(PAGE_SIZE).zip(hashes) {
            destination.copy_from_slice(&Sha256::digest(page));
        }
        Ok(())
    }
}

/// Recognize the bare ad-hoc signature used by the linker and by this editor.
/// The page hashes are recomputed because the image can already have been edited.
fn read_identifier(signature: &[u8], code_limit: usize) -> Result<&CStr, Error> {
    if u32::from_be_bytes(array(signature, 0)?) != SUPERBLOB {
        return Err(Error::Malformed("invalid signature magic"));
    }
    let size = u32::from_be_bytes(array(signature, 4)?) as usize;
    let blob = slice(signature, 0, size)?;
    let index = slice(blob, 12, 8)?;
    if u32::from_be_bytes(array(blob, 8)?) != 1 || u32::from_be_bytes(array(index, 0)?) != 0 {
        return Err(Error::UnsupportedSignature);
    }

    // A linker signature can align its CodeDirectory after the index.
    let directory_offset = u32::from_be_bytes(array(index, 4)?) as usize;
    let padding = blob
        .get(DIRECTORY_OFFSET..directory_offset)
        .ok_or(Error::Malformed("invalid CodeDirectory offset"))?;
    if padding.iter().any(|&byte| byte != 0) {
        return Err(Error::Malformed("invalid CodeDirectory padding"));
    }
    let directory = &blob[directory_offset..];
    if u32::from_be_bytes(array(directory, 0)?) != CODE_DIRECTORY
        || u32::from_be_bytes(array(directory, 4)?) as usize != directory.len()
        || signature[size..].iter().any(|&byte| byte != 0)
    {
        return Err(Error::Malformed("invalid CodeDirectory range"));
    }
    if u32::from_be_bytes(array(directory, 8)?) != DIRECTORY_VERSION {
        return Err(Error::UnsupportedSignature);
    }

    let header = slice(directory, 0, DIRECTORY_HEADER_SIZE)?;
    let flags = u32::from_be_bytes(array(header, 12)?);
    let special_slots = u32::from_be_bytes(array(header, 24)?);
    if (flags != CS_ADHOC && flags != CS_ADHOC_LINKER_SIGNED)
        || special_slots != 0
        || header[36..40] != HASH_FORMAT
        // Spare fields, scatter, team ID, 64-bit code limit, and executable segment base.
        || header[40..72].iter().any(|&byte| byte != 0)
        || header[80..88].iter().any(|&byte| byte != 0)
    {
        return Err(Error::UnsupportedSignature);
    }

    let hash_offset = u32::from_be_bytes(array(header, 16)?) as usize;
    let identifier_offset = u32::from_be_bytes(array(header, 20)?) as usize;
    let page_count = code_limit.div_ceil(PAGE_SIZE);
    let hash_size = page_count.checked_mul(SHA256_SIZE).ok_or(Error::TooLarge)?;
    if identifier_offset < DIRECTORY_HEADER_SIZE
        || directory.len().checked_sub(hash_offset) != Some(hash_size)
        || u32::from_be_bytes(array(header, 28)?) as usize != page_count
        || u32::from_be_bytes(array(header, 32)?) as usize != code_limit
    {
        return Err(Error::Malformed(
            "invalid CodeDirectory offsets or page count",
        ));
    }

    let identifier = c_string(&directory[..hash_offset], identifier_offset)?;
    if identifier.is_empty() {
        return Err(Error::Malformed("empty signing identifier"));
    }
    Ok(identifier)
}
