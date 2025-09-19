use std::{io::{ErrorKind, Result}, path::PathBuf};

use byteorder::ReadBytesExt;
use thiserror::Error;

use uv_fs::Simplified as _;
use uv_warnings::warn_user;

use crate::managed::ManagedPythonInstallation;

// From <mach-o/loader.h> (macOS SDK, XNU source, etc.)
// which is also the source for the header and load command layouts
const MH_MAGIC: u32 = 0xfeedface;
const MH_MAGIC_64: u32 = 0xfeedfacf;
const LC_ID_DYLIB: u32 = 0xd;
const LC_CODE_SIGNATURE: u32 = 0x1d;

// From <kern/cs_blobs.h> (XNU source, also vendored into ld64)
// which is also the source for the code directory layout
const CS_ADHOC: u32 = 0x00000002;
const CS_LINKER_SIGNED: u32 = 0x00020000;
const CSMAGIC_CODEDIRECTORY: u32 = 0xfade0c02;
const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xfade0cc0;
const CSSLOT_CODEDIRECTORY: u32 = 0;
const CSSLOT_ALTERNATE_CODEDIRECTORIES: u32 = 0x1000;
const CSSLOT_ALTERNATE_CODEDIRECTORY_MAX: u32 = 5;
const CSSLOT_ALTERNATE_CODEDIRECTORY_LIMIT: u32 = CSSLOT_ALTERNATE_CODEDIRECTORIES + CSSLOT_ALTERNATE_CODEDIRECTORY_MAX;
const CS_HASHTYPE_SHA1: u32 = 1;
const CS_HASHTYPE_SHA256: u32 =  2;


#[derive(Debug, Copy)]
enum Endian {
    Little,
    Big,
}

/// A very mild convenience reader around Mach-O files, handling / endianness and buffering and
/// collecting a set of modifications. Actual Mach-O parsing beyond the magic is left to the user.
///
/// Note that Mach-O files cannot meaningfully exceed 4 GB because all internal fields are u32, even
/// on 64-bit platforms.
#[derive(Debug)]
struct MachOManipulator {
    path: PathBuf,
    reader: std::io::BufReader<fs_err::File>,
    pub endian: Endian,
    pub offset: u32,
    pub modifications: Vec<(usize, Vec<u8>)>,
}

impl MachOManipulator {
    fn new(path: PathBuf) -> Result<Self> {
        let file = fs_err::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        let magic = reader.read_u32::<byteorder::LittleEndian>()?;
        let endian = if let MH_MAGIC | MH_MAGIC_64 = magic.swap_bytes() {
            Endian::Big
        } else {
            Endian::Little
        };
        reader.seek_relative(-4)?;
        Self { path, reader, endian, offset: 0, modifications: vec![] }
    }

    /// Read a u32 in the current endianness, advancing the cursor.
    fn read_u32(&mut self) -> Result<u32> {
        self.offset += 4;
        match self.endian {
            Endian::Little => self.reader.read_u32::<byteorder::LittleEndian>()?,
            Endian::Big => self.reader.read_u32::<byteorder::BigEndian>()?,
        }
    }

    /// Read several u32s in the current endianness, advancing the cursor.
    fn read_u32s<const N: usize>(&mut self) -> Result<[u32; N]> {
        self.offset += 4 * N;
        let mut result = [0; N];
        match self.endian {
            Endian::Little => self.reader.read_u32_into::<byteorder::LittleEndian>(&mut result)?,
            Endian::Big => self.reader.read_u32_into::<byteorder::BigEndian>(&mut result)?,
        };
        result
    }

    /// Read a fixed number of bytes, advancing the cursor.
    fn read_u8s<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.offset += N;
        let mut result = [0; N];
        self.reader.read_exact(&mut result)?;
        result
    }

    /// Read bytes into an allocated vec, advancing the cursor.
    fn read(&mut self, n: usize) -> Result<Vec<u8>> {
        self.offset += n;
        let mut result = vec![0; n];
        self.reader.read_exact(&result[..])?;
        result
    }

    /// Read from a specific offset, without affecting the regular offset.
    fn read_at(&mut self, buf: &mut [u8], offset: u64) -> Result<usize> {
        self.reader.get_mut().read_at(buf, offset)
    }

    /// Move the cursor (which invalidates the read buffer).
    fn seek(&mut self, offset: usize) -> Result<()> {
        self.offset = offset;
        self.reader.seek(offset)?;
        Ok(())
    }

    /// Move the cursor, trying not to invalidate the read buffer.
    fn seek_relative(&mut self, delta: isize) -> Result<()> {
        self.offset += delta;
        self.reader.seek_relative(offset)
    }

    /// Write out the modifications to disk.
    ///
    /// This creates a new inode (to avoid issues with caching code signatures) and replaces the
    /// original file atomically.
    fn write(self) -> Result<()> {
        let MachOManipulator { path, reader, modifications, .. } = self;
        let perms = reader.get_mut().metadata()?.permissions();
        drop(reader);

        let newfile = tempfile::NamedTempfile::with_prefix_in("uv-python-", path.parent())?;
        let newpath = newfile.into_temp_path.keep()?;
        reflink::reflink_or_copy(self.path, newpath)?;

        let file = fs_err::File::options()
            .write(true)
            .open(&newpath)?;
        file.set_permissions(perms)?;
        for (offset, contents) in modifications {
            file.write_at(contents, offset)?;
        }
        drop(file);

        fs_err::rename(newpath, self.path)?;
        Ok(())
    }
}

