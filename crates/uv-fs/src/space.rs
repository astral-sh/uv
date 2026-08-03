use std::io;
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::ffi::CString;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_HANDLE_EOF, ERROR_MORE_DATA, HANDLE};
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    FILE_STANDARD_INFO, FileStandardInfo, GetFileInformationByHandleEx,
    GetVolumeInformationByHandleW,
};
#[cfg(windows)]
use windows::Win32::System::IO::DeviceIoControl;
#[cfg(windows)]
use windows::Win32::System::Ioctl::{
    FSCTL_GET_INTEGRITY_INFORMATION, FSCTL_GET_INTEGRITY_INFORMATION_BUFFER,
    FSCTL_GET_RETRIEVAL_POINTERS_AND_REFCOUNT, RETRIEVAL_POINTERS_AND_REFCOUNT_BUFFER_0,
    STARTING_VCN_INPUT_BUFFER,
};
#[cfg(windows)]
use windows::core::HRESULT;

/// Return whether the filesystem can identify storage reclaimed by individual files.
pub fn supports_reclaimable_space(path: &Path) -> io::Result<bool> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios", windows))]
    {
        let _ = path;
        Ok(true)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios", windows)))]
    {
        let _ = path;
        Ok(false)
    }
}

/// Return the allocated file data that would be reclaimed by deleting `path` immediately.
///
/// The result excludes data retained by another hardlink, copy-on-write clone, or snapshot.
/// Filesystem metadata is not included.
pub fn reclaimable_space(path: &Path, metadata: &std::fs::Metadata) -> io::Result<u64> {
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
        apple_reclaimable_space(path)
    }

    #[cfg(target_os = "linux")]
    {
        linux_reclaimable_space(path)
    }

    #[cfg(windows)]
    {
        windows_reclaimable_space(path)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios", windows)))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "per-file space measurement is unsupported on this platform",
        ))
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[expect(unsafe_code)]
fn apple_reclaimable_space(path: &Path) -> io::Result<u64> {
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
        return Err(io::Error::last_os_error());
    }

    let returned_fork_attributes =
        u32::from_ne_bytes(response[20..24].try_into().map_err(io::Error::other)?);
    if returned_fork_attributes & libc::ATTR_CMNEXT_PRIVATESIZE == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the filesystem does not report private file sizes",
        ));
    }

    let private_size = i64::from_ne_bytes(response[24..32].try_into().map_err(io::Error::other)?);
    u64::try_from(private_size).map_err(io::Error::other)
}

#[cfg(target_os = "linux")]
#[expect(unsafe_code)]
fn linux_reclaimable_space(path: &Path) -> io::Result<u64> {
    const FIEMAP_EXTENT_LAST: u32 = 0x0000_0001;
    const FIEMAP_EXTENT_UNKNOWN: u32 = 0x0000_0002;
    const FIEMAP_EXTENT_DELALLOC: u32 = 0x0000_0004;
    const FIEMAP_EXTENT_ENCODED: u32 = 0x0000_0008;
    const FIEMAP_EXTENT_NOT_ALIGNED: u32 = 0x0000_0100;
    const FIEMAP_EXTENT_SHARED: u32 = 0x0000_2000;
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

    const FS_IOC_FIEMAP: rustix::ioctl::Opcode =
        rustix::ioctl::opcode::read_write::<Fiemap>(b'f', 11);

    let file = fs_err::File::open(path)?;
    let mut reclaimable = 0_u64;
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
                rustix::ioctl::Updater::<FS_IOC_FIEMAP, _>::new(&mut request),
            )?;
        }

        let mapped_extents =
            usize::try_from(request.header.mapped_extents).map_err(io::Error::other)?;
        if mapped_extents > request.extents.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned more extents than requested",
            ));
        }
        if mapped_extents == 0 {
            return Ok(reclaimable);
        }

        for extent in &request.extents[..mapped_extents] {
            if extent.flags & FIEMAP_EXTENT_DELALLOC != 0 {
                continue;
            }

            if extent.flags
                & (FIEMAP_EXTENT_UNKNOWN | FIEMAP_EXTENT_ENCODED | FIEMAP_EXTENT_NOT_ALIGNED)
                != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "the filesystem cannot report the physical size of an extent",
                ));
            }

            if extent.flags & FIEMAP_EXTENT_SHARED == 0 {
                reclaimable = reclaimable.saturating_add(extent.length);
            }
        }

        let last_extent = &request.extents[mapped_extents - 1];
        if last_extent.flags & FIEMAP_EXTENT_LAST != 0 {
            return Ok(reclaimable);
        }

        let next = last_extent.logical.saturating_add(last_extent.length);
        if next <= start {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the filesystem returned a non-advancing extent",
            ));
        }
        start = next;
    }
}

