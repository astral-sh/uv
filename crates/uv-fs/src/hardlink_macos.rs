use std::ffi::{CStr, OsStr};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use fs_err::os::unix::fs::OpenOptionsExt;

// Constants from sys/attr.h and sys/vnode.h that libc does not expose.
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const VREG: u32 = 1;
const VDIR: u32 = 2;

// getattrlistbulk returns an eight-byte-aligned sequence of variable-length records.
#[repr(align(8))]
struct AttributeBuffer([u8; 64 * 1024]);

/// Collect pruning candidates with `getattrlistbulk`, avoiding a metadata call for every file.
#[expect(unsafe_code)]
pub(super) fn single_link_files(path: &Path) -> io::Result<Option<Vec<PathBuf>>> {
    let directory = fs_err::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_OBJTYPE,
        volattr: 0,
        dirattr: 0,
        fileattr: libc::ATTR_FILE_LINKCOUNT,
        forkattr: 0,
    };
    let mut buffer = AttributeBuffer([0; 64 * 1024]);
    let mut files = Vec::new();
    loop {
        // SAFETY: The descriptor remains open and refers to a directory. `attributes` is a valid
        // request, and `buffer` is writable, eight-byte aligned, and has the supplied length.
        let count = unsafe {
            libc::getattrlistbulk(
                directory.as_raw_fd(),
                (&raw mut attributes).cast(),
                buffer.0.as_mut_ptr().cast(),
                buffer.0.len(),
                u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
            )
        };
        let Ok(count) = usize::try_from(count) else {
            let error = io::Error::last_os_error();
            return match error.raw_os_error() {
                Some(libc::ENOTSUP | libc::ENOSYS | libc::EINVAL) => Ok(None),
                _ => Err(error),
            };
        };
        if count == 0 {
            return Ok(Some(files));
        }
        let Some(batch) = single_link_files_from_buffer(path, &buffer.0, count)? else {
            return Ok(None);
        };
        files.extend(batch);
    }
}

/// Decode a batch using the attribute request in [`single_link_files`].
///
/// Offsets assume `FSOPT_PACK_INVAL_ATTRS`; returned attribute bits still determine which values
/// are valid. A subdirectory or missing required attribute discards the batch's candidates and
/// returns `None` so the caller can walk the directory normally.
fn single_link_files_from_buffer(
    directory: &Path,
    mut buffer: &[u8],
    count: usize,
) -> io::Result<Option<Vec<PathBuf>>> {
    // length, returned attribute sets, error, name reference, object type, and link count.
    const HEADER_SIZE: usize = 44;
    const NAME_REFERENCE_OFFSET: usize = 28;
    let mut files = Vec::new();
    for _ in 0..count {
        let length = read_u32(buffer, 0)? as usize;
        let record = buffer
            .get(..length)
            .filter(|record| record.len() >= HEADER_SIZE)
            .ok_or_else(invalid_attributes)?;
        buffer = &buffer[length..];

        let common = read_u32(record, 4)?;
        let error = read_u32(record, 24)?;
        if common & ATTR_CMN_ERROR != 0 && error != 0 {
            let error = io::Error::from_raw_os_error(
                i32::try_from(error).map_err(|_| invalid_attributes())?,
            );
            if error.kind() == io::ErrorKind::NotFound {
                continue;
            }
            return Err(error);
        }
        if common & (libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE)
            != libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE
        {
            return Ok(None);
        }
        match read_u32(record, 36)? {
            VDIR => return Ok(None),
            VREG => {}
            _ => continue,
        }
        if read_u32(record, 16)? & libc::ATTR_FILE_LINKCOUNT == 0 {
            return Ok(None);
        }
        if read_u32(record, 40)? != 1 {
            continue;
        }

        let name_offset = read_u32(record, NAME_REFERENCE_OFFSET)?.cast_signed();
        let name_start = NAME_REFERENCE_OFFSET
            .checked_add_signed(name_offset as isize)
            .ok_or_else(invalid_attributes)?;
        let name_end = name_start
            .checked_add(read_u32(record, 32)? as usize)
            .ok_or_else(invalid_attributes)?;
        let name = record
            .get(name_start..name_end)
            .ok_or_else(invalid_attributes)?;
        let name = CStr::from_bytes_with_nul(name)
            .map_err(|_| invalid_attributes())?
            .to_bytes();
        if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
            return Err(invalid_attributes());
        }
        files.push(directory.join(OsStr::from_bytes(name)));
    }
    Ok(Some(files))
}