/// A hash that can show up in a Mach-O code signature.
enum CodeSignHash {
    Sha1([u8; 20]),
    Sha256([u8; 32]),
}

impl CodeSignHash {
    fn new(hash_type: u8, hash_size: u8) -> Result<Self> {
        match hash_type, hash_size {
            CS_HASHTYPE_SHA1, 20 => Ok(Self::Sha1([0; 20])),
            CS_HASHTYPE_SHA256, 32 => Ok(Self::Sha256([0; 32])),
            _ => {
                debug!("Unknown hash {hash_type} of size {hash_size}");
                Err(MalformedMachO)
            }
        }
    }

    fn digest(&self, data: &[u8]) -> &[u8] {
        match self {
            Self::Sha1(mut buffer) => {
                let hasher = Sha1::new_with_prefix(data);
                hasher.finalize_into(buffer);
                &*buffer
            }
            Self::Sha256(mut buffer) => {
                let hasher = Sha256::new_with_prefix(data);
                hasher.finalize_into(buffer);
                &*buffer
            }
        }
    }
}

/// A specific loaded page for use by struct Pages.
struct Page {
    index: u32,
    contents: Vec<u8>,
}

/// An in-memory edited view of pages in a Mach-O file with a given page size.
///
/// This is basically a write-back cache.
struct Pages {
    file: &mut MachOManipulator,
    pages: Vec<Page>,
    page_shift: u32,
}

impl Pages {
    fn new(file: &mut MachOManipulator, page_shift: u32) -> Result<Self> {
        if page_shift == 0 {
            // This means the page size is "infinite" which I think is one big page? The kernel
            // doesn't allow this anyway so we reject it for now. We could support this if needed,
            // double-check how other tools in Apple OSS drops interpret this case.
            debug!("pageSize is zero");
            return Err(MalformedMachO);
        }

        Ok(Self { file, pages: vec![], page_shift })
    }

    fn page_length(&self) -> u32 {
        1 << self.page_shift
    }

    fn page_mask(&self) -> u32 {
        ~(self.page_length() - 1)
    }

    /// Return the page containing the given byte offset, paging it in if needed.
    fn page_at(&mut self, offset: usize) -> Result<&mut Vec<u8>> {
        let index = offset >> self.page_shift;
        if let Some(page) = match self.pages.iter_mut().find(|&x| x.index == index) {
            return page.contents;
        }

        let mut contents = vec![0u8; self.page_length()];
        // read_exact_at is documented as leaving the _entire_ contents of the buffer unspecified if
        // it hits EOF before filling. We specifically are okay with an incomplete last page though.
        let mut n = 0;
        while n < contents.len() {
            match self.file.read_at(&mut buf[n..], offset + n) {
                Ok(0) => break,
                Ok(m) => {n += m;}
                Err(ref e) if e.is_interrupted() => {},
                Err(e) => return Err(e),
            }
        }
        contents.truncate(n);
        let page = Page { index, contents };
        self.pages.push(page);
        self.pages.last_mut().contents
    }
        
    fn modify(&mut self, offset: usize, contents: &[u8]) -> Result<()> {
        let (offset, contents) = modification;
        let mut page = self.page_at(offset)?;
        let relative_start = offset & self.page_mask;
        let relative_end = relative_start + contents.len();

        let contents = if relative_end > self.page_length() {
            let (ours, excess) = contents.split_at(self.page_length() - relative_start);
            self.modify(offset.next_multiple_of(self.page_length()), excess)?;
            ours
        } else {
            contents
        };
        let relative_end = relative_start + contents.len();
        if relative_end > page.len() {
            // This can only happen if we have a short page
            page.resize(relative_end, b'\0');
        }
        &mut page[relative_start..relative_end].copy_from_slice(contents);
        Ok(())
    }

