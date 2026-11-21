use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use assert_cmd::prelude::OutputAssertExt;
use assert_fs::prelude::PathChild;
use editpe::{Image, ResourceEntry, ResourceEntryName, ResourceTable};
use fs_err::File;
use goblin::pe::{
    PE, options::ParseOptions, resource::ResourceEntry as DirectoryEntry, utils::find_offset,
};
use windows::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};
use windows::core::PCWSTR;

use super::{
    Launcher, LauncherKind, RESOURCE_PYTHON_PATH, RESOURCE_SCRIPT_DATA, RESOURCE_TRAMPOLINE_KIND,
    RT_RCDATA, WindowMode, get_launcher_bin, windows_python_launcher, windows_script_launcher,
    write_resources,
};

/// Use the Windows resource editor as an independent reference for resource readback.
fn update_native_resources(path: &Path, language: u16, resources: &[(&str, &[u8])]) -> Result<()> {
    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let resources = resources
        .iter()
        .map(|(name, data)| {
            Ok((
                *name,
                name.encode_utf16()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>(),
                *data,
                u32::try_from(data.len())?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    // SAFETY: The path and resource names are null-terminated, the data slices remain live for
    // every update, and the update handle is closed on both success and failure.
    #[allow(unsafe_code)]
    unsafe {
        let handle = BeginUpdateResourceW(PCWSTR(path_wide.as_ptr()), false)
            .context("BeginUpdateResourceW failed")?;
        let result = resources
            .iter()
            .try_for_each(|(name, name_wide, data, size)| {
                UpdateResourceW(
                    handle,
                    PCWSTR(RT_RCDATA as usize as *const u16),
                    PCWSTR(name_wide.as_ptr()),
                    language,
                    Some(data.as_ptr().cast()),
                    *size,
                )
                .with_context(|| format!("UpdateResourceW failed for {name} ({size} bytes)"))
            });
        let finish = EndUpdateResourceW(handle, result.is_err());
        result?;
        finish.context("EndUpdateResourceW failed")?;
    }

    Ok(())
}

fn assert_pe_layout(original: &[u8], output: &[u8]) -> Result<()> {
    let original = PE::parse(original)?;
    let output = PE::parse(output)?;
    let original_header = original
        .header
        .optional_header
        .context("Missing original PE optional header")?;
    let output_header = output
        .header
        .optional_header
        .context("Missing updated PE optional header")?;

    assert_eq!(
        original.header.coff_header.machine,
        output.header.coff_header.machine
    );
    assert_eq!(
        original_header.standard_fields.base_of_data,
        output_header.standard_fields.base_of_data,
    );
    assert_eq!(
        original_header.standard_fields.address_of_entry_point,
        output_header.standard_fields.address_of_entry_point,
    );
    assert_eq!(
        original_header.windows_fields.image_base,
        output_header.windows_fields.image_base,
    );
    assert_eq!(
        original_header.windows_fields.subsystem,
        output_header.windows_fields.subsystem,
    );

    let alignment = output_header.windows_fields.section_alignment;
    let size = u64::from(output_header.windows_fields.size_of_image);
    assert_eq!(size % u64::from(alignment), 0);
    for section in &output.sections {
        let end = u64::from(section.virtual_address)
            + u64::from(section.virtual_size.max(section.size_of_raw_data));
        assert!(end <= size, "Section extends past SizeOfImage");
    }

    Ok(())
}

/// Check that the Windows loader sees the exact resources written to the test launcher.
fn assert_native_resources(path: &Path, resources: &[(&str, &[u8])]) -> Result<()> {
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::LibraryLoader::{
        FindResourceW, LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW, LoadResource, LockResource,
        SizeofResource,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    use windows::core::{HSTRING, Owned};

    // SAFETY: The data-file handle is owned here and stays live through all lookups.
    // ReadProcessMemory checks the source pages and fills a buffer of the expected size.
    #[allow(unsafe_code)]
    unsafe {
        let module = Owned::new(LoadLibraryExW(
            &HSTRING::from(path),
            None,
            LOAD_LIBRARY_AS_DATAFILE,
        )?);
        for &(name, expected) in resources {
            let resource = FindResourceW(
                Some(*module),
                &HSTRING::from(name),
                PCWSTR(RT_RCDATA as usize as *const u16),
            );
            anyhow::ensure!(
                !resource.is_invalid(),
                "Missing {name} in {}",
                path.display()
            );
            assert_eq!(
                SizeofResource(Some(*module), resource) as usize,
                expected.len(),
                "{name}"
            );
            let data = LockResource(LoadResource(Some(*module), resource)?);
            let mut actual = vec![0u8; expected.len()];
            ReadProcessMemory(
                GetCurrentProcess(),
                data,
                actual.as_mut_ptr().cast(),
                actual.len(),
                None,
            )?;
            anyhow::ensure!(actual == expected, "Invalid {name} in {}", path.display());
        }
    }
    Ok(())
}

fn assert_other_resources_unchanged(
    original: &[u8],
    output: &[u8],
    updated_names: &[&str],
) -> Result<()> {
    let original = Image::parse(original)?;
    let output = Image::parse(output)?;
    let Some(original_resources) = original.resource_directory() else {
        return Ok(());
    };
    let output_resources = output
        .resource_directory()
        .context("Missing updated resource directory")?;

    for resource_type in original_resources.root().entries() {
        if resource_type == &ResourceEntryName::ID(RT_RCDATA) {
            let original_table = original_resources
                .root()
                .get(resource_type)
                .and_then(ResourceEntry::as_table)
                .context("Invalid original RCDATA table")?;
            let output_table = output_resources
                .root()
                .get(resource_type)
                .and_then(ResourceEntry::as_table)
                .context("Invalid updated RCDATA table")?;
            for name in original_table.entries() {
                if updated_names
                    .iter()
                    .any(|updated| name == &ResourceEntryName::from_string(updated))
                {
                    continue;
                }
                assert_eq!(original_table.get(name), output_table.get(name));
            }
        } else {
            assert_eq!(
                original_resources.root().get(resource_type),
                output_resources.root().get(resource_type),
            );
        }
    }

    Ok(())
}

fn resource_file_offset(image: &PE<'_>) -> Result<usize> {
    let header = image
        .header
        .optional_header
        .context("Missing PE optional header")?;
    let directory = header
        .data_directories
        .get_resource_table()
        .context("Missing PE resource directory")?;
    find_offset(
        usize::try_from(directory.virtual_address)?,
        &image.sections,
        header.windows_fields.file_alignment,
        &ParseOptions::default(),
    )
    .context("Missing resource file offset")
}

fn resource_section<'a>(
    image: &'a PE<'_>,
) -> Result<(&'a goblin::pe::section_table::SectionTable, usize)> {
    use editpe::types::{CoffHeader, SectionHeader};

    let resource_offset = resource_file_offset(image)?;
    let (index, section) = image
        .sections
        .iter()
        .enumerate()
        .find(|(_, section)| section.pointer_to_raw_data as usize == resource_offset)
        .context("Missing resource section")?;
    let table = image.header.dos_header.pe_pointer as usize
        + 4
        + size_of::<CoffHeader>()
        + usize::from(image.header.coff_header.size_of_optional_header);
    let virtual_size_offset = table
        + index * size_of::<SectionHeader>()
        + std::mem::offset_of!(SectionHeader, virtual_size);
    Ok((section, virtual_size_offset))
}

fn named_resource(launcher: &[u8], key: &str) -> Result<(usize, DirectoryEntry)> {
    use goblin::pe::resource::ImageResourceDirectory;

    let image = PE::parse(launcher)?;
    let offset = resource_file_offset(&image)?;
    let rcdata = image
        .resource_data
        .context("Missing resource data")?
        .entries()
        .find_by_id(u16::try_from(RT_RCDATA)?)?
        .context("Missing RCDATA entry")?;
    let header = image
        .header
        .optional_header
        .context("Missing PE optional header")?;
    let mut directory = *header
        .data_directories
        .get_resource_table()
        .context("Missing resource directory")?;
    directory.virtual_address += rcdata.offset_to_directory();
    let table = ImageResourceDirectory::parse(
        launcher,
        directory,
        &image.sections,
        header.windows_fields.file_alignment,
    )?;
    let entries = offset
        + usize::try_from(rcdata.offset_to_directory())?
        + size_of::<ImageResourceDirectory>();
    let ResourceEntryName::Name(expected) = ResourceEntryName::from_string(key) else {
        unreachable!();
    };
    for name in table.next_iter(entries, launcher)? {
        let name = name?;
        let start = offset + usize::try_from(name.name_offset())?;
        if name.name_is_string() && launcher[start..].starts_with(&expected) {
            return Ok((offset, name));
        }
    }
    anyhow::bail!("Missing resource {key}")
}

/// A PE resource size is untrusted even when the Windows loader accepts the image.
#[test]
fn oversized_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let mut launcher = windows_python_launcher("python.exe", WindowMode::Console)?;
    let (resource_offset, name) = named_resource(&launcher, RESOURCE_TRAMPOLINE_KIND)?;
    let language = name
        .next_depth(&launcher[resource_offset..])?
        .context("Missing resource language")?;
    let entry_offset = resource_offset
        + usize::try_from(
            language
                .offset_to_data()
                .context("Missing resource data entry")?,
        )?;
    let size_offset = entry_offset + 4;
    assert_eq!(
        u32::from_le_bytes(launcher[size_offset..size_offset + 4].try_into()?),
        1,
    );
    let image = PE::parse(&launcher)?;
    let (section, virtual_size_offset) = resource_section(&image)?;
    let rva = u32::from_le_bytes(launcher[entry_offset..size_offset].try_into()?);
    let delta = rva - section.virtual_address;
    let raw_size = section.size_of_raw_data;
    launcher.push(0);
    assert!(resource_offset + usize::try_from(raw_size)? < launcher.len());

    // Give each section bound a turn as the smaller one. The kind is decoded
    // from its first byte, so its payload can fill the rest of the section.
    for virtual_size in [raw_size + 1, raw_size - 1] {
        launcher[virtual_size_offset..virtual_size_offset + 4]
            .copy_from_slice(&virtual_size.to_le_bytes());
        let size = raw_size.min(virtual_size) - delta;
        launcher[size_offset..size_offset + 4].copy_from_slice(&size.to_le_bytes());
        fs_err::write(path.path(), &launcher)?;
        assert_eq!(
            Launcher::try_from_path(path.path())?
                .context("Missing launcher metadata at section boundary")?
                .kind,
            LauncherKind::Python,
        );

        launcher[size_offset..size_offset + 4].copy_from_slice(&(size + 1).to_le_bytes());
        fs_err::write(path.path(), &launcher)?;
        assert!(
            Launcher::try_from_path(path.path())?.is_none(),
            "{virtual_size}"
        );
        assert!(matches!(
            super::read_launcher_target(path.path()),
            Err(super::Error::UnprocessableMetadata)
        ));
    }
    Ok(())
}

/// Reading the kind needs one byte, but its entire declared extent must exist on disk.
#[test]
fn truncated_kind_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let kind = b"\x02truncated";
    let mut launcher = write_resources(
        get_launcher_bin(WindowMode::Console)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, kind),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
        ],
    )?;
    fs_err::write(path.path(), &launcher)?;
    assert!(Launcher::try_from_path(path.path())?.is_some());

    let offset = launcher
        .windows(kind.len())
        .position(|bytes| bytes == kind)
        .context("Missing kind resource")?;
    launcher.truncate(offset + 1);
    fs_err::write(path.path(), launcher)?;
    assert!(Launcher::try_from_path(path.path())?.is_none());
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));
    Ok(())
}