fn read_u32(buffer: &[u8], offset: usize) -> io::Result<u32> {
    buffer
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_ne_bytes)
        .ok_or_else(invalid_attributes)
}

fn invalid_attributes() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bulk file attributes")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::{ATTR_CMN_ERROR, VREG, single_link_files, single_link_files_from_buffer};

    #[test]
    fn reads_multiple_batches_without_following_links() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let directory = root.path().join("files");
        fs_err::create_dir(&directory)?;
        let mut expected = Vec::new();
        for index in 0..1500 {
            let path = directory.join(format!("{index:064x}"));
            fs_err::write(&path, [])?;
            expected.push(path);
        }
        let unicode = directory.join("file-λ");
        fs_err::write(&unicode, [])?;
        expected.push(unicode);
        let retained = root.path().join("retained");
        fs_err::write(&retained, [])?;
        fs_err::hard_link(&retained, directory.join("shared"))?;
        fs_err::os::unix::fs::symlink(&retained, directory.join("symlink"))?;

        let mut actual = single_link_files(&directory)?;
        if let Some(files) = &mut actual {
            files.sort();
        }
        expected.sort();
        assert_eq!(actual, Some(expected));

        fs_err::create_dir(directory.join("nested"))?;
        assert!(single_link_files(&directory)?.is_none());
        Ok(())
    }

    fn record(name: &[u8]) -> Vec<u8> {
        let length = (44 + name.len() + 1).next_multiple_of(8);
        let fields = [
            u32::try_from(length).expect("test attribute record fits in u32"),
            libc::ATTR_CMN_RETURNED_ATTRS
                | ATTR_CMN_ERROR
                | libc::ATTR_CMN_NAME
                | libc::ATTR_CMN_OBJTYPE,
            0,
            0,
            libc::ATTR_FILE_LINKCOUNT,
            0,
            0,
            16,
            u32::try_from(name.len()).expect("test file name fits in u32") + 1,
            VREG,
            1,
        ];
        let mut record = fields
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect::<Vec<_>>();
        record.extend_from_slice(name);
        record.resize(length, 0);
        record
    }

    #[test]
    fn preserves_non_utf8_names() -> io::Result<()> {
        let name = OsStr::from_bytes(b"file-\xff");
        assert_eq!(
            single_link_files_from_buffer(Path::new("files"), &record(name.as_bytes()), 1)?,
            Some(vec![Path::new("files").join(name)]),
        );
        Ok(())
    }

    #[test]
    fn falls_back_when_link_counts_are_unavailable() -> io::Result<()> {
        let mut attributes = record(b"file");
        attributes[16..20].copy_from_slice(&0_u32.to_ne_bytes());
        assert!(single_link_files_from_buffer(Path::new("files"), &attributes, 1)?.is_none());
        Ok(())
    }

    #[test]
    fn rejects_truncated_records_and_names_outside_the_directory() {
        let attributes = record(b"file");
        for length in 0..attributes.len() {
            assert!(
                single_link_files_from_buffer(Path::new("files"), &attributes[..length], 1)
                    .is_err()
            );
        }
        for name in [
            b"../outside".as_slice(),
            b"/outside",
            b".",
            b"..",
            b"",
            b"file\0suffix",
        ] {
            assert!(single_link_files_from_buffer(Path::new("files"), &record(name), 1).is_err());
        }
        let mut attributes = record(b"file");
        attributes[28..32].copy_from_slice(&i32::MIN.to_ne_bytes());
        assert!(single_link_files_from_buffer(Path::new("files"), &attributes, 1).is_err());
    }
}
