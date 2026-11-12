use std::ffi::CStr;
use std::ops::Range;

use goblin::mach::constants::cputype::{CPU_TYPE_ARM64, CPU_TYPE_X86_64};
use goblin::mach::constants::{S_GB_ZEROFILL, S_THREAD_LOCAL_ZEROFILL, S_ZEROFILL, SECTION_TYPE};
use goblin::mach::header::{Header64, MH_DYLIB, MH_MAGIC_64, SIZEOF_HEADER_64};
use goblin::mach::load_command::{
    BuildToolVersion, BuildVersionCommand, DyldInfoCommand, DysymtabCommand, LC_BUILD_VERSION,
    LC_CODE_SIGNATURE, LC_DATA_IN_CODE, LC_DYLD_CHAINED_FIXUPS, LC_DYLD_EXPORTS_TRIE, LC_DYLD_INFO,
    LC_DYLD_INFO_ONLY, LC_DYLIB_CODE_SIGN_DRS, LC_DYSYMTAB, LC_FUNCTION_STARTS, LC_ID_DYLIB,
    LC_LAZY_LOAD_DYLIB, LC_LINKER_OPTIMIZATION_HINT, LC_LOAD_DYLIB, LC_LOAD_UPWARD_DYLIB,
    LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB, LC_ROUTINES_64, LC_RPATH, LC_SEGMENT_64,
    LC_SEGMENT_SPLIT_INFO, LC_SOURCE_VERSION, LC_SUB_CLIENT, LC_SUB_FRAMEWORK, LC_SUB_LIBRARY,
    LC_SUB_UMBRELLA, LC_SYMTAB, LC_UUID, LC_VERSION_MIN_MACOSX, LinkeditDataCommand,
    LoadCommandHeader, PLATFORM_MACOS, RoutinesCommand64, SIZEOF_LINKEDIT_DATA_COMMAND,
    SIZEOF_SECTION_64, SIZEOF_SEGMENT_COMMAND_64, Section64, SegmentCommand64,
    SourceVersionCommand, SymtabCommand, UuidCommand, VersionMinCommand,
};
use scroll::ctx::{SizeWith, TryFromCtx};
use scroll::{Endian, LE, Pread};

use crate::Error;
use crate::bytes::{array, c_string, slice};
use crate::regions::sort_disjoint;
use crate::signature::Signature;

const SEG_TEXT: [u8; 16] = *b"__TEXT\0\0\0\0\0\0\0\0\0\0";
const SEG_LINKEDIT: [u8; 16] = *b"__LINKEDIT\0\0\0\0\0\0";
const DYLIB_HEADER_SIZE: usize = 24;
const NLIST_64_SIZE: u32 = 16;
const DYLIB_TABLE_OF_CONTENTS_SIZE: u32 = 8;
const DYLIB_MODULE_64_SIZE: u32 = 56;
const DYLIB_REFERENCE_SIZE: u32 = 4;
const INDIRECT_SYMBOL_SIZE: u32 = 4;
const RELOCATION_SIZE: u32 = 8;

/// The ranges and command offsets needed to edit a validated dylib.
pub(crate) struct Layout<'a> {
    image: &'a [u8],
    command_end: usize,
    header_limit: usize,
    code_limit: usize,
    install_id: Range<usize>,
    signature_command: Option<usize>,
    signature: Option<&'a [u8]>,
    text_size: u64,
    linkedit: (usize, SegmentCommand64),
    segment_alignment: usize,
}

