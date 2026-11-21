//! Look up trampoline resources without traversing unrelated resource subtrees.
//!
//! Files in a scripts directory can be arbitrary executables. Read the PE headers and
//! follow the type, name and language entries individually, using the little-endian
//! on-disk layouts from `editpe` on Windows.

use std::cmp::Ordering;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Take};
use std::ops::Range;
use std::path::PathBuf;

use editpe::constants::{
    IMAGE_SUBSYSTEM_WINDOWS_GUI, PE_32_MAGIC, PE_64_MAGIC, PE_DOS_MAGIC, PE_NT_SIGNATURE,
    PE_PTR_OFFSET,
};
use editpe::types::{
    CoffHeader, GenericWindowsHeader, ImageDataDirectory, ResourceDataEntry,
    ResourceDirectoryEntry, ResourceDirectoryTable, SectionHeader, StandardHeader, WindowsHeader,
};
use fs_err::File;
use zerocopy::FromBytes;

use self::ReadError::Inconclusive;
use super::{
    Error, Launcher, LauncherKind, RESOURCE_PYTHON_PATH, RESOURCE_SCRIPT_DATA,
    RESOURCE_TRAMPOLINE_KIND, RT_RCDATA, WindowMode,
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

// Target identification reports the underlying error. Rewriting skips an entry
// point with missing or unusable resources and reports payload read and decode errors.
enum ReadError {
    Inconclusive,
    SkipRewrite(Error),
    FailRewrite(Error),
}

impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::UnexpectedEof => Self::Inconclusive,
            _ => Self::SkipRewrite(Error::Io(err)),
        }
    }
}

impl From<ReadError> for Error {
    fn from(err: ReadError) -> Self {
        match err {
            ReadError::Inconclusive => Self::UnprocessableMetadata,
            ReadError::SkipRewrite(err) | ReadError::FailRewrite(err) => err,
        }
    }
}

#[derive(Clone, Copy)]
enum Languages {
    Single,
    PreferNeutral,
}

fn integer_id(entry: ResourceDirectoryEntry) -> Result<u16, ReadError> {
    // The loader can compare just the low word of an integer ID.
    u16::try_from(entry.name_offset_or_integer_id).map_err(|_| Inconclusive)
}

pub(super) fn read_stored_target(file: File) -> Result<Option<(LauncherKind, PathBuf)>, Error> {
    // Bound allocation when inspecting arbitrary executables for discovery or ownership.
    // Each of the 32,767 UTF-16 units in an extended-length path takes at most three
    // UTF-8 bytes. A script's stored path can be longer before canonicalization, and
    // rewriting can replace that path without resolving it.
    const MAX_LOOKUP_PATH_BYTES: u64 = 3 * 32_767;
    let Some(mut file) = LauncherFile::read(file)? else {
        return Ok(None);
    };
    if file.python_path.end - file.python_path.start > MAX_LOOKUP_PATH_BYTES {
        return Err(Error::PathResourceTooLarge);
    }
    let path = file.resources.read_path(file.python_path)?;
    Ok(Some((file.kind, PathBuf::from(path))))
}

pub(super) fn read_launcher(file: File) -> Result<Option<Launcher>, Error> {
    match read_for_rewrite(file) {
        Ok(launcher) => Ok(launcher),
        Err(ReadError::SkipRewrite(_) | Inconclusive) => Ok(None),
        Err(ReadError::FailRewrite(err)) => Err(err),
    }
}