    /// Apply all pending modifications and return the modified pages.
    pub fn get_modified_pages(self) -> Vec<Page> {
        for (offset, contents) in &self.file.modifications {
            self.modify(offset, contents.as_ref());
        }
        self.pages
    }
}
    

fn patch_dylib_install_name_ourselves(dylib: PathBuf) -> Result<(), Error> {
    // In theory, this is straightforward: find the LC_ID_DYLIB load command, make sure there's
    // enough room for the updated string, and overwrite the string.
    //
    // However, on recent versions of macOS on Apple silicon (at least), there's a requirement for
    // every binary to have a valid LC_CODE_SIGNATURE. This doesn't need to be an actual
    // _signature_; it can be an "ad-hoc signature" consisting of just hashes of the file. The
    // Apple-provided linker, as well as other tools that output binaries like install_name_tool and
    // strip, generate an ad-hoc signature and set the "linker-signed" flag, indicating there is
    // nothing interesting about the signature (like entitlements) you should worry about losing if
    // the signature is regenerated. We need to do the same thing.
    //
    // While there is a platform library (libcodedirectory.dylib) to generate the signature, and the
    // source is public (it's vendored into ld64 on Apple's OSS drops), it's geared towards
    // generating a signature from scratch when you understand the full binary; it doesn't have a
    // method to load and modify an existing signature, keeping all the other settings the same.
    // Fortunately, the parts of the signature we need to understand aren't hard:
    // - The signature is located at the end of the file, pointed to by an LC_CODE_SIGNATURE linker
    //   command.
    // - All the pages in the file up to that signature are hashed, one page at a time.
    // - These hashes are stored, along with other info, in a "code directory" data structure.
    // - The code directory is one of several possible "blobs" in the signature. There may be
    //   multiple if you want to support multiple hash types; there may be other data like
    //   entitlements or an actual signature.
    // - These blobs are themselves stored in a "superblob" data structure, which is what is placed
    //   at the end of the file.
    // So, once we know what page(s) we modified while editing LC_ID_DYLIB, we rehash those pages,
    // find where they're located in the code directory, and overwrite the hashes. Because we're
    // requiring that the file is linker-signed, nothing attests to the hashes and we don't need to
    // make further changes.
    //
    // TODO(geofft): Support universal Mach-O files, there are some good arguments for using them

    let f = MachOManipulator::new(dylib)?;

    let mut found_id_dylib = None;
    let mut found_code_signature = None;

    // Read the Mach-O header
    let [magic, _cputype, _cpusubtype, _filetype, ncmds, sizeofcmds, _flags] = f.read_u32s()?;
    match magic {
        MH_MAGIC => {},
        MH_MAGIC_64 => {
            let _reserved = f.read_u32()?;
        },
        _ => {
            return Err(NotMachO);
        },
    };

    // Just to check that we didn't misparse anything, we will check after reading the load commands
    // that we read sizecmds bytes.
    let expected_offset_after_load_commands = f.offset + sizeofcmds;

    // Read the load commands
    for i in 0..ncmds {
        let [cmd, cmdsize] = f.read_u32s()?;
        if cmd == LC_ID_DYLIB {
            let [name_offset, _timestamp, _current_version, _compat_version] = f.read_u32s()?;
            if name_offset != 24 or cmdsize <= 24 {
                debug!("Malformed LC_ID_DYLIB, size {cmdsize}, name offset {name_offest}");
                return Err(MalformedMachO);
            }
            if found_id_dylib.is_some() {
                debug("Already fond an LC_ID_DYLIB");
                return Err(MalformedMachO);
            }
            namelen = cmdsize - name_offset;
            found_id_dylib = Some((f.offset, namelen));
            f.seek_relative(namelen)?;
        } else if cmd == LC_CODE_SIGNATURE {
            let [dataoff, datasize] = f.read_u32s()?;
            if found_code_signature.is_some() {
                debug("Already fond an LC_CODE_SIGNATURE");
                return Err(MalformedMachO);
            }
            found_code_signature = Some((dataoff, datasize));
        } else {
            f.seek_relative(cmdsize - 8)?;
        }
    }

    if f.offset != expected_offset_after_load_commands {
        debug!("At offset {}, expected to be at {expected_offset_after_load_commands}", f.offset);
        return Err(MalformedMachO);
    }

    // Rewrite LC_ID_DYLIB
    let Some(offset, bufsize) = found_id_dylib else {
        // TODO(geofft): We could add one between the last load command and the start of the first
        // segment if there's room, it seems like that's not hard.
        return Err(NoInstallName);
    }
    let mut data = Vec::new(dylib.as_bytes());
    if data.len() < bufsize {
        return Err(NotEnoughPadding(data.len(), bufsize));
    }
    data.resize(bufsize, b'\0');
    f.modify(offset, data);

    // Rewrite the code signature
    // All ints in code signatures are network byte order.
    f.endian = Endian::Big;
    if let Some(dataoff, datasize) = found_code_signature {
        f.seek(std::io::SeekFrom::Start(dataoff))?;

        // Read the "superblob", which tells us how many blobs there are
        // and where they are.
        let superblob_start = f.offset;
        let [magic, _len, nblobs] = f.read_u32s()?;
        if magic != CSMAGIC_EMBEDDED_SIGNATURE {
            debug!("superblob magic {magic:x}");
            return Err(MalformedMachO);
        }
        let blobs: Vec<_> = (0..nblobs).map(|_| {
            let [typ, offset] = f.read_u32s()?;
            (typ, offset)
        }).collect();

        // Rewind to the start of the superblob so relative offsets
        // work. This is an invariant at the top of the loop.
        f.seek_relative(-4 * (3 + 2 * nblobs))?;
        for (typ, offset) in blobs {
            // We are only interested in code directories.
            let CSSLOT_CODE_DIRECTORY | CSSLOT_ALTERNATE_CODEDIRECTORIES..CSSLOT_ALTERNATE_CODEDIRECTORY_LIMIT = typ else continue;
            f.seek_relative(offset)?;
            let magic = f.read_u32()?;
            if magic != CSMAGIC_CODEDIRECTORY {
                debug!("blob magic {magic:x}");
                return Err(MalformedMachO);
            }

            // Read the rest of the code directory structure
            // (See cs_blobs.h in XNU or ld64 source)
            let [_length, _version, flags, hashOffset, _identOffset, _nSpecialSlots, nCodeSlots, codeLimit] = f.read_u32s();
            // Note that pageSize is a shift, i.e. the log2 of the
            // actual page size. Also note that in practice it's 12
            // (4096) even on machines using bigger page sizes.
            let [hashSize, hashType, platform, pageSize] = f.read_u8s()?;

            if (flags & CS_ADHOC) == 0 || (flags & CS_LINKER_SIGNED) == 0 {
                return Err(NotLinkerSigned);
            }
            let cshash = CodeSignHash::new(hashType, hashSize)?;
                
            // Rehash the pages we've modified
            let pages = Pages::new(&mut f, pageSize);
            for Page { index, contents } in pages.get_modified_pages() {
                if index >= nCodeSlots {
                    debug!("Page {index} isn't signed??");
                    continue;
                }
                f.modify(offset + hashOffset + index * hashSize, cshash.digest(contents));
            }
        }
    }

    f.write()?;

    Ok(())
}

