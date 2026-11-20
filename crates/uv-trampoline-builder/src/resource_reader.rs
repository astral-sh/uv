//! Look up trampoline resources without traversing unrelated resource subtrees.
//!
//! Files in a scripts directory can be arbitrary executables. Read the PE headers and
//! follow the type, name and language entries individually, using the little-endian
//! on-disk layouts from `editpe` on Windows.

use std::cmp::Ordering;
use std::io::{BufReader, Read, Seek, SeekFrom, Take};
use std::path::Path;

use editpe::constants::{PE_32_MAGIC, PE_64_MAGIC, PE_DOS_MAGIC, PE_NT_SIGNATURE, PE_PTR_OFFSET};
use editpe::types::{
    CoffHeader, GenericWindowsHeader, ImageDataDirectory, ResourceDataEntry,
    ResourceDirectoryEntry, ResourceDirectoryTable, SectionHeader, StandardHeader, WindowsHeader,
};
use fs_err::File;
use zerocopy::FromBytes;

use super::{Error, LauncherKind, RESOURCE_TRAMPOLINE_KIND, RT_RCDATA};

// The high bit distinguishes a string from an integer ID and a subdirectory from a leaf.
const DIRECTORY_FLAG: u32 = 1 << 31;

#[derive(Clone, Copy)]
enum Key<'a> {
    Id(u32),
    Name(&'a str),
}

pub(super) struct Resources {
    file: BufReader<File>,
    file_size: u64,
    sections: Vec<SectionHeader>,
    directory: ImageDataDirectory,
    rcdata: u32,
}

impl Resources {
    pub(super) fn open(path: &Path) -> Option<Self> {
        let mut file = BufReader::new(File::open(path).ok()?);
        let file_size = file.get_ref().metadata().ok()?.len();
        if file_size < u64::from(PE_PTR_OFFSET) + 4 {
            return None;
        }
        if u16::read_from_io(&mut file).ok()? != PE_DOS_MAGIC {
            return None;
        }
        file.seek_relative(i64::from(PE_PTR_OFFSET) - 2).ok()?;
        let pe_offset = u32::read_from_io(&mut file).ok()?;
        file.seek_relative(i64::from(pe_offset) - i64::from(PE_PTR_OFFSET) - 4)
            .ok()?;
        if u32::read_from_io(&mut file).ok()? != PE_NT_SIGNATURE {
            return None;
        }
        let (directory, sections) = read_headers(&mut file)?;
        let mut resources = Self {
            file,
            file_size,
            sections,
            directory,
            rcdata: 0,
        };
        resources.rcdata = resources.subdirectory(0, Key::Id(RT_RCDATA))?;
        Some(resources)
    }

    /// Locate a file-backed range by its relative virtual address (RVA).
    fn offset(&self, rva: u32, size: u32) -> Option<u64> {
        let section = self
            .sections
            .iter()
            .rev()
            .find(|section| section.virtual_address <= rva)?;
        let delta = rva.checked_sub(section.virtual_address)?;
        if delta.checked_add(size)? > section.size_of_raw_data.min(section.virtual_size)
            || section.pointer_to_raw_data == 0
        {
            return None;
        }
        let offset = u64::from(section.pointer_to_raw_data) + u64::from(delta);
        (offset + u64::from(size) <= self.file_size).then_some(offset)
    }

    /// Table and name offsets are relative to the directory, whereas data entries carry RVAs.
    fn seek_directory(&mut self, pos: u32, size: u32) -> Option<()> {
        if pos.checked_add(size)? > self.directory.size {
            return None;
        }
        let rva = self.directory.virtual_address.checked_add(pos)?;
        let offset = self.offset(rva, size)?;
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        Some(())
    }

    fn read_entry<T: FromBytes>(&mut self, pos: u32) -> Option<T> {
        self.seek_directory(pos, u32::try_from(size_of::<T>()).ok()?)?;
        T::read_from_io(&mut self.file).ok()
    }

    fn entry(&mut self, table: u32, index: u32) -> Option<ResourceDirectoryEntry> {
        let pos = table.checked_add(16 + 8 * index)?;
        self.read_entry(pos)
    }

    fn compare_name(&mut self, offset: u32, name: &str) -> Option<Ordering> {
        let pos = offset.checked_sub(DIRECTORY_FLAG)?;
        let length = self.read_entry::<u16>(pos)?;
        self.seek_directory(pos + 2, u32::from(length) * 2)?;
        for expected in name.encode_utf16().take(usize::from(length)) {
            let order = u16::read_from_io(&mut self.file).ok()?.cmp(&expected);
            if order != Ordering::Equal {
                return Some(order);
            }
        }
        Some(usize::from(length).cmp(&name.encode_utf16().count()))
    }