impl<'a> Layout<'a> {
    pub(crate) fn parse(image: &'a [u8]) -> Result<Self, Error> {
        let header = read::<Header64>(image)?;
        if header.magic != MH_MAGIC_64 || header.filetype != MH_DYLIB {
            return Err(Error::Unsupported(
                "expected a thin little-endian 64-bit dylib",
            ));
        }

        let segment_alignment = match header.cputype {
            CPU_TYPE_ARM64 => 16384,
            CPU_TYPE_X86_64 => 4096,
            _ => {
                return Err(Error::UnsupportedValue {
                    field: "CPU architecture",
                    value: u64::from(header.cputype),
                });
            }
        };

        let mut commands = slice(image, SIZEOF_HEADER_64, header.sizeofcmds as usize)?;
        let command_end = SIZEOF_HEADER_64 + commands.len();
        if header.ncmds as usize > commands.len() / 8 {
            return Err(Error::Malformed("invalid load-command count"));
        }

        let mut install_id = None;
        let mut signature_command = None;
        let mut signature = None;
        let mut text_size = None;
        let mut linkedit = None;
        let mut has_deployment_target = false;
        let mut segments = Vec::new();
        let mut virtual_segments = Vec::new();
        let mut sections = Vec::new();
        let mut file_bounds = None;

        for _ in 0..header.ncmds {
            let offset = command_end - commands.len();
            let command = read::<LoadCommandHeader>(commands)?;
            let size = command.cmdsize as usize;
            if size < 8 || !size.is_multiple_of(8) {
                return Err(Error::Malformed("invalid load-command size"));
            }
            let data = slice(commands, 0, size)?;
            commands = &commands[size..];

            match command.cmd {
                LC_ID_DYLIB => {
                    command_string(data, DYLIB_HEADER_SIZE)?;
                    if install_id.replace(offset..offset + size).is_some() {
                        return Err(Error::Malformed("invalid or duplicate LC_ID_DYLIB"));
                    }
                }
                LC_CODE_SIGNATURE => {
                    let data = fixed::<LinkeditDataCommand>(data)?;
                    if signature_command.replace(offset).is_some() {
                        return Err(Error::Malformed("invalid or duplicate LC_CODE_SIGNATURE"));
                    }
                    if data.datasize != 0 {
                        let offset = data.dataoff as usize;
                        signature = Some((offset, slice(image, offset, data.datasize as usize)?));
                    }
                }
                LC_VERSION_MIN_MACOSX | LC_BUILD_VERSION => {
                    if command.cmd == LC_VERSION_MIN_MACOSX {
                        fixed::<VersionMinCommand>(data)?;
                    } else {
                        let build = read::<BuildVersionCommand>(data)?;
                        let header_size = BuildVersionCommand::size_with(&LE);
                        let tools_size = (build.ntools as usize)
                            .checked_mul(BuildToolVersion::size_with(&LE))
                            .ok_or(Error::TooLarge)?;
                        let tools = slice(data, header_size, tools_size)?;
                        if header_size + tools.len() != data.len() {
                            return Err(Error::Malformed("invalid build-tool table"));
                        }
                        if build.platform != PLATFORM_MACOS {
                            return Err(Error::UnsupportedValue {
                                field: "build platform",
                                value: u64::from(build.platform),
                            });
                        }
                    }
                    if has_deployment_target {
                        return Err(Error::Malformed("multiple deployment targets"));
                    }
                    has_deployment_target = true;
                }
                LC_SEGMENT_64 => {
                    let segment = read::<SegmentCommand64>(data)?;
                    let (records, trailing) =
                        data[SIZEOF_SEGMENT_COMMAND_64..].as_chunks::<SIZEOF_SECTION_64>();
                    if !trailing.is_empty() || records.len() != segment.nsects as usize {
                        return Err(Error::Malformed("invalid segment section table"));
                    }
                    let segment_offset = usize::try_from(segment.fileoff)?;
                    let segment_data =
                        slice(image, segment_offset, usize::try_from(segment.filesize)?)?;
                    let segment_range = segment_offset..segment_offset + segment_data.len();
                    if segment.filesize > segment.vmsize {
                        return Err(Error::Malformed("segment filesize exceeds vmsize"));
                    }

                    let virtual_end = segment
                        .vmaddr
                        .checked_add(segment.vmsize)
                        .ok_or(Error::TooLarge)?;
                    if segment.vmsize > 0 {
                        virtual_segments.push(segment.vmaddr..virtual_end);
                    }

                    if !segment_range.is_empty() {
                        segments.push(segment_range.clone());
                    }

                    if segment.segname == SEG_TEXT
                        && (text_size.replace(segment.filesize).is_some()
                            || segment.fileoff != 0
                            || segment_range.end < command_end)
                    {
                        return Err(Error::Malformed("invalid or duplicate __TEXT segment"));
                    }

                    if segment.segname == SEG_LINKEDIT
                        && (segment.vmsize == 0
                            || !records.is_empty()
                            || linkedit.replace((offset, segment)).is_some())
                    {
                        return Err(Error::Malformed("invalid or duplicate __LINKEDIT segment"));
                    }

                    for record in records {
                        let section = read::<Section64>(record)?;
                        let section_end = section
                            .addr
                            .checked_add(section.size)
                            .ok_or(Error::TooLarge)?;
                        if section.addr < segment.vmaddr || section_end > virtual_end {
                            return Err(Error::Malformed(
                                "section exceeds its segment's virtual address range",
                            ));
                        }

                        extend_file_bounds(
                            &mut file_bounds,
                            [file_table(section.reloff, section.nreloc, RELOCATION_SIZE)],
                        );

                        let section_type = section.flags & SECTION_TYPE;
                        if section_type == S_ZEROFILL
                            || section_type == S_GB_ZEROFILL
                            || section_type == S_THREAD_LOCAL_ZEROFILL
                        {
                            continue;
                        }

                        let section_offset = section.offset as usize;
                        let section_data =
                            slice(image, section_offset, usize::try_from(section.size)?)?;
                        let section_range = section_offset..section_offset + section_data.len();
                        if !section_range.is_empty() {
                            if section_range.start < command_end
                                || section_range.start < segment_range.start
                                || section_range.end > segment_range.end
                            {
                                return Err(Error::Malformed(
                                    "section is outside its segment or overlaps load commands",
                                ));
                            }
                            sections.push(section_range);
                        }
                    }
                }
                LC_SYMTAB => {
                    let table = fixed::<SymtabCommand>(data)?;
                    extend_file_bounds(
                        &mut file_bounds,
                        [
                            file_table(table.symoff, table.nsyms, NLIST_64_SIZE),
                            file_table(table.stroff, table.strsize, 1),
                        ],
                    );
                }
                LC_DYSYMTAB => {
                    let table = fixed::<DysymtabCommand>(data)?;
                    extend_file_bounds(
                        &mut file_bounds,
                        [
                            file_table(table.tocoff, table.ntoc, DYLIB_TABLE_OF_CONTENTS_SIZE),
                            file_table(table.modtaboff, table.nmodtab, DYLIB_MODULE_64_SIZE),
                            file_table(table.extrefsymoff, table.nextrefsyms, DYLIB_REFERENCE_SIZE),
                            file_table(
                                table.indirectsymoff,
                                table.nindirectsyms,
                                INDIRECT_SYMBOL_SIZE,
                            ),
                            file_table(table.extreloff, table.nextrel, RELOCATION_SIZE),
                            file_table(table.locreloff, table.nlocrel, RELOCATION_SIZE),
                        ],
                    );
                }
                LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
                    let info = fixed::<DyldInfoCommand>(data)?;
                    extend_file_bounds(
                        &mut file_bounds,
                        [
                            file_table(info.rebase_off, info.rebase_size, 1),
                            file_table(info.bind_off, info.bind_size, 1),
                            file_table(info.weak_bind_off, info.weak_bind_size, 1),
                            file_table(info.lazy_bind_off, info.lazy_bind_size, 1),
                            file_table(info.export_off, info.export_size, 1),
                        ],
                    );
                }
                LC_SEGMENT_SPLIT_INFO
                | LC_FUNCTION_STARTS
                | LC_DATA_IN_CODE
                | LC_DYLIB_CODE_SIGN_DRS
                | LC_LINKER_OPTIMIZATION_HINT
                | LC_DYLD_EXPORTS_TRIE
                | LC_DYLD_CHAINED_FIXUPS => {
                    let data = fixed::<LinkeditDataCommand>(data)?;
                    extend_file_bounds(
                        &mut file_bounds,
                        [file_table(data.dataoff, data.datasize, 1)],
                    );
                }
                LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB | LC_LOAD_UPWARD_DYLIB
                | LC_LAZY_LOAD_DYLIB => command_string(data, DYLIB_HEADER_SIZE)?,
                LC_RPATH | LC_SUB_FRAMEWORK | LC_SUB_UMBRELLA | LC_SUB_CLIENT | LC_SUB_LIBRARY => {
                    command_string(data, 12)?;
                }
                LC_SOURCE_VERSION => {
                    fixed::<SourceVersionCommand>(data)?;
                }
                LC_UUID => {
                    fixed::<UuidCommand>(data)?;
                }
                LC_ROUTINES_64 => {
                    fixed::<RoutinesCommand64>(data)?;
                }
                _ => {
                    return Err(Error::UnsupportedValue {
                        field: "load command",
                        value: u64::from(command.cmd),
                    });
                }
            }
        }
        if !commands.is_empty() {
            return Err(Error::Malformed("load commands do not fill sizeofcmds"));
        }