/// Directory offsets are relative to the declared directory RVA. Leaf RVAs can point
/// into a different section with a different file-to-memory displacement.
#[test]
fn relocated_resource_directory() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let mut launcher = write_resources(
        get_launcher_bin(WindowMode::Console)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"payload"),
        ],
    )?;
    let image = PE::parse(&launcher)?;
    let header = image
        .header
        .optional_header
        .context("Missing PE optional header")?;
    let (directory_end, directory) =
        header.data_directories.data_directories[2].context("Missing resource directory")?;
    let directory_offset = resource_file_offset(&image)?;
    // Copy the tables into the interior of the first section, leaving the data RVAs
    // pointing at their original payloads. This fixture is inspected as a data file.
    let section = &image.sections[0];
    let offset = section.pointer_to_raw_data as usize + 16;
    let address = section.virtual_address + 16;
    assert!(directory.size + 16 <= section.size_of_raw_data);
    let size = directory.size as usize;
    let field = image.header.dos_header.pe_pointer as usize + 24 + directory_end - 8;
    launcher.copy_within(directory_offset..directory_offset + size, offset);
    // Invalidate the old root while retaining the payloads referenced by RVA.
    let root_size = size_of::<editpe::types::ResourceDirectoryTable>();
    launcher[directory_offset..directory_offset + root_size].fill(0);
    assert_eq!(
        &launcher[field..field + 4],
        &directory.virtual_address.to_le_bytes()
    );
    launcher[field..field + 4].copy_from_slice(&address.to_le_bytes());
    fs_err::write(path.path(), &launcher)?;

    let result = Launcher::try_from_path(path.path())?.context("Missing relocated resources")?;
    assert_eq!(result.python_path(), Path::new("C:/Python312/python.exe"));
    assert_eq!(result.script_data.as_deref(), Some(b"payload".as_slice()));
    Ok(())
}

