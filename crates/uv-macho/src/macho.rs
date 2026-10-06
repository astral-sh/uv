use std::ops::Range;

use crate::bytes::{array, slice};
use crate::format::{
    CPU_TYPE_ARM64, CPU_TYPE_X86_64, Command, HEADER_SIZE, Header, LC_CODE_SIGNATURE, MH_DYLIB,
    MH_MAGIC_64, S_GB_ZEROFILL, S_THREAD_LOCAL_ZEROFILL, S_ZEROFILL, SECT_INFO_PLIST, SECTION_TYPE,
    SEG_LINKEDIT, SEG_TEXT, Section, Segment,
};
use crate::regions::FileRegions;
use crate::signature::{Metadata, SigningAlgorithms};
use crate::{Error, InstallName, SigningIdentifier};

/// A validated dylib layout tied to the image from which it was parsed.
pub(crate) struct Layout<'a> {
    image: &'a [u8],
    commands: Vec<Command<'a>>,
    install_id: usize,
    command_end: usize,
    data_start: usize,
    code_limit: usize,
    signature: Option<&'a [u8]>,
    signature_command: Option<usize>,
    text: Segment<'a>,
    linkedit: Segment<'a>,
    linkedit_index: usize,
    info_plist: Option<&'a [u8]>,
    minimum_version: Option<u32>,
    architecture: Architecture,
}

impl<'a> Layout<'a> {
    pub(crate) fn parse(image: &'a [u8]) -> Result<Self, Error> {
        let header = Header::parse(image)?;
        if header.magic != MH_MAGIC_64 || header.filetype != MH_DYLIB {
            return Err(Error::Unsupported(
                "expected a thin little-endian 64-bit dylib",
            ));
        }

        let architecture = match header.cputype {
            CPU_TYPE_ARM64 => Architecture::Arm64,
            CPU_TYPE_X86_64 => Architecture::X86_64,
            _ => {
                return Err(Error::UnsupportedValue {
                    field: "CPU architecture",
                    value: u64::from(header.cputype),
                });
            }
        };

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

            let command = Command::parse(slice(command_data, 0, size)?)?;
            command_data = &command_data[command.as_bytes().len()..];
            commands.push(command);
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
        let mut file_bounds = None;
        let mut data_start = image.len();

        for (index, command) in commands.iter().enumerate() {
            match command {
                Command::InstallId(_) => {
                    if install_id.replace(index).is_some() {
                        return Err(Error::Malformed("invalid or duplicate LC_ID_DYLIB"));
                    }
                }

                Command::CodeSignature(signature_data) => {
                    let data = signature_data.file_range()?;
                    if signature_command.replace(index).is_some() {
                        return Err(Error::Malformed("invalid or duplicate LC_CODE_SIGNATURE"));
                    }

                    if !data.is_empty() {
                        let offset = usize::try_from(data.start)?;
                        let size = usize::try_from(data.end - data.start)?;
                        signature = Some((offset, slice(image, offset, size)?));
                    }
                }

                Command::DeploymentTarget(data) => {
                    let field = if data.len() == 16 { 8 } else { 12 };
                    let version = u32::from_le_bytes(array(data, field)?);
                    if minimum_version.replace(version).is_some() {
                        return Err(Error::Malformed("multiple deployment targets"));
                    }
                }

                Command::Segment(segment) => {
                    let segment = segment.parse()?;
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
                        if segment_range.start > 0 {
                            data_start = data_start.min(segment_range.start);
                        }
                        segments.insert(segment_range.clone())?;
                    }

                    if segment.segname == SEG_TEXT {
                        if text.replace(segment).is_some()
                            || segment.fileoff != 0
                            || segment_range.end < command_end
                        {
                            return Err(Error::Malformed("invalid or duplicate __TEXT segment"));
                        }
                    }

                    if segment.segname == SEG_LINKEDIT {
                        if linkedit.replace((index, segment)).is_some()
                            || !segment.sections.is_empty()
                        {
                            return Err(Error::Malformed(
                                "invalid or duplicate __LINKEDIT segment",
                            ));
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

                        extend_file_bounds(&mut file_bounds, [section.relocations]);

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
                            data_start = data_start.min(section_range.start);
                            sections.insert(section_range.clone())?;
                        }

                        if section.segname == SEG_TEXT && section.sectname == SECT_INFO_PLIST {
                            if info_plist.replace(section_data).is_some() {
                                return Err(Error::Malformed("duplicate embedded Info.plist"));
                            }
                        }
                    }
                }
                Command::SymbolTable(table) => {
                    extend_file_bounds(&mut file_bounds, table.file_ranges()?);
                }
                Command::DynamicSymbolTable(table) => {
                    extend_file_bounds(&mut file_bounds, table.file_ranges()?);
                }
                Command::DyldInfo(info) => {
                    extend_file_bounds(&mut file_bounds, info.file_ranges()?);
                }
                Command::LinkeditData(data) => {
                    extend_file_bounds(&mut file_bounds, [data.file_range()?]);
                }
                Command::Other(_) => {}
            }
        }

        let install_id = install_id.ok_or(Error::Malformed("missing LC_ID_DYLIB"))?;
        let text = text.ok_or(Error::Malformed("missing __TEXT segment"))?;
        let (linkedit_index, linkedit) =
            linkedit.ok_or(Error::Malformed("missing __LINKEDIT segment"))?;

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

        if let Some(range) = file_bounds {
            let offset = usize::try_from(range.start)?;
            let size = usize::try_from(range.end - range.start)?;
            let data = slice(image, offset, size)?;
            if offset < command_end || offset + data.len() > code_limit {
                return Err(Error::Malformed(
                    "file data overlaps load commands or the signature",
                ));
            }
            data_start = data_start.min(offset);
        }

