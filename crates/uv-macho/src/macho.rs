use std::collections::BTreeMap;
use std::ops::Range;

use crate::Error;
use crate::bytes::{array, slice};
use crate::format::{
    CPU_TYPE_ARM64, CPU_TYPE_X86_64, Command, HEADER_SIZE, Header, LC_BUILD_VERSION,
    LC_CODE_SIGNATURE, LC_DATA_IN_CODE, LC_DYLD_CHAINED_FIXUPS, LC_DYLD_EXPORTS_TRIE, LC_DYLD_INFO,
    LC_DYLD_INFO_ONLY, LC_DYLIB_CODE_SIGN_DRS, LC_DYSYMTAB, LC_FUNCTION_STARTS, LC_ID_DYLIB,
    LC_LAZY_LOAD_DYLIB, LC_LINKER_OPTIMIZATION_HINT, LC_LOAD_DYLIB, LC_LOAD_UPWARD_DYLIB,
    LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB, LC_ROUTINES_64, LC_RPATH, LC_SEGMENT_64,
    LC_SEGMENT_SPLIT_INFO, LC_SOURCE_VERSION, LC_SUB_CLIENT, LC_SUB_FRAMEWORK, LC_SUB_LIBRARY,
    LC_SUB_UMBRELLA, LC_SYMTAB, LC_UUID, LC_VERSION_MIN_MACOSX, MH_DYLIB, MH_MAGIC_64,
    S_GB_ZEROFILL, S_THREAD_LOCAL_ZEROFILL, S_ZEROFILL, SECTION_TYPE, Section, Segment,
};

pub(crate) fn parse(image: &[u8]) -> Result<(), Error> {
    let header = Header::parse(image)?;
    if header.magic != MH_MAGIC_64 || header.filetype != MH_DYLIB {
        return Err(Error::Unsupported(
            "expected a thin little-endian 64-bit dylib",
        ));
    }

    match header.cputype {
        CPU_TYPE_ARM64 | CPU_TYPE_X86_64 => {}
        _ => return Err(Error::Unsupported("CPU architecture")),
    }

    let mut command_data = slice(image, HEADER_SIZE, header.sizeofcmds as usize)?;
    let command_end = HEADER_SIZE + command_data.len();
    if header.ncmds as usize > header.sizeofcmds as usize / 8 {
        return Err(Error::Malformed("invalid load-command count"));
    }

    let mut commands = Vec::new();
    for _ in 0..header.ncmds {
        let size = u32::from_le_bytes(array(command_data, 4)?) as usize;
        if size < 8 || !size.is_multiple_of(8) {
            return Err(Error::Malformed("invalid load-command size"));
        }

        let data = slice(command_data, 0, size)?;
        commands.push(Command::parse(data)?);
        command_data = &command_data[data.len()..];
    }

    if !command_data.is_empty() {
        return Err(Error::Malformed("load commands do not fill sizeofcmds"));
    }

    let mut install_id = None;
    let mut signature = None;
    let mut signature_command = None;
    let mut text = None;
    let mut linkedit = None;
    let mut info_plist = None;
    let mut minimum_version = None;
    let mut segments = FileRegions::default();
    let mut virtual_segments = Vec::new();
    let mut sections = FileRegions::default();
    let mut references = Vec::new();

    for (index, command) in commands.iter().enumerate() {
        match command.kind {
            LC_ID_DYLIB => {
                if install_id.replace(index).is_some() {
                    return Err(Error::Malformed("invalid or duplicate LC_ID_DYLIB"));
                }
            }

            LC_CODE_SIGNATURE => {
                let data_offset = u32::from_le_bytes(array(command.data, 8)?);
                let data_size = u32::from_le_bytes(array(command.data, 12)?);
                if signature_command.replace(index).is_some() {
                    return Err(Error::Malformed("invalid or duplicate LC_CODE_SIGNATURE"));
                }

                if data_size > 0 {
                    let offset = data_offset as usize;
                    signature = Some((offset, slice(image, offset, data_size as usize)?));
                }
            }

            LC_VERSION_MIN_MACOSX => {
                if minimum_version
                    .replace(u32::from_le_bytes(array(command.data, 8)?))
                    .is_some()
                {
                    return Err(Error::Malformed("multiple deployment targets"));
                }
            }

            LC_BUILD_VERSION => {
                if u32::from_le_bytes(array(command.data, 8)?) != 1 {
                    return Err(Error::Unsupported("non-macOS build platform"));
                }

                if minimum_version
                    .replace(u32::from_le_bytes(array(command.data, 12)?))
                    .is_some()
                {
                    return Err(Error::Malformed("multiple deployment targets"));
                }
            }

            LC_SEGMENT_64 => {
                let segment = Segment::parse(command.data)?;
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
                    virtual_segments.push((segment.vmaddr, virtual_end));
                }

                if !segment_range.is_empty() {
                    segments.insert(segment_range.clone())?;
                }

                if segment.segname == *b"__TEXT\0\0\0\0\0\0\0\0\0\0" {
                    if text.replace(segment).is_some()
                        || segment.fileoff != 0
                        || segment_range.end < command_end
                    {
                        return Err(Error::Malformed("invalid or duplicate __TEXT segment"));
                    }
                }

                if segment.segname == *b"__LINKEDIT\0\0\0\0\0\0" {
                    if linkedit.replace((index, segment)).is_some() || !segment.sections.is_empty()
                    {
                        return Err(Error::Malformed("invalid or duplicate __LINKEDIT segment"));
                    }
                }

                for data in segment.sections {
                    let section = Section::parse(data)?;
                    let section_end = section
                        .addr
                        .checked_add(section.size)
                        .ok_or(Error::TooLarge)?;
                    if section.addr < segment.vmaddr || section_end > virtual_end {
                        return Err(Error::Malformed(
                            "section exceeds its segment's virtual address range",
                        ));
                    }

                    references.push((section.reloff, u64::from(section.nreloc) * 8));

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
                        sections.insert(section_range.clone())?;
                    }

                    if section.segname == *b"__TEXT\0\0\0\0\0\0\0\0\0\0"
                        && section.sectname == *b"__info_plist\0\0\0\0"
                    {
                        if info_plist.replace(section_data).is_some() {
                            return Err(Error::Malformed("duplicate embedded Info.plist"));
                        }
                    }
                }
            }
            _ => {}
        }

        file_references(command, &mut references)?;
    }

    install_id.ok_or(Error::Malformed("missing LC_ID_DYLIB"))?;
    text.ok_or(Error::Malformed("missing __TEXT segment"))?;
    let (_, linkedit) = linkedit.ok_or(Error::Malformed("missing __LINKEDIT segment"))?;

    virtual_segments.sort_unstable();
    if virtual_segments
        .windows(2)
        .any(|pair| pair[0].1 > pair[1].0)
        || virtual_segments
            .last()
            .is_none_or(|segment| segment.0 != linkedit.vmaddr)
    {
        return Err(Error::Unsupported(
            "overlapping virtual segments or nonterminal __LINKEDIT",
        ));
    }

    if usize::try_from(linkedit.fileoff + linkedit.filesize)? != image.len() {
        return Err(Error::Unsupported(
            "__LINKEDIT is not the final file segment",
        ));
    }

    let code_limit = if let Some((offset, data)) = signature {
        let padding = &image[offset + data.len()..];
        if offset < usize::try_from(linkedit.fileoff)?
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

    for (offset, size) in references {
        // Empty tables do not reference file data, regardless of their offset.
        if size == 0 {
            continue;
        }

        let offset = offset as usize;
        let data = slice(image, offset, usize::try_from(size)?)?;
        if offset < command_end || offset + data.len() > code_limit {
            return Err(Error::Malformed(
                "file data overlaps load commands or the signature",
            ));
        }
    }

    if sections.end().is_some_and(|end| end > code_limit) {
        return Err(Error::Malformed("section overlaps the code signature"));
    }

    Ok(())
}