/// Windows uses the raw size for sections whose virtual size is unspecified.
#[test]
fn resource_section_with_zero_virtual_size() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let python = which::which("python")?;
    let mut launcher = windows_python_launcher(&python, WindowMode::Console)?;
    let image = PE::parse(&launcher)?;
    let (_, virtual_size) = resource_section(&image)?;
    launcher[virtual_size..virtual_size + 4].fill(0);
    fs_err::write(path.path(), launcher)?;

    Command::new(path.path())
        .args(["-c", "pass"])
        .assert()
        .success();
    let result = Launcher::try_from_path(path.path())?.context("Missing Python launcher")?;
    assert_eq!(result.python_path(), python);
    Ok(())
}

/// An executable in `Scripts` can contain an arbitrarily deep resource tree even
/// when it has no RCDATA. Probing it should stop at the root directory.
#[test]
fn deeply_nested_unrelated_resource() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("other.exe");
    // Reserve space for a chain of single-entry tables and a final empty table.
    let depth = 16_384;
    let mut launcher = write_resources(
        get_launcher_bin(WindowMode::Console)?,
        &[("PADDING", &vec![0; depth * 24 + 16])],
    )?;
    let offset = resource_file_offset(&PE::parse(&launcher)?)?;
    let tables = &mut launcher[offset..offset + depth * 24 + 16];
    tables.fill(0);
    for (index, table) in tables.as_chunks_mut::<24>().0.iter_mut().enumerate() {
        table[14] = 1; // NumberOfIdEntries
        table[16] = 123; // An unrelated resource ID
        let next = 0x8000_0000 | u32::try_from((index + 1) * 24)?;
        table[20..24].copy_from_slice(&next.to_le_bytes());
    }
    fs_err::write(path.path(), &launcher)?;

    assert_eq!(super::read_launcher_target(path.path())?, None);
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn locked_executable_reports_target_lookup_error() -> Result<()> {
    use fs_err::os::windows::fs::OpenOptionsExt;

    let dir = assert_fs::TempDir::new()?;
    let path = dir.child("locked.exe");
    let _locked = fs_err::OpenOptions::new()
        .create_new(true)
        .write(true)
        .share_mode(0)
        .open(path.path())?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::Io(_))
    ));
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

