use std::ops::Range;

use crate::Error;
use crate::bytes::{le32, range, usize_size};
use crate::format::{
    CPU_TYPE_ARM64, CPU_TYPE_X86_64, Command, HEADER_SIZE, Header, LC_BUILD_VERSION,
    LC_CODE_SIGNATURE, LC_DATA_IN_CODE, LC_DYLD_CHAINED_FIXUPS, LC_DYLD_EXPORTS_TRIE, LC_DYLD_INFO,
    LC_DYLD_INFO_ONLY, LC_DYLIB_CODE_SIGN_DRS, LC_DYSYMTAB, LC_FUNCTION_STARTS, LC_ID_DYLIB,
    LC_LAZY_LOAD_DYLIB, LC_LINKER_OPTIMIZATION_HINT, LC_LOAD_DYLIB, LC_LOAD_UPWARD_DYLIB,
    LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB, LC_ROUTINES_64, LC_RPATH, LC_SEGMENT_64,
    LC_SEGMENT_SPLIT_INFO, LC_SOURCE_VERSION, LC_SUB_CLIENT, LC_SUB_FRAMEWORK, LC_SUB_LIBRARY,
    LC_SUB_UMBRELLA, LC_SYMTAB, LC_UUID, LC_VERSION_MIN_MACOSX, MH_DYLIB, MH_MAGIC_64,
    S_GB_ZEROFILL, S_THREAD_LOCAL_ZEROFILL, S_ZEROFILL, SECTION_SIZE, SECTION_TYPE, SEGMENT_SIZE,
    Section, Segment,
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

    let command_end = range(HEADER_SIZE, header.sizeofcmds as usize, image.len())?.end;
    if header.ncmds as usize > header.sizeofcmds as usize / 8 {
        return Err(Error::Malformed("invalid load-command count"));
    }

    let mut commands = Vec::new();
    let mut offset = HEADER_SIZE;

    for _ in 0..header.ncmds {
        range(offset, 8, command_end)?;
        let size = le32(image, offset + 4)? as usize;
        if size < 8 || !size.is_multiple_of(8) {
            return Err(Error::Malformed("invalid load-command size"));
        }

        let data = &image[range(offset, size, command_end)?];
        commands.push(Command::parse(data)?);
        offset += size;
    }

    if offset != command_end {
        return Err(Error::Malformed("load commands do not fill sizeofcmds"));
    }

    let mut install_id = None;
    let mut signature = None;
    let mut signature_command = None;
    let mut text = None;
    let mut linkedit = None;
    let mut info_plist = None;
    let mut minimum_version = None;
    let mut segments = Vec::new();
    let mut virtual_segments = Vec::new();
    let mut sections = Vec::new();
    let mut references = Vec::new();

    for (index, command) in commands.iter().enumerate() {
        if command.kind == LC_ID_DYLIB {
            if install_id.replace(index).is_some() {
                return Err(Error::Malformed("invalid or duplicate LC_ID_DYLIB"));
            }
        }

        if command.kind == LC_CODE_SIGNATURE {
            let data_offset = le32(command.data, 8)?;
            let data_size = le32(command.data, 12)?;
            if signature_command.replace(index).is_some() {
                return Err(Error::Malformed("invalid or duplicate LC_CODE_SIGNATURE"));
            }

            if data_size > 0 {
                signature = Some(range(
                    data_offset as usize,
                    data_size as usize,
                    image.len(),
                )?);
            }
        }

        if command.kind == LC_VERSION_MIN_MACOSX {
            if minimum_version.replace(le32(command.data, 8)?).is_some() {
                return Err(Error::Malformed("multiple deployment targets"));
            }
        }

        if command.kind == LC_BUILD_VERSION {
            if le32(command.data, 8)? != 1 {
                return Err(Error::Unsupported("non-macOS build platform"));
            }

            if minimum_version.replace(le32(command.data, 12)?).is_some() {
                return Err(Error::Malformed("multiple deployment targets"));
            }
        }

        if command.kind == LC_SEGMENT_64 {
            let segment = Segment::parse(command.data)?;
            let segment_range = range(
                usize_size(segment.fileoff)?,
                usize_size(segment.filesize)?,
                image.len(),
            )?;
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
                segments.push(segment_range.clone());
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
                if linkedit.replace((index, segment)).is_some() || segment.nsects != 0 {
                    return Err(Error::Malformed("invalid or duplicate __LINKEDIT segment"));
                }
            }

            let section_bytes = (segment.nsects as usize)
                .checked_mul(SECTION_SIZE)
                .ok_or(Error::TooLarge)?;
            if range(SEGMENT_SIZE, section_bytes, command.data.len())?.end != command.data.len() {
                return Err(Error::Malformed("invalid segment section table"));
            }

            for section_index in 0..segment.nsects as usize {
                let offset = SEGMENT_SIZE + section_index * SECTION_SIZE;
                let section = Section::parse(&command.data[offset..offset + SECTION_SIZE])?;
                let section_end = section
                    .addr
                    .checked_add(section.size)
                    .ok_or(Error::TooLarge)?;
                if section.addr < segment.vmaddr || section_end > virtual_end {
                    return Err(Error::Malformed(
                        "section exceeds its segment's virtual address range",
                    ));
                }

                add_reference(
                    &mut references,
                    section.reloff,
                    u64::from(section.nreloc) * 8,
                    image.len(),
                )?;

                let section_type = section.flags & SECTION_TYPE;
                if section_type == S_ZEROFILL
                    || section_type == S_GB_ZEROFILL
                    || section_type == S_THREAD_LOCAL_ZEROFILL
                {
                    continue;
                }

                let section_range = range(
                    section.offset as usize,
                    usize_size(section.size)?,
                    image.len(),
                )?;
                if !section_range.is_empty() {
                    if section_range.start < command_end
                        || section_range.start < segment_range.start
                        || section_range.end > segment_range.end
                    {
                        return Err(Error::Malformed(
                            "section is outside its segment or overlaps load commands",
                        ));
                    }
                    sections.push(section_range.clone());
                }

                if section.segname == *b"__TEXT\0\0\0\0\0\0\0\0\0\0"
                    && section.sectname == *b"__info_plist\0\0\0\0"
                {
                    if info_plist.replace(&image[section_range]).is_some() {
                        return Err(Error::Malformed("duplicate embedded Info.plist"));
                    }
                }
            }
        }

        file_references(command, &mut references, image.len())?;
    }

    check_overlaps(&mut segments)?;
    check_overlaps(&mut sections)?;

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

    if usize_size(linkedit.fileoff + linkedit.filesize)? != image.len() {
        return Err(Error::Unsupported(
            "__LINKEDIT is not the final file segment",
        ));
    }

    let code_limit = if let Some(signature) = &signature {
        if signature.start < usize_size(linkedit.fileoff)?
            || !signature.start.is_multiple_of(16)
            || image.len() - signature.end > 15
            || image[signature.end..].iter().any(|byte| *byte != 0)
        {
            return Err(Error::Unsupported(
                "code signature is not at the end of __LINKEDIT",
            ));
        }

        signature.start
    } else {
        image.len()
    };

    for reference in &references {
        if reference.start < command_end || reference.end > code_limit {
            return Err(Error::Malformed(
                "file data overlaps load commands or the signature",
            ));
        }
    }

    if sections.iter().any(|section| section.end > code_limit) {
        return Err(Error::Malformed("section overlaps the code signature"));
    }

    Ok(())
}