        let install_id = install_id.ok_or(Error::Malformed("missing LC_ID_DYLIB"))?;
        let text_size = text_size.ok_or(Error::Malformed("missing __TEXT segment"))?;
        let linkedit = linkedit.ok_or(Error::Malformed("missing __LINKEDIT segment"))?;

        let (_, linkedit_segment) = linkedit;
        if !sort_disjoint(&mut segments) || !sort_disjoint(&mut sections) {
            return Err(Error::Malformed("overlapping file regions"));
        }
        if !sort_disjoint(&mut virtual_segments)
            || virtual_segments
                .last()
                .is_none_or(|segment| segment.start != linkedit_segment.vmaddr)
        {
            return Err(Error::Unsupported(
                "overlapping virtual segments or nonterminal __LINKEDIT",
            ));
        }

        if usize::try_from(linkedit_segment.fileoff + linkedit_segment.filesize)? != image.len() {
            return Err(Error::Unsupported(
                "__LINKEDIT is not the final file segment",
            ));
        }

        let code_limit = if let Some((offset, data)) = signature {
            let padding = &image[offset + data.len()..];
            if offset < usize::try_from(linkedit_segment.fileoff)?
                || !offset.is_multiple_of(16)
                || padding.len() > 15
                || padding.iter().any(|byte| *byte != 0)
            {
                return Err(Error::Unsupported(
                    "code signature is not at the end of __LINKEDIT",
                ));
            }

            offset
        } else {
            image.len()
        };