/// MUI configuration permits the loader to obtain any launcher resource from a sidecar.
#[test]
fn mui_configuration_precludes_launcher_identification() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let launcher = windows_python_launcher("C:/Python312/python.exe", WindowMode::Console)?;
    let mut image = Image::parse(launcher)?;
    let mut resources = image
        .resource_directory()
        .context("Missing resources")?
        .clone();
    resources.root_mut().insert(
        ResourceEntryName::from_string("MUI"),
        ResourceEntry::Table(ResourceTable::default()),
    );
    // All of the launcher's RCDATA can live in the language-specific file.
    let mut sidecar_only = resources.clone();
    sidecar_only
        .root_mut()
        .remove(ResourceEntryName::ID(RT_RCDATA))
        .context("Missing RCDATA")?;
    for directory in [resources, sidecar_only] {
        image.set_resource_directory(directory)?;
        fs_err::write(path.path(), image.data())?;
        assert!(matches!(
            super::read_launcher_target(path.path()),
            Err(super::Error::UnprocessableMetadata)
        ));
        assert!(Launcher::try_from_path(path.path())?.is_none());
    }
    Ok(())
}

#[test]
fn nul_after_resource_name_precludes_launcher_identification() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    // The middle name in the sorted table shares the complete lookup key.
    let image = |other: &str| {
        write_resources(
            get_launcher_bin(WindowMode::Console)?,
            &[
                (RESOURCE_PYTHON_PATH, b"python.exe"),
                (other, b"other.exe"),
                (RESOURCE_TRAMPOLINE_KIND, &[2]),
            ],
        )
    };
    fs_err::write(path.path(), image("UV_PYTHON_PATH_")?)?;
    assert_eq!(
        super::read_launcher_target(path.path())?,
        Some((LauncherKind::Python, temp_dir.path().join("python.exe")))
    );
    fs_err::write(path.path(), image("UV_PYTHON_PATH\0")?)?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn ambiguous_rcdata_id_precludes_launcher_identification() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let launcher = windows_python_launcher("python.exe", WindowMode::Console)?;
    let mut image = Image::parse(launcher)?;
    let rcdata = image
        .resource_directory()
        .context("Missing resources")?
        .root()
        .get(ResourceEntryName::ID(RT_RCDATA))
        .context("Missing RCDATA")?
        .clone();
    let mut resources = editpe::ResourceDirectory::default();
    resources
        .root_mut()
        .insert(ResourceEntryName::ID(0x1_0000 | RT_RCDATA), rcdata);
    image.set_resource_directory(resources)?;
    fs_err::write(path.path(), image.data())?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn localized_launcher_target_must_be_unambiguous() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    fs_err::write(path.path(), get_launcher_bin(WindowMode::Console)?)?;
    update_native_resources(
        path.path(),
        0x0409,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, b"missing-en.exe"),
        ],
    )?;
    assert_eq!(
        super::read_launcher_target(path.path())?,
        Some((LauncherKind::Python, temp_dir.path().join("missing-en.exe")))
    );

    update_native_resources(
        path.path(),
        0x0407,
        &[(RESOURCE_PYTHON_PATH, b"missing-de.exe")],
    )?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));

    let python = which::which("python")?;
    update_native_resources(
        path.path(),
        0,
        &[(
            RESOURCE_PYTHON_PATH,
            python.to_str().context("Invalid path")?.as_bytes(),
        )],
    )?;
    Command::new(path.path())
        .args(["-c", "pass"])
        .assert()
        .success();
    assert_eq!(
        super::read_launcher_target(path.path())?,
        Some((LauncherKind::Python, python))
    );

    // A binary search can find the second neutral entry when IDs are duplicated.
    let mut neutral = fs_err::read(path.path())?;
    let (offset, name) = named_resource(&neutral, RESOURCE_PYTHON_PATH)?;
    let second = offset + usize::try_from(name.offset_to_directory())? + 16 + 8;
    assert_eq!(&neutral[second..second + 4], &0x0407_u32.to_le_bytes());
    neutral[second..second + 4].fill(0);
    fs_err::write(path.path(), neutral)?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));
    Ok(())
}