    /// Search just the sorted names or IDs in one directory.
    fn subdirectory(&mut self, table: u32, key: Key<'_>) -> Option<u32> {
        let header: ResourceDirectoryTable = self.read_entry(table)?;
        let names = u32::from(header.number_of_name_entries);
        let mut range = match key {
            Key::Id(_) => names..names + u32::from(header.number_of_id_entries),
            Key::Name(_) => 0..names,
        };
        while !range.is_empty() {
            let mid = range.start.midpoint(range.end - 1);
            let entry = self.entry(table, mid)?;
            let order = match key {
                Key::Id(id) => entry.name_offset_or_integer_id.cmp(&id),
                Key::Name(name) => self.compare_name(entry.name_offset_or_integer_id, name)?,
            };
            match order {
                Ordering::Equal => {
                    return entry
                        .data_entry_or_subdirectory_offset
                        .checked_sub(DIRECTORY_FLAG);
                }
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
            }
        }
        None
    }

    fn resource(&mut self, name: &str) -> Option<Take<&mut BufReader<File>>> {
        let languages = self.subdirectory(self.rcdata, Key::Name(name))?;
        let header: ResourceDirectoryTable = self.read_entry(languages)?;
        if header.number_of_name_entries != 0 || header.number_of_id_entries == 0 {
            return None;
        }
        // Language IDs are sorted, so the neutral resource comes first when present.
        let entry = self.entry(languages, 0)?;
        let pos = entry.data_entry_or_subdirectory_offset;
        if pos & DIRECTORY_FLAG != 0 {
            return None;
        }
        let data: ResourceDataEntry = self.read_entry(pos)?;
        if data.size > 0 {
            let offset = self.offset(data.data_rva, data.size)?;
            self.file.seek(SeekFrom::Start(offset)).ok()?;
        }
        Some((&mut self.file).take(u64::from(data.size)))
    }

    pub(super) fn kind(&mut self) -> Result<Option<LauncherKind>, Error> {
        let Some(data) = self.resource(RESOURCE_TRAMPOLINE_KIND) else {
            return Ok(None);
        };
        let value = u8::read_from_io(data).map_err(|_| Error::UnprocessableMetadata)?;
        let kind = LauncherKind::from_resource_value(value).ok_or(Error::UnprocessableMetadata)?;
        Ok(Some(kind))
    }

    pub(super) fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let mut data = self.resource(name)?;
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes).ok()?;
        (data.limit() == 0).then_some(bytes)
    }
}

/// Read the resource data directory and the sections used to resolve its addresses.
fn read_headers(file: &mut BufReader<File>) -> Option<(ImageDataDirectory, Vec<SectionHeader>)> {
    let coff = CoffHeader::read_from_io(&mut *file).ok()?;
    // The Windows loader supports up to 96 sections.
    if coff.number_of_sections > 96 {
        return None;
    }
    let mut header = (&mut *file).take(coff.size_of_optional_header.into());
    let standard = StandardHeader::read_from_io(&mut header).ok()?;
    let windows = match standard.magic {
        PE_32_MAGIC => {
            u32::read_from_io(&mut header).ok()?; // BaseOfData in PE32
            GenericWindowsHeader::WindowsHeader32(WindowsHeader::read_from_io(&mut header).ok()?)
        }
        PE_64_MAGIC => {
            GenericWindowsHeader::WindowsHeader64(WindowsHeader::read_from_io(&mut header).ok()?)
        }
        _ => return None,
    };
    // The resource directory follows the export and import directories.
    if windows.number_of_rva_and_sizes() < 3 {
        return None;
    }
    let [_, _, directory] = <[ImageDataDirectory; 3]>::read_from_io(&mut header).ok()?;
    if directory.virtual_address == 0 || directory.size == 0 {
        return None;
    }
    let remaining = header.limit();
    file.seek_relative(i64::try_from(remaining).ok()?).ok()?;
    let mut sections = Vec::with_capacity(usize::from(coff.number_of_sections));
    let mut previous_end = windows.size_of_headers();
    let alignment = windows.section_alignment();
    if !alignment.is_power_of_two() {
        return None;
    }
    for _ in 0..coff.number_of_sections {
        let section = SectionHeader::read_from_io(&mut *file).ok()?;
        // Reject ambiguous RVA mappings and offsets the loader would round down.
        if section.virtual_address < previous_end
            || !section.virtual_address.is_multiple_of(alignment)
            || (alignment >= 4096 && !section.pointer_to_raw_data.is_multiple_of(512))
            || (alignment < 4096
                && section.size_of_raw_data > 0
                && section.pointer_to_raw_data != section.virtual_address)
        {
            return None;
        }
        previous_end = section.virtual_address.checked_add(section.virtual_size)?;
        if previous_end > windows.size_of_image() {
            return None;
        }
        sections.push(section);
    }
    Some((directory, sections))
}
