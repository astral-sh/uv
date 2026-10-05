//! The subset of Mach-O records used by the dylib editor.
//!
//! Field layouts and command constants follow Apple's `loader.h`:
//! <https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/EXTERNAL_HEADERS/mach-o/loader.h>.
//! Command boundaries and command-relative strings also follow ruby-macho's readers:
//! <https://github.com/Homebrew/ruby-macho/blob/e106f7782df467357d0273c17aacd40df953de66/lib/macho/load_commands.rb>.

use crate::Error;
use crate::bytes::{array, c_string, le32, le64, range};

pub(crate) const HEADER_SIZE: usize = 32;
pub(crate) const SEGMENT_SIZE: usize = 72;
pub(crate) const SECTION_SIZE: usize = 80;
pub(crate) const MH_MAGIC_64: u32 = 0xfeed_facf;
pub(crate) const MH_DYLIB: u32 = 6;
pub(crate) const CPU_TYPE_ARM64: u32 = 0x0100_000c;
pub(crate) const CPU_TYPE_X86_64: u32 = 0x0100_0007;
pub(crate) const SECTION_TYPE: u32 = 0xff;
pub(crate) const S_ZEROFILL: u32 = 1;
pub(crate) const S_GB_ZEROFILL: u32 = 0xc;
pub(crate) const S_THREAD_LOCAL_ZEROFILL: u32 = 0x12;

pub(crate) const LC_SYMTAB: u32 = 0x2;
pub(crate) const LC_DYSYMTAB: u32 = 0xb;
pub(crate) const LC_LOAD_DYLIB: u32 = 0xc;
pub(crate) const LC_ID_DYLIB: u32 = 0xd;
pub(crate) const LC_SUB_FRAMEWORK: u32 = 0x12;
pub(crate) const LC_SUB_UMBRELLA: u32 = 0x13;
pub(crate) const LC_SUB_CLIENT: u32 = 0x14;
pub(crate) const LC_SUB_LIBRARY: u32 = 0x15;
pub(crate) const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
pub(crate) const LC_SEGMENT_64: u32 = 0x19;
pub(crate) const LC_ROUTINES_64: u32 = 0x1a;
pub(crate) const LC_UUID: u32 = 0x1b;
pub(crate) const LC_RPATH: u32 = 0x8000_001c;
pub(crate) const LC_CODE_SIGNATURE: u32 = 0x1d;
pub(crate) const LC_SEGMENT_SPLIT_INFO: u32 = 0x1e;
pub(crate) const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
pub(crate) const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
pub(crate) const LC_DYLD_INFO: u32 = 0x22;
pub(crate) const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
pub(crate) const LC_LOAD_UPWARD_DYLIB: u32 = 0x8000_0023;
pub(crate) const LC_VERSION_MIN_MACOSX: u32 = 0x24;
pub(crate) const LC_FUNCTION_STARTS: u32 = 0x26;
pub(crate) const LC_DATA_IN_CODE: u32 = 0x29;
pub(crate) const LC_SOURCE_VERSION: u32 = 0x2a;
pub(crate) const LC_DYLIB_CODE_SIGN_DRS: u32 = 0x2b;
pub(crate) const LC_LINKER_OPTIMIZATION_HINT: u32 = 0x2e;
pub(crate) const LC_BUILD_VERSION: u32 = 0x32;
pub(crate) const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
pub(crate) const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;

/// Fields used from `mach_header_64`. The remaining fields stay in the original bytes.
pub(crate) struct Header {
    pub(crate) magic: u32,
    pub(crate) cputype: u32,
    pub(crate) filetype: u32,
    pub(crate) ncmds: u32,
    pub(crate) sizeofcmds: u32,
}

impl Header {
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Error> {
        range(0, HEADER_SIZE, data.len())?;

        Ok(Self {
            magic: le32(data, 0)?,
            cputype: le32(data, 4)?,
            filetype: le32(data, 12)?,
            ncmds: le32(data, 16)?,
            sizeofcmds: le32(data, 20)?,
        })
    }
}

/// A bounded load command, retaining its original bytes for serialization.
pub(crate) struct Command<'a> {
    pub(crate) kind: u32,
    pub(crate) data: &'a [u8],
}