#[test]
fn resources_match_native_windows_updates() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    for mode in [WindowMode::Console, WindowMode::Windowed] {
        let launcher = get_launcher_bin(mode)?;
        let header = PE::parse(launcher)?
            .header
            .optional_header
            .context("Missing PE optional header")?;
        let file_alignment = usize::try_from(header.windows_fields.file_alignment)?;
        let section_alignment = usize::try_from(header.windows_fields.section_alignment)?;
        let empty_resources: &[(&str, &[u8])] = &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, &[]),
        ];
        let empty = write_resources(launcher, empty_resources)?;
        let resource_overhead = usize::try_from(
            PE::parse(&empty)?
                .header
                .optional_header
                .context("Missing updated PE optional header")?
                .data_directories
                .get_resource_table()
                .context("Missing PE resource table")?
                .size,
        )?;
        let mut sizes = vec![1, 65_535, 65_536, 65_537];
        for alignment in [file_alignment, section_alignment] {
            // Account for the resource tables and existing resources so the complete directory
            // crosses the boundary, not just the script payload.
            let boundary = alignment - resource_overhead % alignment;
            sizes.extend([boundary - 1, boundary, boundary + 1]);
        }
        // UpdateResourceW rejects empty resources, which are covered separately below.
        sizes.retain(|size| *size > 0);
        sizes.sort_unstable();
        sizes.dedup();

        for size in sizes {
            let native_path = temp_dir.child(format!("native-{mode:?}-{size}.exe"));
            let edited_path = temp_dir.child(format!("edited-{mode:?}-{size}.exe"));
            let payload = vec![0x5a; size];
            let resources: &[(&str, &[u8])] = &[
                (RESOURCE_TRAMPOLINE_KIND, &[1]),
                (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
                (RESOURCE_SCRIPT_DATA, &payload),
            ];
            fs_err::write(native_path.path(), launcher)?;
            update_native_resources(native_path.path(), 0, resources)?;
            let native = fs_err::read(native_path.path())?;
            let edited = write_resources(launcher, resources)?;
            fs_err::write(edited_path.path(), &edited)?;

            assert_native_resources(edited_path.path(), resources)?;
            assert_pe_layout(launcher, &native)?;
            assert_pe_layout(launcher, &edited)?;
            assert_other_resources_unchanged(
                launcher,
                &edited,
                &[
                    RESOURCE_TRAMPOLINE_KIND,
                    RESOURCE_PYTHON_PATH,
                    RESOURCE_SCRIPT_DATA,
                ],
            )?;

            let native = Launcher::try_from_path(native_path.path())?
                .context("Windows-generated launcher was not recognized")?;
            let edited = Launcher::try_from_path(edited_path.path())?
                .context("editpe-generated launcher was not recognized")?;
            assert_eq!(native.kind, LauncherKind::Script);
            assert_eq!(edited.kind, native.kind);
            assert_eq!(native.python_path(), Path::new("C:/Python312/python.exe"));
            assert_eq!(edited.python_path(), native.python_path());
            assert_eq!(native.script_data.as_deref(), Some(payload.as_slice()));
            assert_eq!(edited.script_data.as_deref(), Some(payload.as_slice()));
        }
    }

    Ok(())
}

#[test]
fn header_read_failure_reports_target_lookup_error() -> Result<()> {
    use fs_err::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;

    use super::resource_reader::{read_launcher, read_target};

    let dir = assert_fs::TempDir::new()?;
    let path = dir.child("proxy.exe");
    let image = windows_python_launcher("python.exe", WindowMode::Console)?;
    fs_err::write(path.path(), &image)?;

    // Grant access to metadata so the first denied read is the DOS header.
    let file = fs_err::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0)
        .open(path.path())?;
    assert_eq!(file.metadata()?.len(), image.len() as u64);
    assert!(matches!(
        read_target(file.try_clone()?),
        Err(super::Error::Io(err)) if err.kind() == std::io::ErrorKind::PermissionDenied
    ));
    assert!(read_launcher(file)?.is_none());
    Ok(())
}

