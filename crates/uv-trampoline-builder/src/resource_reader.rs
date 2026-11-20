//! Look up trampoline resources without traversing unrelated resource subtrees.
//!
//! Files in a scripts directory can be arbitrary executables. Read the PE headers and
//! follow the type, name and language entries individually, using the little-endian
//! on-disk layouts from `editpe` on Windows.

use std::cmp::Ordering;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Take};
use std::ops::Range;
use std::path::Path;

use editpe::constants::{PE_32_MAGIC, PE_64_MAGIC, PE_DOS_MAGIC, PE_NT_SIGNATURE, PE_PTR_OFFSET};
use editpe::types::{
    CoffHeader, GenericWindowsHeader, ImageDataDirectory, ResourceDataEntry,
    ResourceDirectoryEntry, ResourceDirectoryTable, SectionHeader, StandardHeader, WindowsHeader,
};
use fs_err::File;
use zerocopy::FromBytes;

use super::{Error, LauncherKind, RESOURCE_SCRIPT_DATA, RESOURCE_TRAMPOLINE_KIND, RT_RCDATA};

// The high bit distinguishes a string from an integer ID and a subdirectory from a leaf.
const DIRECTORY_FLAG: u32 = 1 << 31;

#[derive(Clone, Copy)]
enum Key<'a> {
    Id(u32),
    Name(&'a str),
}

enum Lookup {
    Directory(u32),
    Missing,
}

// Take tracks the cursor within the file length. Its absolute seeks use
// seek_relative on the BufReader, retaining buffered bytes for nearby entries.
type ImageFile = Take<BufReader<File>>;

pub(super) struct Resources {
    file: ImageFile,
    file_size: u64,
    sections: Vec<SectionHeader>,
    directory: ImageDataDirectory,
    rcdata: u32,
}

