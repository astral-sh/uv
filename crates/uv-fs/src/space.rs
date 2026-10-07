use std::io;
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::ffi::CString;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(target_os = "linux")]
use linux_raw_sys::ioctl::{
    FIEMAP_EXTENT_DATA_INLINE, FIEMAP_EXTENT_DELALLOC, FIEMAP_EXTENT_ENCODED, FIEMAP_EXTENT_LAST,
    FIEMAP_EXTENT_NOT_ALIGNED, FIEMAP_EXTENT_SHARED, FIEMAP_EXTENT_UNKNOWN, FS_IOC_FIEMAP,
};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios"))]
use rustix::io::Errno;
use thiserror::Error;

/// An error encountered while measuring a file's physical storage.
#[derive(Debug, Error)]
pub enum PhysicalSpaceError {
    /// The filesystem cannot report exclusively owned physical storage.
    #[error("the filesystem does not support physical space accounting")]
    UnsupportedFilesystem,
    /// The file's physical storage could not be measured.
    #[error(transparent)]
    UnmeasurableFile(#[from] io::Error),
}

/// Return whether the current platform supports fine-grained space accounting.
pub const fn supports_fine_grained_accounting() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios"
    ))
}

/// Return the physical file data that would be reclaimed by deleting `path`.
///
/// The result excludes data retained by another hardlink, copy-on-write clone, or snapshot.
/// Filesystem metadata is not included.
pub fn physical_space(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<u64, PhysicalSpaceError> {
    if !metadata.is_file() {
        #[cfg(unix)]
        {
            return Ok(metadata.blocks().saturating_mul(512));
        }

        #[cfg(not(unix))]
        {
            return Ok(0);
        }
    }

    #[cfg(unix)]
    if metadata.nlink() > 1 {
        return Ok(0);
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        apple_physical_space(path)
    }

    #[cfg(target_os = "linux")]
    {
        linux_physical_space(path)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
    {
        let _ = path;
        Err(PhysicalSpaceError::UnsupportedFilesystem)
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[expect(unsafe_code)]
fn apple_physical_space(path: &Path) -> Result<u64, PhysicalSpaceError> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: libc::ATTR_CMNEXT_PRIVATESIZE,
    };
    let mut response = [0_u8; 32];

    // SAFETY: `path` is null-terminated and valid for the duration of the call. `attributes` is a
    // valid attribute request, and `response` has enough space for the length, returned attribute
    // set, and requested private-size value.
    let result = unsafe {
        libc::getattrlist(
            path.as_ptr(),
            (&raw mut attributes).cast(),
            response.as_mut_ptr().cast(),
            response.len(),
            libc::FSOPT_ATTR_CMN_EXTENDED,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if Errno::from_io_error(&error) == Some(Errno::NOTSUP) {
            return Err(PhysicalSpaceError::UnsupportedFilesystem);
        }
        return Err(error.into());
    }

    let returned_fork_attributes =
        u32::from_ne_bytes(response[20..24].try_into().map_err(io::Error::other)?);
    if returned_fork_attributes & libc::ATTR_CMNEXT_PRIVATESIZE == 0 {
        return Err(PhysicalSpaceError::UnsupportedFilesystem);
    }

    let private_size = i64::from_ne_bytes(response[24..32].try_into().map_err(io::Error::other)?);
    Ok(u64::try_from(private_size).map_err(io::Error::other)?)
}

#[cfg(target_os = "linux")]
#[expect(unsafe_code)]
fn linux_physical_space(path: &Path) -> Result<u64, PhysicalSpaceError> {
    const MAX_EXTENTS: usize = 32;

    #[derive(Default)]
    #[repr(C)]
    struct Fiemap {
        start: u64,
        length: u64,
        flags: u32,
        mapped_extents: u32,
        extent_count: u32,
        reserved: u32,
    }

    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    struct FiemapExtent {
        logical: u64,
        physical: u64,
        length: u64,
        reserved64: [u64; 2],
        flags: u32,
        reserved: [u32; 3],
    }

    #[derive(Default)]
    #[repr(C)]
    struct FiemapBuffer {
        header: Fiemap,
        extents: [FiemapExtent; MAX_EXTENTS],
    }

    let file = fs_err::File::open(path)?;
    let mut physical = 0_u64;
    let mut start = 0_u64;

    loop {
        let mut request = FiemapBuffer {
            header: Fiemap {
                start,
                length: u64::MAX.saturating_sub(start),
                extent_count: u32::try_from(MAX_EXTENTS).map_err(io::Error::other)?,
                ..Fiemap::default()
            },
            ..FiemapBuffer::default()
        };

        // SAFETY: `FS_IOC_FIEMAP` is the Linux fiemap ioctl opcode, and `request` begins with the
        // expected fiemap header followed by enough initialized storage for all requested extents.
        unsafe {
            rustix::ioctl::ioctl(
                &file,
                rustix::ioctl::Updater::<{ FS_IOC_FIEMAP as rustix::ioctl::Opcode }, _>::new(
                    &mut request,
                ),
            )
        }
        .map_err(|error| match error {
            // Linux returns `ENOTTY` when the ioctl is unsupported for this file.
            Errno::NOTSUP | Errno::NOTTY => PhysicalSpaceError::UnsupportedFilesystem,
            error => PhysicalSpaceError::UnmeasurableFile(error.into()),
        })?;

        let mapped_extents =
            usize::try_from(request.header.mapped_extents).map_err(io::Error::other)?;
        if mapped_extents > request.extents.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned more extents than requested",
            )
            .into());
        }
        if mapped_extents == 0 {
            return Ok(physical);
        }

        for extent in &request.extents[..mapped_extents] {
            if extent.flags
                & (FIEMAP_EXTENT_DELALLOC | FIEMAP_EXTENT_DATA_INLINE | FIEMAP_EXTENT_SHARED)
                != 0
            {
                continue;
            }

            if extent.flags
                & (FIEMAP_EXTENT_UNKNOWN | FIEMAP_EXTENT_ENCODED | FIEMAP_EXTENT_NOT_ALIGNED)
                != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "the filesystem cannot report the physical size of an extent",
                )
                .into());
            }

            physical = physical.saturating_add(extent.length);
        }

        let last_extent = &request.extents[mapped_extents - 1];
        if last_extent.flags & FIEMAP_EXTENT_LAST != 0 {
            return Ok(physical);
        }

        let next = last_extent.logical.saturating_add(last_extent.length);
        if next <= start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned a non-advancing extent",
            )
            .into());
        }
        start = next;
    }
}