pub fn patch_dylib_install_name(dylib: PathBuf) -> Result<(), Error> {
    let output = match std::process::Command::new("install_name_tool")
        .arg("-id")
        .arg(&dylib)
        .arg(&dylib)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            let e = if e.kind() == ErrorKind::NotFound {
                Error::MissingInstallNameTool
            } else {
                e.into()
            };
            return Err(e);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(Error::RenameError { dylib, stderr });
    }

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("`install_name_tool` is not available on this system.
This utility is part of macOS Developer Tools. Please ensure that the Xcode Command Line Tools are installed by running:

    xcode-select --install

For more information, see: https://developer.apple.com/xcode/")]
    MissingInstallNameTool,
    #[error("Failed to update the install name of the Python dynamic library located at `{}`", dylib.user_display())]
    RenameError { dylib: PathBuf, stderr: String },
    #[error("Library is not a Mach-O file")]
    NotMachO,
    #[error("Mach-O file could not be parsed")]
    MalformedMachO,
    #[error("Library does not have an existing install name (LC_ID_DYLIB) to edit")
    NoInstallName,
    #[error("Cannot fit an install name of {0} bytes in to a buffer of {1} bytes")]
    NotEnoughPadding(usize, u32),


}

impl Error {
    /// Emit a user-friendly warning about the patching failure.
    pub fn warn_user(&self, installation: &ManagedPythonInstallation) {
        let error = if tracing::enabled!(tracing::Level::DEBUG) {
            format!("\nUnderlying error: {self}")
        } else {
            String::new()
        };
        warn_user!(
            "Failed to patch the install name of the dynamic library for {}. This may cause issues when building Python native extensions.{}",
            installation.executable(false).simplified_display(),
            error
        );
    }
}