impl Resources {
    pub(super) fn open(path: &Path) -> Option<Self> {
        let file = File::open(path).ok()?;
        let file_size = file.metadata().ok()?.len();
        if file_size < u64::from(PE_PTR_OFFSET) + 4 {
            return None;
        }
        let mut file = BufReader::new(file).take(file_size);
        if u16::read_from_io(&mut file).ok()? != PE_DOS_MAGIC {
            return None;
        }
        file.seek(SeekFrom::Start(PE_PTR_OFFSET.into())).ok()?;
        let pe_offset = u32::read_from_io(&mut file).ok()?;
        file.seek(SeekFrom::Start(pe_offset.into())).ok()?;
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
        let Lookup::Directory(rcdata) = resources.subdirectory(0, Key::Id(RT_RCDATA))? else {
            return None;
        };
        resources.rcdata = rcdata;
        // With MUI configuration, FindResourceW can obtain a different target from
        // a language-specific sidecar. Only infer ownership from a standalone image.
        if let Lookup::Directory(_) = resources.subdirectory(0, Key::Name("MUI"))? {
            return None;
        }
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
    fn directory_offset(&self, pos: u32, size: u32) -> Option<u64> {
        if pos.checked_add(size)? > self.directory.size {
            return None;
        }
        let rva = self.directory.virtual_address.checked_add(pos)?;
        self.offset(rva, size)
    }

    fn seek_directory(&mut self, pos: u32, size: u32) -> Option<()> {
        let offset = self.directory_offset(pos, size)?;
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        Some(())
    }

    fn read_entry<T: FromBytes>(&mut self, pos: u32) -> Option<T> {
        self.seek_directory(pos, u32::try_from(size_of::<T>()).ok()?)?;
        T::read_from_io(&mut self.file).ok()
    }

    fn compare_entry(&mut self, table: u32, index: u32, key: Key<'_>) -> Option<(Ordering, u32)> {
        let pos = table.checked_add(16 + 8 * index)?;
        let entry: ResourceDirectoryEntry = self.read_entry(pos)?;
        let order = match key {
            Key::Id(id) => entry.name_offset_or_integer_id.cmp(&id),
            Key::Name(name) => self.compare_name(entry.name_offset_or_integer_id, name)?,
        };
        Some((order, entry.data_entry_or_subdirectory_offset))
    }

    fn compare_name(&mut self, offset: u32, name: &str) -> Option<Ordering> {
        let pos = offset.checked_sub(DIRECTORY_FLAG)?;
        let length = self.read_entry::<u16>(pos)?;
        self.directory_offset(pos, 2 + u32::from(length) * 2)?;
        for expected in name.encode_utf16().take(usize::from(length)) {
            let order = u16::read_from_io(&mut self.file).ok()?.cmp(&expected);
            if order != Ordering::Equal {
                return Some(order);
            }
        }
        Some(usize::from(length).cmp(&name.encode_utf16().count()))
    }

    /// Search the sorted names or IDs. Return `None` if the directory cannot be searched.
    fn subdirectory(&mut self, table: u32, key: Key<'_>) -> Option<Lookup> {
        let header: ResourceDirectoryTable = self.read_entry(table)?;
        let names = u32::from(header.number_of_name_entries);
        let mut range = match key {
            Key::Id(_) => names..names + u32::from(header.number_of_id_entries),
            Key::Name(_) => 0..names,
        };
        while !range.is_empty() {
            let mid = range.start.midpoint(range.end - 1);
            let (order, offset) = self.compare_entry(table, mid, key)?;
            match order {
                Ordering::Equal => {
                    // Duplicates in a sorted table are adjacent. Entries immediately
                    // outside the remaining range already compared unequal.
                    if mid > range.start
                        && self.compare_entry(table, mid - 1, key)?.0 != Ordering::Less
                    {
                        return None;
                    }
                    if mid + 1 < range.end
                        && self.compare_entry(table, mid + 1, key)?.0 != Ordering::Greater
                    {
                        return None;
                    }
                    let offset = offset.checked_sub(DIRECTORY_FLAG)?;
                    return Some(Lookup::Directory(offset));
                }
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
            }
        }
        Some(Lookup::Missing)
    }

    fn data_entry(&mut self, name: &str) -> Option<ResourceDataEntry> {
        let Lookup::Directory(languages) = self.subdirectory(self.rcdata, Key::Name(name))? else {
            return None;
        };
        let header: ResourceDirectoryTable = self.read_entry(languages)?;
        if header.number_of_name_entries != 0 || header.number_of_id_entries == 0 {
            return None;
        }
        let count = u32::from(header.number_of_id_entries);
        // Python locates the ZIP from the end of the executable, independently
        // of the language the Windows resource loader would select.
        if name == RESOURCE_SCRIPT_DATA && count != 1 {
            return None;
        }
        self.seek_directory(languages + 16, 8 * count)?;
        let entry = ResourceDirectoryEntry::read_from_io(&mut self.file).ok()?;
        let mut language = u16::try_from(entry.name_offset_or_integer_id).ok()?;
        // uv writes neutral resources. With several localized variants and no neutral
        // entry, the target selected by FindResourceW depends on the user's language.
        // Uninstall needs an unambiguous target to establish ownership of the link.
        if language != 0 && count > 1 {
            return None;
        }
        // The loader searches sorted IDs. In particular, duplicate neutral IDs
        // can make it select a different entry from the first.
        for _ in 1..count {
            let other = ResourceDirectoryEntry::read_from_io(&mut self.file).ok()?;
            let next = u16::try_from(other.name_offset_or_integer_id).ok()?;
            if next <= language {
                return None;
            }
            language = next;
        }
        let pos = entry.data_entry_or_subdirectory_offset;
        if pos & DIRECTORY_FLAG != 0 {
            return None;
        }
        self.read_entry(pos)
    }

    fn resource(&mut self, name: &str) -> io::Result<Option<Take<&mut ImageFile>>> {
        let Some(data) = self.data_entry(name) else {
            return Ok(None);
        };
        let offset = if data.size == 0 {
            0
        } else if let Some(offset) = self.offset(data.data_rva, data.size) {
            offset
        } else {
            return Ok(None);
        };
        let size = u64::from(data.size);
        if name == RESOURCE_SCRIPT_DATA
            && let Some(archive) = self.last_archive()?
            && (archive.start < offset || archive.end > offset + size)
        {
            return Ok(None);
        }
        self.file.seek(SeekFrom::Start(offset))?;
        Ok(Some((&mut self.file).take(size)))
    }

    /// Python searches for a ZIP in the entire executable, including its overlay.
    /// A rewrite only copies the named payload, so decline an archive outside it.
    fn last_archive(&mut self) -> io::Result<Option<Range<u64>>> {
        // zipimport allows a 16-bit comment, a 22-byte end record and up to 76
        // bytes of ZIP64 records. See CPython's Lib/zipimport.py (_read_directory).
        const RECORDS_SIZE: usize = 22 + 76;
        const SEARCH_SIZE: usize = 65_535 + RECORDS_SIZE;
        // Reading the resource table of a small launcher often buffers the ZIP
        // through EOF. Use it if it includes room for all the end records.
        let buffer = self.file.get_ref().buffer();
        if buffer.len() as u64 == self.file.limit() && buffer.len() <= SEARCH_SIZE {
            let start = self.file_size - self.file.limit();
            if let Some(archive) = archive_range(buffer, start)
                && archive.end - start >= RECORDS_SIZE as u64
            {
                return Ok(Some(archive));
            }
        }
        let len = usize::try_from(self.file_size)
            .unwrap_or(usize::MAX)
            .min(SEARCH_SIZE);
        let start = self.file_size - len as u64;
        let mut tail = vec![0; len];
        self.file.seek(SeekFrom::Start(start))?;
        self.file.read_exact(&mut tail)?;
        Ok(archive_range(&tail, start))
    }

    pub(super) fn kind(&mut self) -> Result<Option<LauncherKind>, Error> {
        let Some(data) = self.resource(RESOURCE_TRAMPOLINE_KIND)? else {
            return Ok(None);
        };
        if data.limit() == 0 {
            return Err(Error::UnprocessableMetadata);
        }
        let value = u8::read_from_io(data)?;
        let kind = LauncherKind::from_resource_value(value).ok_or(Error::UnprocessableMetadata)?;
        Ok(Some(kind))
    }

    pub(super) fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(mut data) = self.resource(name)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        Ok((data.limit() == 0).then_some(bytes))
    }
}