#[test]
fn missing_script_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    fs_err::write(
        path.path(),
        write_resources(
            get_launcher_bin(WindowMode::Console)?,
            &[
                (RESOURCE_TRAMPOLINE_KIND, &[1]),
                (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            ],
        )?,
    )?;

    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn multiple_script_payloads_are_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    fs_err::write(path.path(), get_launcher_bin(WindowMode::Console)?)?;
    update_native_resources(
        path.path(),
        0x0409,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"localized payload"),
        ],
    )?;
    assert!(Launcher::try_from_path(path.path())?.is_some());
    update_native_resources(
        path.path(),
        0,
        &[(RESOURCE_SCRIPT_DATA, b"neutral payload")],
    )?;

    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn duplicate_script_names_are_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let other = "UV_SCRIPT_DATB";
    let mut launcher = write_resources(
        get_launcher_bin(WindowMode::Console)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"first payload"),
            (other, b"second payload"),
        ],
    )?;
    fs_err::write(path.path(), &launcher)?;
    assert!(Launcher::try_from_path(path.path())?.is_some());

    // Rename the second entry in place to give the sorted table duplicate keys.
    let name: Vec<_> = other.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let offset = launcher
        .windows(name.len())
        .position(|bytes| bytes == name)
        .context("Missing second name")?;
    launcher[offset + name.len() - 2] = b'A';
    fs_err::write(path.path(), launcher)?;
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn python_proxy_keeps_its_neutral_payload() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    fs_err::write(path.path(), get_launcher_bin(WindowMode::Console)?)?;
    update_native_resources(
        path.path(),
        0,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"neutral payload"),
        ],
    )?;
    update_native_resources(
        path.path(),
        0x0409,
        &[(RESOURCE_SCRIPT_DATA, b"localized payload")],
    )?;
    let launcher = Launcher::try_from_path(path.path())?.context("Missing Python proxy")?;
    let rewritten = temp_dir.child("rewritten.exe");
    launcher.write_to_file(&mut File::create(rewritten.path())?)?;
    let launcher = Launcher::try_from_path(rewritten.path())?.context("Missing rewritten proxy")?;
    assert_eq!(launcher.kind, LauncherKind::Python);
    assert_eq!(
        launcher.script_data.as_deref(),
        Some(b"neutral payload".as_slice())
    );
    Ok(())
}

#[test]
#[cfg(target_pointer_width = "64")]
fn wow64_proxy_target_depends_on_the_launchers_view() -> Result<()> {
    // A 32-bit process sees System32 via the Sysnative alias. A 64-bit
    // process cannot resolve that alias and might diagnose a broken proxy.
    let dir = assert_fs::TempDir::new()?;
    let proxy = dir.child("proxy.exe");
    let windows = std::env::var_os("SystemRoot").context("Missing Windows directory")?;
    let redirected = Path::new(&windows).join(r"Sysnative\python.exe");
    let bytes = write_resources(
        include_bytes!("../trampolines/uv-trampoline-i686-console.exe"),
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (
                RESOURCE_PYTHON_PATH,
                redirected.to_str().context("Invalid path")?.as_bytes(),
            ),
        ],
    )?;
    fs_err::write(proxy.path(), bytes)?;
    assert!(matches!(
        super::read_launcher_target(proxy.path()),
        Err(super::Error::UnprocessableMetadata)
    ));

    fs_err::write(
        proxy.path(),
        windows_python_launcher(&redirected, WindowMode::Console)?,
    )?;
    assert_eq!(
        super::read_launcher_target(proxy.path())?,
        Some((LauncherKind::Python, redirected))
    );
    Ok(())
}

#[test]
fn empty_target_is_not_a_launcher() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    fs_err::write(
        path.path(),
        write_resources(
            get_launcher_bin(WindowMode::Console)?,
            &[
                (RESOURCE_TRAMPOLINE_KIND, &[2]),
                (RESOURCE_PYTHON_PATH, &[]),
            ],
        )?,
    )?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::EmptyPath)
    ));
    assert!(Launcher::try_from_path(path.path())?.is_none());

    assert!(matches!(
        windows_python_launcher("", WindowMode::Console),
        Err(super::Error::EmptyPath)
    ));
    assert!(matches!(
        windows_script_launcher("pass", false, ""),
        Err(super::Error::EmptyPath)
    ));
    fs_err::write(
        path.path(),
        windows_python_launcher("python.exe", WindowMode::Console)?,
    )?;
    let launcher = Launcher::try_from_path(path.path())?.context("Missing Python proxy")?;
    assert!(matches!(
        launcher.with_python_path(""),
        Err(super::Error::EmptyPath)
    ));
    Ok(())
}