#[cfg(windows)]
#[expect(unsafe_code)]
fn windows_reclaimable_space(path: &Path) -> io::Result<u64> {
    const FILE_SUPPORTS_BLOCK_REFCOUNTING: u32 = 0x0800_0000;

    let file = fs_err::File::open(path)?;
    let handle = HANDLE(file.as_raw_handle());
    let mut information = FILE_STANDARD_INFO::default();

    // SAFETY: `file` remains open during the call, and `information` is a valid, writable
    // `FILE_STANDARD_INFO` buffer whose exact size is passed to Windows.
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            (&raw mut information).cast(),
            u32::try_from(std::mem::size_of::<FILE_STANDARD_INFO>()).map_err(io::Error::other)?,
        )?;
    }

    if information.NumberOfLinks > 1 {
        return Ok(0);
    }

    let allocated = u64::try_from(information.AllocationSize).map_err(io::Error::other)?;
    if allocated == 0 {
        return Ok(0);
    }

    let mut filesystem_flags = 0;

    // SAFETY: `file` remains open during the call, and `filesystem_flags` points to a valid,
    // writable output value. All other optional output values are omitted.
    unsafe {
        GetVolumeInformationByHandleW(
            handle,
            None,
            None,
            None,
            Some(&raw mut filesystem_flags),
            None,
        )?;
    }

    if filesystem_flags & FILE_SUPPORTS_BLOCK_REFCOUNTING == 0 {
        return Ok(allocated);
    }

    refs_reclaimable_space(handle)
}

/// Sum the exclusively owned allocated extents of a file on ReFS.
#[cfg(windows)]
#[expect(unsafe_code)]
fn refs_reclaimable_space(handle: HANDLE) -> io::Result<u64> {
    const MAX_EXTENTS: usize = 32;

    #[derive(Default)]
    #[repr(C)]
    struct RefcountBuffer {
        extent_count: u32,
        starting_vcn: i64,
        extents: [RETRIEVAL_POINTERS_AND_REFCOUNT_BUFFER_0; MAX_EXTENTS],
    }

    let mut integrity = FSCTL_GET_INTEGRITY_INFORMATION_BUFFER::default();
    let mut bytes_returned = 0;

    // SAFETY: `handle` is valid for the duration of the call, and `integrity` is a writable
    // `FSCTL_GET_INTEGRITY_INFORMATION_BUFFER` whose exact size is passed to Windows.
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_INTEGRITY_INFORMATION,
            None,
            0,
            Some((&raw mut integrity).cast()),
            u32::try_from(std::mem::size_of::<FSCTL_GET_INTEGRITY_INFORMATION_BUFFER>())
                .map_err(io::Error::other)?,
            Some(&raw mut bytes_returned),
            None,
        )?;
    }

    let cluster_size = u64::from(integrity.ClusterSizeInBytes);
    if cluster_size == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ReFS returned a zero-byte cluster size",
        ));
    }

    let mut reclaimable = 0_u64;
    let mut starting_vcn = 0_i64;

    loop {
        let request = STARTING_VCN_INPUT_BUFFER {
            StartingVcn: starting_vcn,
        };
        let mut response = RefcountBuffer::default();
        bytes_returned = 0;

        // SAFETY: `handle` remains valid, `request` is an initialized input buffer, and `response`
        // has the documented header followed by initialized storage for all requested extents.
        let result = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_GET_RETRIEVAL_POINTERS_AND_REFCOUNT,
                Some((&raw const request).cast()),
                u32::try_from(std::mem::size_of::<STARTING_VCN_INPUT_BUFFER>())
                    .map_err(io::Error::other)?,
                Some((&raw mut response).cast()),
                u32::try_from(std::mem::size_of::<RefcountBuffer>()).map_err(io::Error::other)?,
                Some(&raw mut bytes_returned),
                None,
            )
        };

        let has_more = match result {
            Ok(()) => false,
            Err(error) if error.code() == HRESULT::from(ERROR_MORE_DATA) => true,
            Err(error) if error.code() == HRESULT::from(ERROR_HANDLE_EOF) => {
                return Ok(reclaimable);
            }
            Err(error) => return Err(error.into()),
        };

        let extent_count = usize::try_from(response.extent_count).map_err(io::Error::other)?;
        if extent_count > response.extents.len()
            || usize::try_from(bytes_returned).map_err(io::Error::other)?
                < std::mem::offset_of!(RefcountBuffer, extents)
                    + extent_count * std::mem::size_of::<RETRIEVAL_POINTERS_AND_REFCOUNT_BUFFER_0>()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ReFS returned an invalid extent list",
            ));
        }

        let mut current_vcn = response.starting_vcn;
        for extent in &response.extents[..extent_count] {
            let clusters = extent.NextVcn.checked_sub(current_vcn).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ReFS returned unordered extents",
                )
            })?;
            if clusters <= 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ReFS returned a non-advancing extent",
                ));
            }

            if extent.Lcn >= 0 && extent.ReferenceCount == 1 {
                let clusters = u64::try_from(clusters).map_err(io::Error::other)?;
                reclaimable = reclaimable.saturating_add(clusters.saturating_mul(cluster_size));
            }

            current_vcn = extent.NextVcn;
        }

        if !has_more {
            return Ok(reclaimable);
        }
        if current_vcn <= starting_vcn {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ReFS returned a non-advancing extent list",
            ));
        }
        starting_vcn = current_vcn;
    }
}