fn read_for_rewrite(file: File) -> Result<Option<Launcher>, ReadError> {
    let Some(mut file) = LauncherFile::read(file)? else {
        return Ok(None);
    };
    let mut locate_script = |policy| {
        file.resources
            .locate(file.rcdata, RESOURCE_SCRIPT_DATA, policy)
    };
    let range = match file.kind {
        LauncherKind::Script => Some(locate_script(Languages::Single)?.ok_or(Inconclusive)?),
        LauncherKind::Python => locate_script(Languages::PreferNeutral)?,
    };
    let python_path = file.resources.read_path(file.python_path)?;
    let script_data = range
        .map(|range| file.resources.read(range))
        .transpose()
        .map_err(ReadError::FailRewrite)?;
    Ok(Some(Launcher {
        kind: file.kind,
        window_mode: file.resources.window_mode,
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
    window_mode: WindowMode,
    sections: Vec<SectionHeader>,
    directory: ImageDataDirectory,
}

struct LauncherFile {
    resources: Resources,
    rcdata: u32,
    kind: LauncherKind,
    python_path: Range<u64>,
}

impl LauncherFile {
    fn read(file: File) -> Result<Option<Self>, ReadError> {
        let Some(mut resources) = read_headers(file)? else {
            return Ok(None);
        };
        // With MUI configuration, `FindResourceW` can obtain the entire RCDATA
        // tree from a language-specific sidecar.
        if resources.subdirectory(0, Key::Name("MUI"))?.is_some() {
            return Err(Inconclusive);
        }
        let Some(rcdata) = resources.subdirectory(0, Key::Id(RT_RCDATA))? else {
            return Ok(None);
        };
        let Some(range) =
            resources.locate(rcdata, RESOURCE_TRAMPOLINE_KIND, Languages::PreferNeutral)?
        else {
            return Ok(None);
        };
        let kind = resources.read_kind(range).map_err(ReadError::FailRewrite)?;
        let python_path = resources
            .locate(rcdata, RESOURCE_PYTHON_PATH, Languages::PreferNeutral)?
            .ok_or(Inconclusive)?;
        Ok(Some(Self {
            resources,
            rcdata,
            kind,
            python_path,
        }))
    }
}

impl Resources {
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

    fn seek_directory(&mut self, pos: u32, size: u32) -> Result<(), ReadError> {
        let offset = self.directory_offset(pos, size).ok_or(Inconclusive)?;
        self.file.seek(SeekFrom::Start(offset))?;
        Ok(())
    }

    fn read_entry<T: FromBytes>(&mut self, pos: u32) -> Result<T, ReadError> {
        self.seek_directory(pos, pe_size::<T>())?;
        Ok(T::read_from_io(&mut self.file)?)
    }

    fn compare_entry(
        &mut self,
        table: u32,
        index: u32,
        key: Key<'_>,
    ) -> Result<(Ordering, u32), ReadError> {
        let pos = table
            .checked_add(TABLE_SIZE + ENTRY_SIZE * index)
            .ok_or(Inconclusive)?;
        let entry: ResourceDirectoryEntry = self.read_entry(pos)?;
        let order = match key {
            Key::Id(id) => u32::from(integer_id(entry)?).cmp(&id),
            Key::Name(name) => self.compare_name(entry.name_offset_or_integer_id, name)?,
        };
        Ok((order, entry.data_entry_or_subdirectory_offset))
    }

    fn compare_name(&mut self, offset: u32, name: &str) -> Result<Ordering, ReadError> {
        let pos = offset.checked_sub(DIRECTORY_FLAG).ok_or(Inconclusive)?;
        let length = self.read_entry::<u16>(pos)?;
        self.directory_offset(pos, 2 + u32::from(length) * 2)
            .ok_or(Inconclusive)?;
        for expected in name.encode_utf16().take(usize::from(length)) {
            let order = u16::read_from_io(&mut self.file)?.cmp(&expected);
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        let order = usize::from(length).cmp(&name.encode_utf16().count());
        // Wine compares the NUL-terminated key against the counted name. A NUL
        // after the key can send its search to the other half of the table.
        if order == Ordering::Greater && u16::read_from_io(&mut self.file)? == 0 {
            return Err(Inconclusive);
        }
        Ok(order)
    }

    /// Search the sorted names or IDs.
    fn subdirectory(&mut self, table: u32, key: Key<'_>) -> Result<Option<u32>, ReadError> {
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
                        return Err(Inconclusive);
                    }
                    if mid + 1 < range.end
                        && self.compare_entry(table, mid + 1, key)?.0 != Ordering::Greater
                    {
                        return Err(Inconclusive);
                    }
                    let offset = offset.checked_sub(DIRECTORY_FLAG).ok_or(Inconclusive)?;
                    return Ok(Some(offset));
                }
                Ordering::Less => range.start = mid + 1,
                Ordering::Greater => range.end = mid,
            }
        }
        Ok(None)
    }

    fn locate(
        &mut self,
        rcdata: u32,
        name: &str,
        policy: Languages,
    ) -> Result<Option<Range<u64>>, ReadError> {
        let Some(languages) = self.subdirectory(rcdata, Key::Name(name))? else {
            return Ok(None);
        };
        self.locate_language(languages, policy).map(Some)
    }

    fn locate_language(
        &mut self,
        languages: u32,
        policy: Languages,
    ) -> Result<Range<u64>, ReadError> {
        let header: ResourceDirectoryTable = self.read_entry(languages)?;
        if header.number_of_name_entries != 0 || header.number_of_id_entries == 0 {
            return Err(Inconclusive);
        }
        let count = u32::from(header.number_of_id_entries);
        if matches!(policy, Languages::Single) && count > 1 {
            return Err(Inconclusive);
        }
        self.seek_directory(languages + TABLE_SIZE, ENTRY_SIZE * count)?;
        let entry = ResourceDirectoryEntry::read_from_io(&mut self.file)?;
        let mut language = integer_id(entry)?;
        // uv writes neutral resources. With several localized variants and no neutral
        // entry, the target selected by FindResourceW depends on the user's language.
        // Uninstall needs an unambiguous target to establish ownership of the link.
        if language != 0 && count > 1 {
            return Err(Inconclusive);
        }
        // The loader searches sorted IDs. In particular, duplicate neutral IDs
        // can make it select a different entry from the first.
        for _ in 1..count {
            let other = ResourceDirectoryEntry::read_from_io(&mut self.file)?;
            let next = integer_id(other)?;
            if next <= language {
                return Err(Inconclusive);
            }
            language = next;
        }
        let pos = entry.data_entry_or_subdirectory_offset;
        if pos & DIRECTORY_FLAG != 0 {
            return Err(Inconclusive);
        }
        let data: ResourceDataEntry = self.read_entry(pos)?;
        let offset = if data.size == 0 {
            0
        } else {
            self.offset(data.data_rva, data.size).ok_or(Inconclusive)?
        };
        Ok(offset..offset + u64::from(data.size))
    }

    fn read_kind(&mut self, range: Range<u64>) -> Result<LauncherKind, Error> {
        if range.is_empty() {
            return Err(Error::UnprocessableMetadata);
        }
        self.file.seek(SeekFrom::Start(range.start))?;
        let value = u8::read_from_io(&mut self.file)?;
        LauncherKind::from_resource_value(value).ok_or(Error::UnprocessableMetadata)
    }

    fn read_path(&mut self, range: Range<u64>) -> Result<String, ReadError> {
        let bytes = self.read(range).map_err(ReadError::FailRewrite)?;
        let path = String::from_utf8(bytes)
            .map_err(|err| ReadError::FailRewrite(Error::InvalidPath(err.utf8_error())))?;
        super::validate_python_path(&path).map_err(ReadError::SkipRewrite)?;
        Ok(path)
    }

    fn read(&mut self, range: Range<u64>) -> Result<Vec<u8>, Error> {
        self.file.seek(SeekFrom::Start(range.start))?;
        let mut data = (&mut self.file).take(range.end - range.start);
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        if data.limit() > 0 {
            return Err(Error::Io(io::ErrorKind::UnexpectedEof.into()));
        }
        Ok(bytes)
    }
}

