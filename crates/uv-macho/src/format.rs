//! The subset of Mach-O records used by the dylib editor.
//!
//! Field layouts and command constants follow Apple's `loader.h`:
//! <https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/EXTERNAL_HEADERS/mach-o/loader.h>.
//! Command boundaries and command-relative strings also follow ruby-macho's readers:
//! <https://github.com/Homebrew/ruby-macho/blob/e106f7782df467357d0273c17aacd40df953de66/lib/macho/load_commands.rb>.

use std::ops::Range;

use crate::Error;
use crate::bytes::{array, c_string, slice};

pub(crate) const HEADER_SIZE: usize = 32;
const SEGMENT_SIZE: usize = 72;
const SECTION_SIZE: usize = 80;
const NLIST_64_SIZE: u32 = 16;
const DYLIB_TABLE_OF_CONTENTS_SIZE: u32 = 8;
const DYLIB_MODULE_64_SIZE: u32 = 56;
const DYLIB_REFERENCE_SIZE: u32 = 4;
const INDIRECT_SYMBOL_SIZE: u32 = 4;
const RELOCATION_INFO_SIZE: u32 = 8;
pub(crate) const MH_MAGIC_64: u32 = 0xfeed_facf;
pub(crate) const MH_DYLIB: u32 = 6;
pub(crate) const CPU_TYPE_ARM64: u32 = 0x0100_000c;
pub(crate) const CPU_TYPE_X86_64: u32 = 0x0100_0007;
pub(crate) const SECTION_TYPE: u32 = 0xff;
pub(crate) const S_ZEROFILL: u32 = 1;
pub(crate) const S_GB_ZEROFILL: u32 = 0xc;
pub(crate) const S_THREAD_LOCAL_ZEROFILL: u32 = 0x12;

pub(crate) const SEG_TEXT: [u8; 16] = *b"__TEXT\0\0\0\0\0\0\0\0\0\0";
pub(crate) const SEG_LINKEDIT: [u8; 16] = *b"__LINKEDIT\0\0\0\0\0\0";
pub(crate) const SECT_INFO_PLIST: [u8; 16] = *b"__info_plist\0\0\0\0";

const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xb;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_ID_DYLIB: u32 = 0xd;
const LC_SUB_FRAMEWORK: u32 = 0x12;
const LC_SUB_UMBRELLA: u32 = 0x13;
const LC_SUB_CLIENT: u32 = 0x14;
const LC_SUB_LIBRARY: u32 = 0x15;
const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
const LC_SEGMENT_64: u32 = 0x19;
const LC_ROUTINES_64: u32 = 0x1a;
const LC_UUID: u32 = 0x1b;
const LC_RPATH: u32 = 0x8000_001c;
pub(crate) const LC_CODE_SIGNATURE: u32 = 0x1d;
const LC_SEGMENT_SPLIT_INFO: u32 = 0x1e;
const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x8000_0023;
const LC_VERSION_MIN_MACOSX: u32 = 0x24;
const LC_FUNCTION_STARTS: u32 = 0x26;
const LC_DATA_IN_CODE: u32 = 0x29;
const LC_SOURCE_VERSION: u32 = 0x2a;
const LC_DYLIB_CODE_SIGN_DRS: u32 = 0x2b;
const LC_LINKER_OPTIMIZATION_HINT: u32 = 0x2e;
const LC_BUILD_VERSION: u32 = 0x32;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;

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
        let data = &array::<HEADER_SIZE>(data, 0)?;

        Ok(Self {
            magic: u32::from_le_bytes(array(data, 0)?),
            cputype: u32::from_le_bytes(array(data, 4)?),
            filetype: u32::from_le_bytes(array(data, 12)?),
            ncmds: u32::from_le_bytes(array(data, 16)?),
            sizeofcmds: u32::from_le_bytes(array(data, 20)?),
        })
    }
}