        // Load commands must stay within __TEXT and before any section or file table.
        let mut header_limit = usize::try_from(text_size)?;
        if let Some(first_section) = sections.first() {
            header_limit = header_limit.min(first_section.start);
        }
        if let Some(range) = file_bounds {
            let offset = usize::try_from(range.start)?;
            let size = usize::try_from(range.end - range.start)?;
            let data = slice(image, offset, size)?;
            if offset < command_end || offset + data.len() > code_limit {
                return Err(Error::Malformed(
                    "file data overlaps load commands or the signature",
                ));
            }
            header_limit = header_limit.min(offset);
        }

        Ok(Self {
            image,
            command_end,
            header_limit,
            code_limit,
            install_id,
            signature_command,
            signature: signature.map(|(_, data)| data),
            text_size,
            linkedit,
            segment_alignment,
        })
    }

    pub(crate) fn set_install_name(
        &self,
        name: &CStr,
        identifier: &CStr,
    ) -> Result<Vec<u8>, Error> {
        let signature_start = self
            .code_limit
            .checked_next_multiple_of(16)
            .ok_or(Error::TooLarge)?;
        let signature = Signature::prepare(self.signature, identifier, signature_start)?;
        let signature_size = signature
            .size()
            .checked_next_multiple_of(16)
            .ok_or(Error::TooLarge)?;
        let final_size = signature_start
            .checked_add(signature_size)
            .ok_or(Error::TooLarge)?;
        u32::try_from(final_size)?;

        let name = name.to_bytes_with_nul();
        let install_size = name
            .len()
            .checked_add(DYLIB_HEADER_SIZE)
            .and_then(|size| size.checked_next_multiple_of(8))
            .ok_or(Error::TooLarge)?;
        let tail = self
            .install_id
            .start
            .checked_add(install_size)
            .ok_or(Error::TooLarge)?;
        let tail_end = tail
            .checked_add(self.command_end - self.install_id.end)
            .ok_or(Error::TooLarge)?;
        let command_end = tail_end
            .checked_add(if self.signature_command.is_none() {
                SIZEOF_LINKEDIT_DATA_COMMAND
            } else {
                0
            })
            .ok_or(Error::TooLarge)?;
        if command_end > self.header_limit
            || (command_end > self.command_end
                && self.image[self.command_end..command_end]
                    .iter()
                    .any(|byte| *byte != 0))
        {
            return Err(Error::InsufficientHeaderPadding);
        }

        // Only the command suffix moves. Bytes beyond the command area retain their file offsets.
        let shifted = |offset| {
            if offset >= self.install_id.end {
                tail + (offset - self.install_id.end)
            } else {
                offset
            }
        };
        let mut output = Vec::with_capacity(final_size);
        let name_start = self.install_id.start + DYLIB_HEADER_SIZE;
        output.extend_from_slice(&self.image[..name_start]);
        output[self.install_id.start + 4..self.install_id.start + 8]
            .copy_from_slice(&u32::try_from(install_size)?.to_le_bytes());
        output[self.install_id.start + 8..self.install_id.start + 12]
            .copy_from_slice(&u32::try_from(DYLIB_HEADER_SIZE)?.to_le_bytes());
        output.extend_from_slice(name);
        output.resize(tail, 0);
        output.extend_from_slice(&self.image[self.install_id.end..self.command_end]);

        let signature_offset = if let Some(offset) = self.signature_command {
            shifted(offset)
        } else {
            let count = u32::from_le_bytes(array(&output, 16)?)
                .checked_add(1)
                .ok_or(Error::TooLarge)?;
            output[16..20].copy_from_slice(&count.to_le_bytes());
            output.extend_from_slice(&LC_CODE_SIGNATURE.to_le_bytes());
            output.extend_from_slice(&u32::try_from(SIZEOF_LINKEDIT_DATA_COMMAND)?.to_le_bytes());
            output.resize(command_end, 0);
            tail_end
        };
        output[20..24]
            .copy_from_slice(&u32::try_from(command_end - SIZEOF_HEADER_64)?.to_le_bytes());
        output.resize(command_end.max(self.command_end), 0);
        output.extend_from_slice(&self.image[output.len()..self.code_limit]);
        output.resize(final_size, 0);

        // Finalize the headers before hashing the bytes they describe.
        output[signature_offset + 8..signature_offset + 12]
            .copy_from_slice(&u32::try_from(signature_start)?.to_le_bytes());
        output[signature_offset + 12..signature_offset + 16]
            .copy_from_slice(&u32::try_from(signature_size)?.to_le_bytes());
        let (linkedit_offset, segment) = self.linkedit;
        let linkedit_offset = shifted(linkedit_offset);
        let linkedit_size = final_size
            .checked_sub(usize::try_from(segment.fileoff)?)
            .ok_or(Error::Malformed("invalid __LINKEDIT offset"))?;
        let virtual_size = segment.vmsize.max(
            linkedit_size
                .checked_next_multiple_of(self.segment_alignment)
                .ok_or(Error::TooLarge)? as u64,
        );
        segment
            .vmaddr
            .checked_add(virtual_size)
            .ok_or(Error::TooLarge)?;
        output[linkedit_offset + 32..linkedit_offset + 40]
            .copy_from_slice(&virtual_size.to_le_bytes());
        output[linkedit_offset + 48..linkedit_offset + 56]
            .copy_from_slice(&(linkedit_size as u64).to_le_bytes());

        signature.write(&mut output, self.text_size)?;
        Ok(output)
    }
}

