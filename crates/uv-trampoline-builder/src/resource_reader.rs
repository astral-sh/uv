//! Look up trampoline resources without traversing unrelated resource subtrees.
//!
//! Files in a scripts directory can be arbitrary executables. Read the PE headers and
//! follow the type, name and language entries individually, using the little-endian
//! on-disk layouts from `editpe` on Windows.

use std::cmp::Ordering;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Take};
use std::ops::Range;
use std::path::{Path, PathBuf};

use editpe::constants::{PE_32_MAGIC, PE_64_MAGIC, PE_DOS_MAGIC, PE_NT_SIGNATURE, PE_PTR_OFFSET};
use editpe::types::{
    CoffHeader, GenericWindowsHeader, ImageDataDirectory, ResourceDataEntry,
    ResourceDirectoryEntry, ResourceDirectoryTable, SectionHeader, StandardHeader, WindowsHeader,
};
use fs_err::File;
use zerocopy::FromBytes;

use super::{
    Error, Launcher, LauncherKind, RESOURCE_PYTHON_PATH, RESOURCE_SCRIPT_DATA,
    RESOURCE_TRAMPOLINE_KIND, RT_RCDATA,
};

// The high bit distinguishes a string from an integer ID and a subdirectory from a leaf.
const DIRECTORY_FLAG: u32 = 1 << 31;
const TABLE_SIZE: u32 = pe_size::<ResourceDirectoryTable>();
const ENTRY_SIZE: u32 = pe_size::<ResourceDirectoryEntry>();

const fn pe_size<T>() -> u32 {
    const { assert!(size_of::<T>() <= u32::MAX as usize) }
    #[expect(clippy::cast_possible_truncation, reason = "checked at compile time")]
    {
        size_of::<T>() as u32
    }
}

#[derive(Clone, Copy)]
enum Key<'a> {
    Id(u32),
    Name(&'a str),
}

enum Lookup<T> {
    Found(T),
    Missing,
}

#[derive(Clone, Copy)]
enum Languages {
    Single,
    PreferNeutral,
}

pub(super) fn read_target(path: &Path) -> Result<Option<(LauncherKind, PathBuf)>, Error> {
    let Lookup::Found((mut resources, kind_languages)) =
        Resources::open_launcher(path).ok_or(Error::UnprocessableMetadata)?
    else {
        return Ok(None);
    };
    let (kind, target) = resources
        .target(kind_languages)?
        .ok_or(Error::UnprocessableMetadata)?;
    let mut target = PathBuf::from(target);
    if target.is_relative() {
        // The trampoline joins the stored path to the parent of current_exe().
        let executable = std::path::absolute(path)?;
        let parent = executable.parent().ok_or(Error::UnprocessableMetadata)?;
        target = parent.join(target);
        // A drive-relative target such as C:python.exe depends on the
        // working directory for that drive even after the join.
        if target.is_relative() {
            return Err(Error::UnprocessableMetadata);
        }
    }
    Ok(Some((kind, target)))
}

pub(super) fn read_launcher(path: &Path) -> Result<Option<Launcher>, Error> {
    let Some(Lookup::Found((mut resources, kind_languages))) = Resources::open_launcher(path)
    else {
        return Ok(None);
    };
    let Some((kind, python_path)) = resources.target(kind_languages)? else {
        return Ok(None);
    };
    let languages = match kind {
        LauncherKind::Script => Languages::Single,
        LauncherKind::Python => Languages::PreferNeutral,
    };
    let script_data = match resources.locate(RESOURCE_SCRIPT_DATA, languages) {
        Some(Lookup::Missing) if kind == LauncherKind::Python => None,
        Some(Lookup::Missing) | None => return Ok(None),
        Some(Lookup::Found(range)) => Some(resources.read(range)?),
    };
    Ok(Some(Launcher {
        kind,
        python_path,
        script_data,
    }))
}

// Take tracks the cursor within the file length. Its absolute seeks use
// seek_relative on the BufReader, retaining buffered bytes for nearby entries.
type ImageFile = Take<BufReader<File>>;

struct Resources {
    file: ImageFile,
    file_size: u64,
    sections: Vec<SectionHeader>,
    directory: ImageDataDirectory,
    rcdata: u32,
}