/// A bounded load command, retaining its original bytes for serialization.
pub(crate) enum Command<'a> {
    Segment(SegmentCommand<'a>),
    InstallId(&'a [u8]),
    CodeSignature(LinkeditData<'a>),
    DeploymentTarget(&'a [u8]),
    SymbolTable(SymbolTable<'a>),
    DynamicSymbolTable(DynamicSymbolTable<'a>),
    DyldInfo(DyldInfo<'a>),
    LinkeditData(LinkeditData<'a>),
    Other(&'a [u8]),
}

impl<'a> Command<'a> {
    pub(crate) fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let kind = u32::from_le_bytes(array(data, 0)?);
        Ok(match kind {
            LC_SEGMENT_64 => {
                slice(data, 0, SEGMENT_SIZE)?;
                Self::Segment(SegmentCommand(data))
            }
            LC_SYMTAB => Self::SymbolTable(SymbolTable(fixed(data)?)),
            LC_DYSYMTAB => Self::DynamicSymbolTable(DynamicSymbolTable(fixed(data)?)),
            LC_DYLD_INFO | LC_DYLD_INFO_ONLY => Self::DyldInfo(DyldInfo(fixed(data)?)),
            LC_CODE_SIGNATURE => Self::CodeSignature(LinkeditData(fixed(data)?)),
            LC_SEGMENT_SPLIT_INFO
            | LC_FUNCTION_STARTS
            | LC_DATA_IN_CODE
            | LC_DYLIB_CODE_SIGN_DRS
            | LC_LINKER_OPTIMIZATION_HINT
            | LC_DYLD_EXPORTS_TRIE
            | LC_DYLD_CHAINED_FIXUPS => Self::LinkeditData(LinkeditData(fixed(data)?)),
            LC_VERSION_MIN_MACOSX => Self::DeploymentTarget(fixed::<16>(data)?),
            LC_BUILD_VERSION => {
                let header = slice(data, 0, 24)?;
                let tools_size = (u32::from_le_bytes(array(header, 20)?) as usize)
                    .checked_mul(8)
                    .ok_or(Error::TooLarge)?;
                let tools = slice(data, header.len(), tools_size)?;
                if header.len() + tools.len() != data.len() {
                    return Err(Error::Malformed("invalid build-tool table"));
                }
                let platform = u32::from_le_bytes(array(header, 8)?);
                if platform != 1 {
                    return Err(Error::UnsupportedValue {
                        field: "build platform",
                        value: u64::from(platform),
                    });
                }
                Self::DeploymentTarget(data)
            }
            LC_ID_DYLIB => {
                command_string(data, 24)?;
                Self::InstallId(data)
            }
            LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB | LC_LOAD_UPWARD_DYLIB
            | LC_LAZY_LOAD_DYLIB => {
                command_string(data, 24)?;
                Self::Other(data)
            }
            LC_RPATH | LC_SUB_FRAMEWORK | LC_SUB_UMBRELLA | LC_SUB_CLIENT | LC_SUB_LIBRARY => {
                command_string(data, 12)?;
                Self::Other(data)
            }
            LC_SOURCE_VERSION => Self::Other(fixed::<16>(data)?),
            LC_UUID => Self::Other(fixed::<24>(data)?),
            LC_ROUTINES_64 => Self::Other(fixed::<72>(data)?),
            _ => {
                return Err(Error::UnsupportedValue {
                    field: "load command",
                    value: u64::from(kind),
                });
            }
        })
    }

    pub(crate) fn as_bytes(&self) -> &'a [u8] {
        match self {
            Self::Segment(SegmentCommand(data))
            | Self::InstallId(data)
            | Self::DeploymentTarget(data)
            | Self::Other(data) => data,
            Self::CodeSignature(LinkeditData(data)) | Self::LinkeditData(LinkeditData(data)) => {
                data.as_slice()
            }
            Self::SymbolTable(SymbolTable(data)) => data.as_slice(),
            Self::DynamicSymbolTable(DynamicSymbolTable(data)) => data.as_slice(),
            Self::DyldInfo(DyldInfo(data)) => data.as_slice(),
        }
    }
}

/// Read a fixed-size load command without copying it.
fn fixed<const N: usize>(data: &[u8]) -> Result<&[u8; N], Error> {
    let header = data
        .first_chunk::<N>()
        .ok_or(Error::Malformed("range extends past its containing data"))?;
    if data.len() != N {
        return Err(Error::Malformed("invalid fixed-size load command"));
    }
    Ok(header)
}

/// Validate a string whose offset is relative to the beginning of its command.
fn command_string(data: &[u8], header_size: usize) -> Result<(), Error> {
    let header = slice(data, 0, header_size)?;
    let offset = u32::from_le_bytes(array(header, 8)?) as usize;
    if offset < header_size {
        return Err(Error::Malformed("invalid load-command string offset"));
    }
    c_string(data, offset)?;
    Ok(())
}

pub(crate) struct SymbolTable<'a>(&'a [u8; 24]);

impl SymbolTable<'_> {
    pub(crate) fn file_ranges(&self) -> Result<[Range<u64>; 2], Error> {
        let symbols = file_table(self.0, 8, NLIST_64_SIZE)?;
        let strings = file_table(self.0, 16, 1)?;
        Ok([symbols, strings])
    }
}

pub(crate) struct DynamicSymbolTable<'a>(&'a [u8; 80]);

impl DynamicSymbolTable<'_> {
    pub(crate) fn file_ranges(&self) -> Result<[Range<u64>; 6], Error> {
        let table_of_contents = file_table(self.0, 32, DYLIB_TABLE_OF_CONTENTS_SIZE)?;
        let modules = file_table(self.0, 40, DYLIB_MODULE_64_SIZE)?;
        let references = file_table(self.0, 48, DYLIB_REFERENCE_SIZE)?;
        let indirect_symbols = file_table(self.0, 56, INDIRECT_SYMBOL_SIZE)?;
        let external_relocations = file_table(self.0, 64, RELOCATION_INFO_SIZE)?;
        let local_relocations = file_table(self.0, 72, RELOCATION_INFO_SIZE)?;
        Ok([
            table_of_contents,
            modules,
            references,
            indirect_symbols,
            external_relocations,
            local_relocations,
        ])
    }
}

pub(crate) struct DyldInfo<'a>(&'a [u8; 48]);

impl DyldInfo<'_> {
    pub(crate) fn file_ranges(&self) -> Result<[Range<u64>; 5], Error> {
        let rebase = file_table(self.0, 8, 1)?;
        let bind = file_table(self.0, 16, 1)?;
        let weak_bind = file_table(self.0, 24, 1)?;
        let lazy_bind = file_table(self.0, 32, 1)?;
        let export = file_table(self.0, 40, 1)?;
        Ok([rebase, bind, weak_bind, lazy_bind, export])
    }
}