#[test]
fn nul_in_target_is_rejected() -> Result<()> {
    let python = "python.exe\0suffix";
    assert!(matches!(
        windows_python_launcher(python, WindowMode::Console),
        Err(super::Error::NulInPath)
    ));
    let dir = assert_fs::TempDir::new()?;
    let path = dir.child("proxy.exe");
    fs_err::write(
        path.path(),
        write_resources(
            get_launcher_bin(WindowMode::Console)?,
            &[
                (RESOURCE_TRAMPOLINE_KIND, &[2]),
                (RESOURCE_PYTHON_PATH, python.as_bytes()),
            ],
        )?,
    )?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::NulInPath)
    ));
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn unpaired_surrogate_in_target_is_rejected() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let python = Path::new("invalid")
        .join(OsString::from_wide(&[0xd800]))
        .join("python.exe");
    assert!(matches!(
        windows_python_launcher(&python, WindowMode::Console),
        Err(super::Error::NonUtf8Path { path }) if path == python
    ));
}

#[test]
fn interpreter_lookup_limits_path_allocation() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let python = "界".repeat(32_767);
    fs_err::write(
        path.path(),
        windows_python_launcher(&python, WindowMode::Console)?,
    )?;
    let (_, target) = super::read_launcher_target(path.path())?.context("Missing Python proxy")?;
    assert_eq!(target, temp_dir.path().join(&python));

    // Exceed the limit by one byte that would cause a UTF-8 decoding error
    // if the reader didn't reject the declared size first.
    let mut payload = python.into_bytes();
    payload.push(0xff);
    fs_err::write(
        path.path(),
        write_resources(
            get_launcher_bin(WindowMode::Console)?,
            &[
                (RESOURCE_TRAMPOLINE_KIND, &[2]),
                (RESOURCE_PYTHON_PATH, &payload),
            ],
        )?,
    )?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::PathResourceTooLarge)
    ));
    assert!(matches!(
        Launcher::try_from_path(path.path()),
        Err(super::Error::InvalidPath(_))
    ));
    Ok(())
}

#[test]
fn long_stored_paths_can_be_rewritten() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("script.exe");
    let replacement = which::which("python")?;
    let proxy = temp_dir.child("proxy.exe");
    fs_err::write(
        proxy.path(),
        windows_python_launcher(&replacement, WindowMode::Console)?,
    )?;
    let python = format!("{}proxy.exe", r".\".repeat(50_000));
    fs_err::write(
        path.path(),
        windows_script_launcher("print('long path')", false, &python)?,
    )?;
    // Joining a relative target to a verbatim launcher path collapses the
    // redundant components before the trampoline asks Windows to open it.
    // Python can pass the verbatim path to `CreateProcessW` as supplied.
    // `std::process::Command` strips its prefix for short executable paths.
    Command::new(&replacement)
        .args([
            "-c",
            "import subprocess, sys; subprocess.run([sys.argv[1]], executable=sys.argv[1], check=True)",
        ])
        .arg(fs_err::canonicalize(path.path())?)
        .assert()
        .success()
        .stdout("long path\r\n");
    let launcher = Launcher::try_from_path(path.path())?.context("Missing script launcher")?;
    assert_eq!(launcher.python_path().as_os_str(), python.as_str());
    launcher
        .with_python_path(&replacement)?
        .write_to_file(&mut File::create(path.path())?)?;
    fs_err::remove_file(proxy.path())?;
    Command::new(path.path())
        .assert()
        .success()
        .stdout("long path\r\n");
    Ok(())
}

#[test]
fn relative_target_is_resolved_beside_the_launcher() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let target = Path::new("python.exe");
    fs_err::write(
        path.path(),
        windows_python_launcher(target, WindowMode::Console)?,
    )?;
    assert_eq!(
        super::read_launcher_target(path.path())?,
        Some((LauncherKind::Python, temp_dir.path().join(target)))
    );
    let launcher = Launcher::try_from_path(path.path())?.context("Missing Python proxy")?;
    assert_eq!(launcher.python_path(), target);
    Ok(())
}

#[test]
fn drive_relative_target_cannot_establish_ownership() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let target = Path::new("C:python.exe");
    fs_err::write(
        path.path(),
        windows_python_launcher(target, WindowMode::Console)?,
    )?;
    assert!(matches!(
        super::read_launcher_target(path.path()),
        Err(super::Error::UnprocessableMetadata)
    ));
    let launcher = Launcher::try_from_path(path.path())?.context("Missing Python proxy")?;
    assert_eq!(launcher.python_path(), target);
    Ok(())
}