fn check_overlaps(regions: &mut [Range<usize>]) -> Result<(), Error> {
    regions.sort_unstable_by_key(|region| region.start);
    if regions.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(Error::Malformed("overlapping file regions"));
    }

    Ok(())
}

fn add_reference(
    references: &mut Vec<Range<usize>>,
    offset: u32,
    size: u64,
    limit: usize,
) -> Result<(), Error> {
    if size > 0 {
        references.push(range(offset as usize, usize_size(size)?, limit)?);
    }

    Ok(())
}

/// Track every referenced file range so header growth and signature replacement
/// cannot overwrite symbol tables, relocations, or dyld metadata.
fn file_references(
    command: &Command<'_>,
    references: &mut Vec<Range<usize>>,
    limit: usize,
) -> Result<(), Error> {
    let data = command.data;
    match command.kind {
        LC_SYMTAB => {
            // symoff/nsyms (nlist_64), followed by stroff/strsize.
            add_reference(
                references,
                le32(data, 8)?,
                u64::from(le32(data, 12)?) * 16,
                limit,
            )?;
            add_reference(
                references,
                le32(data, 16)?,
                u64::from(le32(data, 20)?),
                limit,
            )?;
        }
        LC_DYSYMTAB => {
            // Each table has adjacent offset/count fields. The first six fields
            // index the symbol table; the remaining six pairs reference file data.
            for (field, entry_size) in [
                (32, 8),  // dylib_table_of_contents
                (40, 56), // dylib_module_64
                (48, 4),  // dylib_reference
                (56, 4),  // indirect symbols
                (64, 8),  // external relocations
                (72, 8),  // local relocations
            ] {
                add_reference(
                    references,
                    le32(data, field)?,
                    u64::from(le32(data, field + 4)?) * entry_size,
                    limit,
                )?;
            }
        }
        LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
            // Rebase, bind, weak bind, lazy bind, and export offset/size pairs.
            for field in [8, 16, 24, 32, 40] {
                add_reference(
                    references,
                    le32(data, field)?,
                    u64::from(le32(data, field + 4)?),
                    limit,
                )?;
            }
        }
        LC_SEGMENT_SPLIT_INFO
        | LC_FUNCTION_STARTS
        | LC_DATA_IN_CODE
        | LC_DYLIB_CODE_SIGN_DRS
        | LC_LINKER_OPTIMIZATION_HINT
        | LC_DYLD_EXPORTS_TRIE
        | LC_DYLD_CHAINED_FIXUPS => {
            add_reference(
                references,
                le32(data, 8)?,
                u64::from(le32(data, 12)?),
                limit,
            )?;
        }
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
        | LC_SUB_LIBRARY => {}
        _ => return Err(Error::Unsupported("load-command file references")),
    }

    Ok(())
}

#[cfg(test)]
mod tests;