pub(crate) struct LinkeditData<'a>(&'a [u8; 16]);

impl LinkeditData<'_> {
    pub(crate) fn file_range(&self) -> Result<Range<u64>, Error> {
        file_table(self.0, 8, 1)
    }
}

/// Read the file range of a table whose offset and entry count are adjacent fields.
fn file_table(data: &[u8], field: usize, entry_size: u32) -> Result<Range<u64>, Error> {
    let offset = u64::from(u32::from_le_bytes(array(data, field)?));
    let count = u64::from(u32::from_le_bytes(array(data, field + 4)?));
    // The offset, count, and entry size are all u32, so the end offset fits in u64.
    Ok(offset..offset + count * u64::from(entry_size))
}

/// A segment header and its complete table of fixed-size section records.
#[derive(Clone, Copy)]
pub(crate) struct Segment<'a> {
    pub(crate) segname: [u8; 16],
    pub(crate) vmaddr: u64,
    pub(crate) vmsize: u64,
    pub(crate) fileoff: u64,
    pub(crate) filesize: u64,
    pub(crate) sections: &'a [[u8; SECTION_SIZE]],
}

pub(crate) struct SegmentCommand<'a>(&'a [u8]);

impl<'a> SegmentCommand<'a> {
    pub(crate) fn parse(&self) -> Result<Segment<'a>, Error> {
        let data = self.0;
        let (header, sections) = data
            .split_at_checked(SEGMENT_SIZE)
            .ok_or(Error::Malformed("range extends past its containing data"))?;
        let (sections, trailing) = sections.as_chunks::<SECTION_SIZE>();
        if !trailing.is_empty() || sections.len() != u32::from_le_bytes(array(header, 64)?) as usize
        {
            return Err(Error::Malformed("invalid segment section table"));
        }

        Ok(Segment {
            segname: array(header, 8)?,
            vmaddr: u64::from_le_bytes(array(header, 24)?),
            vmsize: u64::from_le_bytes(array(header, 32)?),
            fileoff: u64::from_le_bytes(array(header, 40)?),
            filesize: u64::from_le_bytes(array(header, 48)?),
            sections,
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
    pub(crate) relocations: Range<u64>,
    pub(crate) flags: u32,
}

impl Section {
    pub(crate) fn parse(data: &[u8; SECTION_SIZE]) -> Result<Self, Error> {
        Ok(Self {
            sectname: array(data, 0)?,
            segname: array(data, 16)?,
            addr: u64::from_le_bytes(array(data, 32)?),
            size: u64::from_le_bytes(array(data, 40)?),
            offset: u32::from_le_bytes(array(data, 48)?),
            relocations: file_table(data, 56, RELOCATION_INFO_SIZE)?,
            flags: u32::from_le_bytes(array(data, 64)?),
        })
    }
}