        if sections.end().is_some_and(|end| end > code_limit) {
            return Err(Error::Malformed("section overlaps the code signature"));
        }

        Ok(Self {
            image,
            commands,
            install_id,
            command_end,
            data_start,
            code_limit,
            signature: signature.map(|(_, data)| data),
            signature_command,
            text,
            linkedit,
            linkedit_index,
            info_plist,
            minimum_version,
            architecture,
        })
    }

    pub(crate) fn replace_install_name(&self, name: InstallName<'_>) -> Result<Vec<u8>, Error> {
        let name = name.as_c_str().to_bytes_with_nul();
        let mut output = self.image[..HEADER_SIZE].to_vec();

        for (index, command) in self.commands.iter().enumerate() {
            if index == self.install_id {
                let size = name
                    .len()
                    .checked_add(24)
                    .and_then(|size| size.checked_next_multiple_of(8))
                    .ok_or(Error::TooLarge)?;
                let size_u32 = u32::try_from(size)?;

                let mut replacement = vec![0; size];
                replacement[..24].copy_from_slice(&command.as_bytes()[..24]);
                replacement[4..8].copy_from_slice(&size_u32.to_le_bytes());
                replacement[8..12].copy_from_slice(&24u32.to_le_bytes());
                replacement[24..24 + name.len()].copy_from_slice(name);

                output.extend(replacement);
            } else {
                output.extend_from_slice(command.as_bytes());
            }
        }

        self.replace_commands(output)
    }

    fn replace_commands(&self, mut output: Vec<u8>) -> Result<Vec<u8>, Error> {
        if output.len() > self.data_start || output.len() > self.code_limit {
            return Err(Error::InsufficientHeaderPadding);
        }

        if output.len() > self.command_end
            && self.image[self.command_end..output.len()]
                .iter()
                .any(|byte| *byte != 0)
        {
            return Err(Error::InsufficientHeaderPadding);
        }

        let sizeofcmds = u32::try_from(output.len() - HEADER_SIZE)?;
        output[20..24].copy_from_slice(&sizeofcmds.to_le_bytes());
        output.resize(output.len().max(self.command_end), 0);
        output.extend_from_slice(&self.image[output.len()..]);

        Ok(output)
    }

    pub(crate) fn adhoc_sign(&self, identifier: SigningIdentifier<'_>) -> Result<Vec<u8>, Error> {
        let metadata =
            Metadata::read(self.signature, identifier, self.code_limit, self.info_plist)?;

        let command_offset = |index: usize| {
            HEADER_SIZE
                + self.commands[..index]
                    .iter()
                    .map(|command| command.as_bytes().len())
                    .sum::<usize>()
        };
        let linkedit_offset = command_offset(self.linkedit_index);

        let (mut output, signature_offset) = if let Some(index) = self.signature_command {
            (self.image.to_vec(), command_offset(index))
        } else {
            let mut commands = self.image[..self.command_end].to_vec();
            let offset = commands.len();
            commands.resize(offset + 16, 0);
            commands[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(LC_CODE_SIGNATURE));
            commands[offset + 4..offset + 8].copy_from_slice(&u32::to_le_bytes(16));
            commands[16..20]
                .copy_from_slice(&u32::to_le_bytes(u32::try_from(self.commands.len() + 1)?));

            (self.replace_commands(commands)?, offset)
        };

        output.truncate(self.code_limit);
        let signature_start = output
            .len()
            .checked_next_multiple_of(16)
            .ok_or(Error::TooLarge)?;
        output.resize(signature_start, 0);

        let algorithms = if self
            .minimum_version
            .is_some_and(|version| version < 0x000a_0b04)
        {
            SigningAlgorithms::Sha1AndSha256
        } else {
            SigningAlgorithms::Sha256
        };
        let signature = metadata.prepare(
            signature_start,
            (self.text.fileoff, self.text.filesize),
            self.info_plist,
            algorithms,
        )?;
        let signature_size = signature
            .size()
            .checked_next_multiple_of(16)
            .ok_or(Error::TooLarge)?;
        let final_size = signature_start
            .checked_add(signature_size)
            .ok_or(Error::TooLarge)?;
        u32::try_from(final_size)?;

        // Finalize the load commands before hashing the image they describe.
        output[signature_offset + 8..signature_offset + 12]
            .copy_from_slice(&u32::to_le_bytes(u32::try_from(signature_start)?));
        output[signature_offset + 12..signature_offset + 16]
            .copy_from_slice(&u32::to_le_bytes(u32::try_from(signature_size)?));

        let linkedit_size = final_size
            .checked_sub(usize::try_from(self.linkedit.fileoff)?)
            .ok_or(Error::Malformed("invalid __LINKEDIT offset"))?;
        output[linkedit_offset + 48..linkedit_offset + 56]
            .copy_from_slice(&u64::to_le_bytes(linkedit_size as u64));

        // __LINKEDIT must have enough virtual pages even when a replacement signature grows.
        let segment_alignment = match self.architecture {
            Architecture::Arm64 => 16384,
            Architecture::X86_64 => 4096,
        };
        let virtual_size = self.linkedit.vmsize.max(
            linkedit_size
                .checked_next_multiple_of(segment_alignment)
                .ok_or(Error::TooLarge)? as u64,
        );
        self.linkedit
            .vmaddr
            .checked_add(virtual_size)
            .ok_or(Error::TooLarge)?;
        output[linkedit_offset + 32..linkedit_offset + 40]
            .copy_from_slice(&u64::to_le_bytes(virtual_size));

        output.extend(signature.sign(&output)?);
        output.resize(final_size, 0);

        Ok(output)
    }
}

enum Architecture {
    Arm64,
    X86_64,
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

#[cfg(test)]
mod tests;