/// Nonempty file regions that cannot overlap, regardless of insertion order.
#[derive(Default)]
struct FileRegions {
    /// Each start offset maps to the corresponding exclusive end offset.
    by_start: BTreeMap<usize, usize>,
}

impl FileRegions {
    fn insert(&mut self, region: Range<usize>) -> Result<(), Error> {
        if region.is_empty() {
            return Ok(());
        }

        let overlaps_previous = self
            .by_start
            .range(..=region.start)
            .next_back()
            .is_some_and(|(_, end)| *end > region.start);
        let overlaps_next = self
            .by_start
            .range(region.start..)
            .next()
            .is_some_and(|(start, _)| *start < region.end);
        if overlaps_previous || overlaps_next {
            return Err(Error::Malformed("overlapping file regions"));
        }

        self.by_start.insert(region.start, region.end);
        Ok(())
    }

    fn end(&self) -> Option<usize> {
        self.by_start.last_key_value().map(|(_, end)| *end)
    }
}

/// Collect file offsets and byte lengths for symbol tables and dyld metadata.
fn file_references(command: &Command<'_>, references: &mut Vec<(u32, u64)>) -> Result<(), Error> {
    // Each table has adjacent offset/count fields. Convert entry counts to byte
    // lengths here; the layout reader checks the resulting file ranges together.
    let fields: &[(usize, u64)] = match command.kind {
        LC_SYMTAB => &[(8, 16), (16, 1)], // nlist_64 entries, then string bytes
        LC_DYSYMTAB => &[
            (32, 8),  // dylib_table_of_contents
            (40, 56), // dylib_module_64
            (48, 4),  // dylib_reference
            (56, 4),  // indirect symbols
            (64, 8),  // external relocations
            (72, 8),  // local relocations
        ],
        // Rebase, bind, weak bind, lazy bind, and export bytes.
        LC_DYLD_INFO | LC_DYLD_INFO_ONLY => &[(8, 1), (16, 1), (24, 1), (32, 1), (40, 1)],
        LC_SEGMENT_SPLIT_INFO
        | LC_FUNCTION_STARTS
        | LC_DATA_IN_CODE
        | LC_DYLIB_CODE_SIGN_DRS
        | LC_LINKER_OPTIMIZATION_HINT
        | LC_DYLD_EXPORTS_TRIE
        | LC_DYLD_CHAINED_FIXUPS => &[(8, 1)],
        LC_SEGMENT_64
        | LC_ID_DYLIB
        | LC_CODE_SIGNATURE
        | LC_UUID
        | LC_LOAD_DYLIB
        | LC_LOAD_WEAK_DYLIB
        | LC_REEXPORT_DYLIB
        | LC_LOAD_UPWARD_DYLIB
        | LC_LAZY_LOAD_DYLIB
        | LC_RPATH
        | LC_ROUTINES_64
        | LC_VERSION_MIN_MACOSX
        | LC_BUILD_VERSION
        | LC_SOURCE_VERSION
        | LC_SUB_FRAMEWORK
        | LC_SUB_UMBRELLA
        | LC_SUB_CLIENT
        | LC_SUB_LIBRARY => &[],
        _ => return Err(Error::Unsupported("load-command file references")),
    };

    for &(field, entry_size) in fields {
        references.push((
            u32::from_le_bytes(array(command.data, field)?),
            u64::from(u32::from_le_bytes(array(command.data, field + 4)?)) * entry_size,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests;