/// Read the resource data directory and the sections used to resolve its addresses.
fn read_headers(file: File) -> Result<Option<Resources>, ReadError> {
    let file_size = file.metadata()?.len();
    if file_size < u64::from(PE_PTR_OFFSET) + 4 {
        return Ok(None);
    }
    let mut file = BufReader::new(file).take(file_size);
    if u16::read_from_io(&mut file)? != PE_DOS_MAGIC {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(PE_PTR_OFFSET.into()))?;
    let pe_offset = u64::from(u32::read_from_io(&mut file)?);
    if pe_offset > file_size {
        return Err(Inconclusive);
    }
    file.seek(SeekFrom::Start(pe_offset))?;
    if u32::read_from_io(&mut file)? != PE_NT_SIGNATURE {
        return Ok(None);
    }
    let coff = CoffHeader::read_from_io(&mut file)?;
    // The Windows loader supports up to 96 sections.
    if coff.number_of_sections > 96 {
        return Err(Inconclusive);
    }
    let optional_size = u64::from(coff.size_of_optional_header);
    if optional_size > file.limit() {
        return Err(Inconclusive);
    }
    let mut header = (&mut file).take(optional_size);
    let standard = StandardHeader::read_from_io(&mut header)?;
    let windows = match standard.magic {
        PE_32_MAGIC => {
            u32::read_from_io(&mut header)?; // BaseOfData in PE32
            GenericWindowsHeader::WindowsHeader32(WindowsHeader::read_from_io(&mut header)?)
        }
        PE_64_MAGIC => {
            GenericWindowsHeader::WindowsHeader64(WindowsHeader::read_from_io(&mut header)?)
        }
        _ => return Err(Inconclusive),
    };
    // The resource directory follows the export and import directories.
    if windows.number_of_rva_and_sizes() < 3 {
        return Ok(None);
    }
    let [_, _, directory] = <[ImageDataDirectory; 3]>::read_from_io(&mut header)?;
    if directory.virtual_address == 0 {
        return Ok(None);
    }
    if directory.size == 0 {
        return Err(Inconclusive);
    }
    header.seek(SeekFrom::End(0))?;
    let mut sections = Vec::with_capacity(usize::from(coff.number_of_sections));
    let mut previous_end = windows.size_of_headers();
    let alignment = windows.section_alignment();
    if !alignment.is_power_of_two() {
        return Err(Inconclusive);
    }
    for _ in 0..coff.number_of_sections {
        let mut section = SectionHeader::read_from_io(&mut file)?;
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
            return Err(Inconclusive);
        }
        previous_end = section
            .virtual_address
            .checked_add(section.virtual_size)
            .ok_or(Inconclusive)?;
        if previous_end > windows.size_of_image() {
            return Err(Inconclusive);
        }
        sections.push(section);
    }
    // The GUI template can target `python.exe` when `pythonw.exe` is unavailable.
    let window_mode = if windows.subsystem() == IMAGE_SUBSYSTEM_WINDOWS_GUI {
        WindowMode::Windowed
    } else {
        WindowMode::Console
    };
    Ok(Some(Resources {
        file,
        file_size,
        window_mode,
        sections,
        directory,
    }))
}