/// Locate the last ZIP's start by subtracting the central directory size and
/// its archive-relative offset from the position of the end record.
fn archive_range(tail: &[u8], start: u64) -> Option<Range<u64>> {
    let pos = tail
        .windows(22)
        .rposition(|record| record.starts_with(b"PK\x05\x06"))?;
    let end = start + pos as u64 + 22;
    let (header, size, offset) = if pos >= 76 && tail[pos - 76..].starts_with(b"PK\x06\x06") {
        let record = &tail[pos - 76..pos];
        let size = u64::read_from_bytes(&record[40..48]).ok()?;
        let offset = u64::read_from_bytes(&record[48..56]).ok()?;
        (pos - 76, size, offset)
    } else {
        let record = &tail[pos..];
        let size = u32::read_from_bytes(&record[12..16]).ok()?;
        let offset = u32::read_from_bytes(&record[16..20]).ok()?;
        (pos, u64::from(size), u64::from(offset))
    };
    let base = (start + header as u64)
        .saturating_sub(size)
        .saturating_sub(offset);
    Some(base..end)
}

/// Read the resource data directory and the sections used to resolve its addresses.
fn read_headers(file: &mut ImageFile) -> Option<(ImageDataDirectory, Vec<SectionHeader>)> {
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
    header.seek(SeekFrom::End(0)).ok()?;
    let mut sections = Vec::with_capacity(usize::from(coff.number_of_sections));
    let mut previous_end = windows.size_of_headers();
    let alignment = windows.section_alignment();
    if !alignment.is_power_of_two() {
        return None;
    }
    for _ in 0..coff.number_of_sections {
        let mut section = SectionHeader::read_from_io(&mut *file).ok()?;
        // Windows maps the raw data when the section has no explicit virtual size.
        if section.virtual_size == 0 {
            section.virtual_size = section.size_of_raw_data;
        }
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
