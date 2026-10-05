//! Inspect fixture and output bytes independently of the editor's private parser.

use anyhow::{Context, Result};
use scroll::{LE, Pread};

pub(crate) struct LoadCommand<'a> {
    pub(crate) offset: usize,
    pub(crate) kind: u32,
    pub(crate) data: &'a [u8],
}

pub(crate) fn commands(image: &[u8]) -> Result<Vec<LoadCommand<'_>>> {
    let count = image.pread_with::<u32>(16, LE)?;
    let mut offset = 32;
    let mut commands = Vec::new();

    for _ in 0..count {
        let kind = image.pread_with(offset, LE)?;
        let size = image.pread_with::<u32>(offset + 4, LE)? as usize;
        commands.push(LoadCommand {
            offset,
            kind,
            data: image.get(offset..offset + size).context("load command")?,
        });
        offset += size;
    }

    Ok(commands)
}

pub(crate) fn install_name(image: &[u8]) -> Result<&[u8]> {
    let command = commands(image)?
        .into_iter()
        .find(|command| command.kind == 0xd)
        .context("LC_ID_DYLIB")?;
    let offset = command.data.pread_with::<u32>(8, LE)? as usize;
    let bytes = command.data.get(offset..).context("install name")?;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .context("name terminator")?;

    Ok(&bytes[..end])
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Section<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) offset: usize,
    pub(crate) data: &'a [u8],
}

pub(crate) fn sections(image: &[u8]) -> Result<Vec<Section<'_>>> {
    let mut sections = Vec::new();

    for command in commands(image)? {
        if command.kind != 0x19 {
            continue;
        }

        let count = command.data.pread_with::<u32>(64, LE)? as usize;
        for index in 0..count {
            let section = command
                .data
                .get(72 + index * 80..72 + (index + 1) * 80)
                .context("section header")?;
            let offset = section.pread_with::<u32>(48, LE)? as usize;
            let flags = section.pread_with::<u32>(64, LE)?;
            let size = match flags & 0xff {
                1 | 0xc | 0x12 => 0,
                _ => usize::try_from(section.pread_with::<u64>(40, LE)?)?,
            };
            sections.push(Section {
                name: &section[..16],
                offset,
                data: image.get(offset..offset + size).context("section data")?,
            });
        }
    }

    Ok(sections)
}

/// Space for `LC_ID_DYLIB`, including its fixed fields and terminating NUL.
pub(crate) fn name_capacity(image: &[u8]) -> Result<usize> {
    let first_section = sections(image)?
        .into_iter()
        .filter(|section| !section.data.is_empty())
        .map(|section| section.offset)
        .min()
        .context("first section")?;
    let commands = commands(image)?;
    let last = commands.last().context("last command")?;
    let install_id = commands
        .iter()
        .find(|command| command.kind == 0xd)
        .context("LC_ID_DYLIB")?;

    Ok(first_section - (last.offset + last.data.len()) + install_id.data.len())
}