#[test]
fn python_proxy_target_is_independent_of_script_languages() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let python = "C:/Python312/python.exe";
    fs_err::write(path.path(), get_launcher_bin(WindowMode::Console)?)?;
    update_native_resources(
        path.path(),
        0x0407,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, python.as_bytes()),
            (RESOURCE_SCRIPT_DATA, b"unused"),
        ],
    )?;
    assert!(Launcher::try_from_path(path.path())?.is_some());
    update_native_resources(path.path(), 0x0409, &[(RESOURCE_SCRIPT_DATA, b"unused")])?;
    assert_eq!(
        super::read_launcher_target(path.path())?,
        Some((LauncherKind::Python, python.into()))
    );
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn empty_script_resource_is_preserved() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("empty.exe");
    let resources: &[(&str, &[u8])] = &[
        (RESOURCE_TRAMPOLINE_KIND, &[1]),
        (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
        (RESOURCE_SCRIPT_DATA, &[]),
    ];
    fs_err::write(
        path.path(),
        write_resources(get_launcher_bin(WindowMode::Console)?, resources)?,
    )?;

    let launcher = Launcher::try_from_path(path.path())?
        .context("Launcher with an empty script resource was not recognized")?;
    assert_eq!(launcher.script_data, Some(Vec::new()));

    let destination = temp_dir.child("rewritten.exe");
    launcher.write_to_file(&mut File::create(destination.path())?)?;
    let rewritten = Launcher::try_from_path(destination.path())?
        .context("Rewritten launcher with an empty script resource was not recognized")?;
    assert_eq!(rewritten.script_data, Some(Vec::new()));
    Ok(())
}

#[test]
fn rewriting_preserves_the_window_mode() -> Result<()> {
    use editpe::constants::{IMAGE_SUBSYSTEM_WINDOWS_CUI, IMAGE_SUBSYSTEM_WINDOWS_GUI};

    fn subsystem(path: &Path) -> Result<u16> {
        let bytes = fs_err::read(path)?;
        let header = PE::parse(&bytes)?
            .header
            .optional_header
            .context("Missing PE optional header")?;
        Ok(header.windows_fields.subsystem)
    }

    let dir = assert_fs::TempDir::new()?;
    let source = dir.child("source.exe");
    let destination = dir.child("destination.exe");
    for (is_gui, name, expected) in [
        (true, "python.exe", IMAGE_SUBSYSTEM_WINDOWS_GUI),
        (false, "pythonw.exe", IMAGE_SUBSYSTEM_WINDOWS_CUI),
    ] {
        fs_err::write(
            source.path(),
            windows_script_launcher("pass", is_gui, name)?,
        )?;
        assert_eq!(subsystem(source.path())?, expected);
        let launcher =
            Launcher::try_from_path(source.path())?.context("Missing script launcher")?;
        launcher
            .with_python_path(dir.path().join(name))?
            .write_to_file(&mut File::create(destination.path())?)?;
        assert_eq!(subsystem(destination.path())?, expected);
    }
    Ok(())
}

#[test]
fn large_script_survives_launcher_rewrites() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let original_path = temp_dir.child("original.exe");
    let rewritten_path = temp_dir.child("rewritten.exe");
    let repeated_path = temp_dir.child("repeated.exe");
    let original_python = Path::new(r"C:\old Python\π\python.exe");
    let python = which::which("python")?;

    let script = format!(
        "{}print('large trampoline')\n",
        "# Padding keeps the stored ZIP larger than 64 KiB.\n".repeat(2_000),
    );
    let original = windows_script_launcher(&script, false, original_python)?;
    fs_err::write(original_path.path(), &original)?;
    let launcher = Launcher::try_from_path(original_path.path())?
        .context("Original launcher was not recognized")?;
    assert_eq!(launcher.python_path(), original_python);
    let launcher = launcher.with_python_path(&python)?;
    let script_data = launcher
        .script_data
        .as_deref()
        .context("Missing original script data")?;
    assert!(script_data.len() > 65_536);

    launcher.write_to_file(&mut File::create(rewritten_path.path())?)?;
    let rewritten = Launcher::try_from_path(rewritten_path.path())?
        .context("Rewritten launcher was not recognized")?;
    assert_eq!(rewritten.python_path(), python);
    assert_eq!(rewritten.script_data.as_deref(), Some(script_data));
    rewritten.write_to_file(&mut File::create(repeated_path.path())?)?;

    for path in [rewritten_path.path(), repeated_path.path()] {
        Command::new(path)
            .assert()
            .success()
            .stdout("large trampoline\r\n");
    }

    Ok(())
}
