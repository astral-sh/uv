//! Inspect fixture and output bytes independently of the editor's private parser.

use std::ffi::CStr;

use anyhow::{Context, Result};

use goblin::mach::MachO;

pub(crate) const ARM64: &[u8] = include_bytes!("../fixtures/arm64.dylib");
pub(crate) const X86_64: &[u8] = include_bytes!("../fixtures/x86_64.dylib");
pub(crate) const SIGNED: &[u8] = include_bytes!("../fixtures/linker-signed-arm64.dylib");

pub(crate) struct LoadCommand<'a> {
    pub(crate) offset: usize,
    pub(crate) kind: u32,
    pub(crate) data: &'a [u8],
}

pub(crate) fn commands(image: &[u8]) -> Result<Vec<LoadCommand<'_>>> {
    let count = u32::from_le_bytes(image[16..20].try_into()?);
    let mut offset = 32;
    let mut commands = Vec::new();

    for _ in 0..count {
        let kind = u32::from_le_bytes(image[offset..offset + 4].try_into()?);
        let size = u32::from_le_bytes(image[offset + 4..offset + 8].try_into()?) as usize;
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
    let offset = u32::from_le_bytes(command.data[8..12].try_into()?) as usize;
    let bytes = command.data.get(offset..).context("install name")?;
    Ok(CStr::from_bytes_until_nul(bytes)?.to_bytes())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Section<'a> {
    pub(crate) name: [u8; 16],
    pub(crate) offset: usize,
    pub(crate) data: &'a [u8],
}

pub(crate) fn sections(image: &[u8]) -> Result<Vec<Section<'_>>> {
    let mut sections = Vec::new();

    for section in MachO::parse(image, 0)?.segments.sections().flatten() {
        let (section, data) = section?;
        sections.push(Section {
            name: section.sectname,
            offset: section.offset as usize,
            data,
        });
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
