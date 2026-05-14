//! Zero out non-deterministic fields in PE executables.
//!
//! Even with the `/Brepro`, the PE timestamp and debug directory GUID change
//! between builds. We have to zero out those fields so the final binaries are
//! byte-for-byte reproducible.
//!
//! Fields zeroed:
//! - COFF header `TimeDateStamp`
//! - Each `IMAGE_DEBUG_DIRECTORY` `TimeDateStamp`
//! - `CodeView` PDB70 `Signature` (GUID) and `Age`

use std::env;

use anyhow::{Context, Result, bail, format_err};
use goblin::pe::PE;
use goblin::pe::debug::{IMAGE_DEBUG_TYPE_CODEVIEW, ImageDebugDirectory};
use goblin::pe::section_table::SectionTable;
use uv_fs::Simplified;

/// Collected fixup locations from the PE parse.
struct DebugEntryFixup {
    /// File offset of this `IMAGE_DEBUG_DIRECTORY` entry.
    entry_file_offset: usize,
    /// The `IMAGE_DEBUG_DIRECTORY.data_type` field.
    data_type: u32,
    /// The `IMAGE_DEBUG_DIRECTORY.pointer_to_raw_data` field (file offset of the payload).
    pointer_to_raw_data: u32,
}

/// Resolve an RVA to a file offset using the PE section table.
fn rva_to_file_offset(sections: &[SectionTable], rva: u32) -> Option<usize> {
    sections.iter().find_map(|section| {
        if rva >= section.virtual_address && rva < section.virtual_address + section.virtual_size {
            Some((rva - section.virtual_address + section.pointer_to_raw_data) as usize)
        } else {
            None
        }
    })
}

fn parse_debug_entries(data: &mut [u8]) -> Result<(usize, Vec<DebugEntryFixup>)> {
    let pe = PE::parse(data)?;

    // COFF header: pe_pointer -> "PE\0\0" (4 bytes) -> CoffHeader fields.
    // `CoffHeader.time_date_stamp` is the second u32 field (offset 4).
    let coff_timestamp_offset = pe.header.dos_header.pe_pointer as usize + 4 + 4;

    let mut debug_entries = Vec::new();
    if let Some(debug_data_directory) = pe
        .header
        .optional_header
        .and_then(|opt| opt.data_directories.get_debug_table().copied())
    {
        if let Some(debug_data) = &pe.debug_data {
            let debug_dir_file_offset =
                rva_to_file_offset(&pe.sections, debug_data_directory.virtual_address)
                    .ok_or_else(|| format_err!("cannot resolve debug directory RVA"))?;
            let entry_size = size_of::<ImageDebugDirectory>();

            for (i, entry) in debug_data.entries().enumerate() {
                let entry = entry?;
                debug_entries.push(DebugEntryFixup {
                    entry_file_offset: debug_dir_file_offset + i * entry_size,
                    data_type: entry.data_type,
                    pointer_to_raw_data: entry.pointer_to_raw_data,
                });
            }
        }
    }

    Ok((coff_timestamp_offset, debug_entries))
}

fn clear_debug_entries(
    data: &mut [u8],
    coff_timestamp_offset: usize,
    debug_entries: &[DebugEntryFixup],
) {
    // Zero COFF header `TimeDateStamp`.
    data[coff_timestamp_offset..coff_timestamp_offset + 4].fill(0);

    // Zero debug directory timestamps and CodeView GUIDs.
    for entry in debug_entries {
        // Zero `IMAGE_DEBUG_DIRECTORY.time_date_stamp` (offset 4 in the struct).
        let timestamp_offset = entry.entry_file_offset + 4;
        data[timestamp_offset..timestamp_offset + 4].fill(0);

        if entry.data_type == IMAGE_DEBUG_TYPE_CODEVIEW {
            // PDB70 layout: magic[4] ("RSDS") | guid[16] | age[4] | filename...
            let payload = entry.pointer_to_raw_data as usize;
            data[payload + 4..payload + 20].fill(0); // Signature (GUID)
            data[payload + 20..payload + 24].fill(0); // Age
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("Usage: {} <file.exe> [...]", args[0]);
    }
    for path in &args[1..] {
        let mut data = fs_err::read(path)?;
        let (coff_timestamp_offset, debug_entries) = parse_debug_entries(&mut data)
            .with_context(|| format!("Failed to normalize: {}", path.user_display()))?;
        clear_debug_entries(&mut data, coff_timestamp_offset, &debug_entries);
        fs_err::write(path, &data)?;
    }
    Ok(())
}