impl Resources {
    /// Find the kind resource's language directory. Return `None` when the
    /// file can't establish whether the loader can find the resource.
    fn open_launcher(path: &Path) -> Option<Lookup<(Self, u32)>> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Some(Lookup::Missing),
            Err(_) => return None,
        };
        let file_size = file.metadata().ok()?.len();
        if file_size < u64::from(PE_PTR_OFFSET) + 4 {
            return Some(Lookup::Missing);
        }
        let mut file = BufReader::new(file).take(file_size);
        if u16::read_from_io(&mut file).ok()? != PE_DOS_MAGIC {
            return Some(Lookup::Missing);
        }
        file.seek(SeekFrom::Start(PE_PTR_OFFSET.into())).ok()?;
        let pe_offset = u32::read_from_io(&mut file).ok()?;
        file.seek(SeekFrom::Start(pe_offset.into())).ok()?;
        if u32::read_from_io(&mut file).ok()? != PE_NT_SIGNATURE {
            return Some(Lookup::Missing);
        }
        let Lookup::Found((directory, sections)) = read_headers(&mut file)? else {
            return Some(Lookup::Missing);
        };
        let mut resources = Self {
            file,
            file_size,
            sections,
            directory,
            rcdata: 0,
        };
        // With MUI configuration, `FindResourceW` can obtain the entire RCDATA
        // tree from a language-specific sidecar.
        if let Lookup::Found(_) = resources.subdirectory(0, Key::Name("MUI"))? {
            return None;
        }
        let Lookup::Found(rcdata) = resources.subdirectory(0, Key::Id(RT_RCDATA))? else {
            return Some(Lookup::Missing);
        };
        resources.rcdata = rcdata;
        let Lookup::Found(kind_languages) =
            resources.subdirectory(rcdata, Key::Name(RESOURCE_TRAMPOLINE_KIND))?
        else {
            return Some(Lookup::Missing);
        };
        Some(Lookup::Found((resources, kind_languages)))
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
        self.seek_directory(pos, pe_size::<T>())?;
        T::read_from_io(&mut self.file).ok()
    }

    fn compare_entry(&mut self, table: u32, index: u32, key: Key<'_>) -> Option<(Ordering, u32)> {
        let pos = table.checked_add(TABLE_SIZE + ENTRY_SIZE * index)?;
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
    fn subdirectory(&mut self, table: u32, key: Key<'_>) -> Option<Lookup<u32>> {
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
                    return Some(Lookup::Found(offset));
                }
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
            }
        }
        Some(Lookup::Missing)
    }

    fn locate(&mut self, name: &str, policy: Languages) -> Option<Lookup<Range<u64>>> {
        let Lookup::Found(languages) = self.subdirectory(self.rcdata, Key::Name(name))? else {
            return Some(Lookup::Missing);
        };
        self.locate_language(languages, policy).map(Lookup::Found)
    }

    fn locate_language(&mut self, languages: u32, policy: Languages) -> Option<Range<u64>> {
        let header: ResourceDirectoryTable = self.read_entry(languages)?;
        if header.number_of_name_entries != 0 || header.number_of_id_entries == 0 {
            return None;
        }
        let count = u32::from(header.number_of_id_entries);
        if matches!(policy, Languages::Single) && count > 1 {
            return None;
        }
        self.seek_directory(languages + TABLE_SIZE, ENTRY_SIZE * count)?;
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
        let data: ResourceDataEntry = self.read_entry(pos)?;
        let offset = if data.size == 0 {
            0
        } else {
            self.offset(data.data_rva, data.size)?
        };
        Some(offset..offset + u64::from(data.size))
    }

    fn target(&mut self, kind_languages: u32) -> Result<Option<(LauncherKind, String)>, Error> {
        let Some(payload) = self.locate_language(kind_languages, Languages::PreferNeutral) else {
            return Ok(None);
        };
        if payload.is_empty() {
            return Err(Error::UnprocessableMetadata);
        }
        self.file.seek(SeekFrom::Start(payload.start))?;
        let value = u8::read_from_io(&mut self.file)?;
        let kind = LauncherKind::from_resource_value(value).ok_or(Error::UnprocessableMetadata)?;
        let Some(Lookup::Found(path)) = self.locate(RESOURCE_PYTHON_PATH, Languages::PreferNeutral)
        else {
            return Ok(None);
        };
        let length = path.end - path.start;
        if length == 0 || length > super::MAX_PYTHON_PATH_BYTES as u64 {
            return Ok(None);
        }
        let path = String::from_utf8(self.read(path)?)
            .map_err(|err| Error::InvalidPath(err.utf8_error()))?;
        Ok(Some((kind, path)))
    }

    fn read(&mut self, range: Range<u64>) -> io::Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(range.start))?;
        let mut data = (&mut self.file).take(range.end - range.start);
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        if data.limit() > 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(bytes)
    }
}

/// Read the resource data directory and the sections used to resolve its addresses.
fn read_headers(file: &mut ImageFile) -> Option<Lookup<(ImageDataDirectory, Vec<SectionHeader>)>> {
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
        return Some(Lookup::Missing);
    }
    let [_, _, directory] = <[ImageDataDirectory; 3]>::read_from_io(&mut header).ok()?;
    if directory.virtual_address == 0 {
        return Some(Lookup::Missing);
    }
    if directory.size == 0 {
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
    Some(Lookup::Found((directory, sections)))
}