impl<'a> Command<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let kind = le32(data, 0)?;
        let size = match kind {
            LC_SEGMENT_64 => SEGMENT_SIZE,
            LC_SYMTAB | LC_UUID | LC_BUILD_VERSION => 24,
            LC_DYSYMTAB => 80,
            LC_DYLD_INFO | LC_DYLD_INFO_ONLY => 48,
            LC_ROUTINES_64 => 72,
            LC_CODE_SIGNATURE
            | LC_SEGMENT_SPLIT_INFO
            | LC_FUNCTION_STARTS
            | LC_DATA_IN_CODE
            | LC_DYLIB_CODE_SIGN_DRS
            | LC_LINKER_OPTIMIZATION_HINT
            | LC_DYLD_EXPORTS_TRIE
            | LC_DYLD_CHAINED_FIXUPS
            | LC_VERSION_MIN_MACOSX
            | LC_SOURCE_VERSION => 16,
            LC_ID_DYLIB | LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB
            | LC_LOAD_UPWARD_DYLIB | LC_LAZY_LOAD_DYLIB => 24,
            LC_RPATH | LC_SUB_FRAMEWORK | LC_SUB_UMBRELLA | LC_SUB_CLIENT | LC_SUB_LIBRARY => 12,
            _ => return Err(Error::Unsupported("load command")),
        };
        range(0, size, data.len())?;

        match kind {
            LC_SEGMENT_64 => {
                // The section table is checked while reading the segment's sections.
            }
            LC_BUILD_VERSION => {
                let tools_size = (le32(data, 20)? as usize)
                    .checked_mul(8)
                    .ok_or(Error::TooLarge)?;
                if range(size, tools_size, data.len())?.end != data.len() {
                    return Err(Error::Malformed("invalid build-tool table"));
                }
            }
            LC_ID_DYLIB | LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB
            | LC_LOAD_UPWARD_DYLIB | LC_LAZY_LOAD_DYLIB | LC_RPATH | LC_SUB_FRAMEWORK
            | LC_SUB_UMBRELLA | LC_SUB_CLIENT | LC_SUB_LIBRARY => {
                let offset = le32(data, 8)? as usize;
                if offset < size {
                    return Err(Error::Malformed("invalid load-command string offset"));
                }
                c_string(data, offset)?;
            }
            _ => {
                if data.len() != size {
                    return Err(Error::Malformed("invalid fixed-size load command"));
                }
            }
        }

        Ok(Self { kind, data })
    }
}

/// Fields used from `segment_command_64`.
#[derive(Clone, Copy)]
pub(crate) struct Segment {
    pub(crate) segname: [u8; 16],
    pub(crate) vmaddr: u64,
    pub(crate) vmsize: u64,
    pub(crate) fileoff: u64,
    pub(crate) filesize: u64,
    pub(crate) nsects: u32,
}

impl Segment {
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Error> {
        range(0, SEGMENT_SIZE, data.len())?;

        Ok(Self {
            segname: array(data, 8)?,
            vmaddr: le64(data, 24)?,
            vmsize: le64(data, 32)?,
            fileoff: le64(data, 40)?,
            filesize: le64(data, 48)?,
            nsects: le32(data, 64)?,
        })
    }
}

/// Fields used from `section_64`, including its file and relocation ranges.
pub(crate) struct Section {
    pub(crate) sectname: [u8; 16],
    pub(crate) segname: [u8; 16],
    pub(crate) addr: u64,
    pub(crate) size: u64,
    pub(crate) offset: u32,
    pub(crate) reloff: u32,
    pub(crate) nreloc: u32,
    pub(crate) flags: u32,
}

impl Section {
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Error> {
        range(0, SECTION_SIZE, data.len())?;

        Ok(Self {
            sectname: array(data, 0)?,
            segname: array(data, 16)?,
            addr: le64(data, 32)?,
            size: le64(data, 40)?,
            offset: le32(data, 48)?,
            reloff: le32(data, 56)?,
            nreloc: le32(data, 60)?,
            flags: le32(data, 64)?,
        })
    }
}