/// Decode a fixed record without relying on alignment or host endianness.
fn read<'a, T: TryFromCtx<'a, Endian, Error = scroll::Error>>(data: &'a [u8]) -> Result<T, Error> {
    data.pread_with(0, LE)
        .map_err(|_| Error::Malformed("range extends past its containing data"))
}

fn fixed<'a, T>(data: &'a [u8]) -> Result<T, Error>
where
    T: TryFromCtx<'a, Endian, Error = scroll::Error> + SizeWith<Endian>,
{
    let record = read(data)?;
    if data.len() != T::size_with(&LE) {
        return Err(Error::Malformed("invalid fixed-size load command"));
    }
    Ok(record)
}

fn command_string(data: &[u8], header_size: usize) -> Result<(), Error> {
    let header = slice(data, 0, header_size)?;
    let offset = u32::from_le_bytes(array(header, 8)?) as usize;
    if offset < header_size {
        return Err(Error::Malformed("invalid load-command string offset"));
    }
    c_string(data, offset)?;
    Ok(())
}

fn file_table(offset: u32, count: u32, entry_size: u32) -> Range<u64> {
    let offset = u64::from(offset);
    offset..offset + u64::from(count) * u64::from(entry_size)
}

/// Extend the bounds to contain every nonempty file range.
fn extend_file_bounds(
    bounds: &mut Option<Range<u64>>,
    ranges: impl IntoIterator<Item = Range<u64>>,
) {
    for range in ranges {
        // Empty tables do not reference file data, regardless of their offset.
        if range.is_empty() {
            continue;
        }
        if let Some(bounds) = bounds {
            bounds.start = bounds.start.min(range.start);
            bounds.end = bounds.end.max(range.end);
        } else {
            *bounds = Some(range);
        }
    }
}
