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
    RT_RCDATA, WindowMode, get_launcher_bin, script_archive, windows_python_launcher,
    windows_script_launcher, write_resources,
};

/// Use the Windows resource editor as an independent reference for resource readback.
fn write_native_resources(
    path: &Path,
    launcher: &[u8],
    language: u16,
    resources: &[(&str, &[u8])],
) -> Result<Vec<u8>> {
    fs_err::write(path, launcher)?;
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

    Ok(fs_err::read(path)?)
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

/// Locate the first named RCDATA entry, which is the Python path in these fixtures.
fn path_resource(launcher: &[u8]) -> Result<(usize, DirectoryEntry)> {
    let image = PE::parse(launcher)?;
    let offset = resource_file_offset(&image)?;
    let rcdata = image
        .resource_data
        .context("Missing resource data")?
        .entries()
        .find_by_id(u16::try_from(RT_RCDATA)?)?
        .context("Missing RCDATA entry")?;
    let name = rcdata
        .next_depth(&launcher[offset..])?
        .context("Missing named resource")?;
    Ok((offset, name))
}

/// A PE resource size is untrusted even when the Windows loader accepts the image.
#[test]
fn oversized_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let python_path = b"C:/Python312/python.exe";
    let mut launcher = write_resources(
        get_launcher_bin(false)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, python_path),
        ],
    )?;
    fs_err::write(path.path(), &launcher)?;
    assert_eq!(
        Launcher::try_from_path(path.path())?
            .context("Missing launcher metadata")?
            .python_path,
        Path::new("C:/Python312/python.exe"),
    );

    let (resource_offset, name) = path_resource(&launcher)?;
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
        u32::from_le_bytes(launcher[size_offset..size_offset + 4].try_into()?) as usize,
        python_path.len(),
    );
    // The path starts within the image. Claiming a full image's worth of bytes
    // extends past its section, but the trailing padding keeps the range inside the file.
    let size = u32::try_from(launcher.len())?;
    launcher[size_offset..size_offset + 4].copy_from_slice(&size.to_le_bytes());
    launcher.resize(2 * launcher.len(), 0);
    fs_err::write(path.path(), &launcher)?;

    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

/// Reading the kind needs one byte, but its entire declared extent must exist on disk.
#[test]
fn truncated_kind_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let kind = b"\x02truncated";
    let mut launcher = write_resources(
        get_launcher_bin(false)?,
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
    Ok(())
}

/// Directory offsets are relative to the declared directory RVA. Leaf RVAs can point
/// into a different section with a different file-to-memory displacement.
#[test]
fn relocated_resource_directory() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let mut launcher = write_resources(
        get_launcher_bin(false)?,
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
    assert_eq!(
        &launcher[field..field + 4],
        &directory.virtual_address.to_le_bytes()
    );
    launcher[field..field + 4].copy_from_slice(&address.to_le_bytes());
    fs_err::write(path.path(), &launcher)?;

    let result = Launcher::try_from_path(path.path())?.context("Missing relocated resources")?;
    assert_eq!(result.python_path, Path::new("C:/Python312/python.exe"));
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
    let resource_offset = resource_file_offset(&image)?;
    let index = image
        .sections
        .iter()
        .position(|section| section.pointer_to_raw_data as usize == resource_offset)
        .context("Missing resource section")?;
    let table = image.header.dos_header.pe_pointer as usize
        + 24
        + usize::from(image.header.coff_header.size_of_optional_header);
    let virtual_size = table + 40 * index + 8;
    launcher[virtual_size..virtual_size + 4].fill(0);
    fs_err::write(path.path(), launcher)?;

    Command::new(path.path())
        .args(["-c", "pass"])
        .assert()
        .success();
    let result = Launcher::try_from_path(path.path())?.context("Missing Python launcher")?;
    assert_eq!(result.python_path, python);
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
        get_launcher_bin(false)?,
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

    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

/// MUI configuration permits the loader to obtain the Python path from a sidecar.
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
    image.set_resource_directory(resources)?;
    fs_err::write(path.path(), image.data())?;

    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn localized_launcher_target_must_be_unambiguous() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let english = write_native_resources(
        path.path(),
        get_launcher_bin(false)?,
        0x0409,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, b"missing-en.exe"),
        ],
    )?;
    let launcher = Launcher::try_from_path(path.path())?.context("Missing localized launcher")?;
    assert_eq!(launcher.python_path, Path::new("missing-en.exe"));

    let localized = write_native_resources(
        path.path(),
        &english,
        0x0407,
        &[(RESOURCE_PYTHON_PATH, b"missing-de.exe")],
    )?;
    assert!(Launcher::try_from_path(path.path())?.is_none());

    let python = which::which("python")?;
    let mut neutral = write_native_resources(
        path.path(),
        &localized,
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
    let launcher = Launcher::try_from_path(path.path())?.context("Missing neutral launcher")?;
    assert_eq!(launcher.python_path, python);

    // A binary search can find the second neutral entry when IDs are duplicated.
    let (offset, name) = path_resource(&neutral)?;
    let second = offset + usize::try_from(name.offset_to_directory())? + 16 + 8;
    assert_eq!(&neutral[second..second + 4], &0x0407_u32.to_le_bytes());
    neutral[second..second + 4].fill(0);
    fs_err::write(path.path(), neutral)?;
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn resources_match_native_windows_updates() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;

    for is_gui in [false, true] {
        let launcher = get_launcher_bin(is_gui)?;
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
            let native_path = temp_dir.child(format!("native-{is_gui}-{size}.exe"));
            let edited_path = temp_dir.child(format!("edited-{is_gui}-{size}.exe"));
            let payload = vec![0x5a; size];
            let resources: &[(&str, &[u8])] = &[
                (RESOURCE_TRAMPOLINE_KIND, &[1]),
                (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
                (RESOURCE_SCRIPT_DATA, &payload),
            ];
            let native = write_native_resources(native_path.path(), launcher, 0, resources)?;
            let edited = write_resources(launcher, resources)?;
            fs_err::write(edited_path.path(), &edited)?;

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
            assert_eq!(edited.python_path, native.python_path);
            assert_eq!(native.script_data.as_deref(), Some(payload.as_slice()));
            assert_eq!(edited.script_data.as_deref(), Some(payload.as_slice()));
        }
    }

    Ok(())
}

#[test]
fn missing_script_resource_is_rejected() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    fs_err::write(
        path.path(),
        write_resources(
            get_launcher_bin(false)?,
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
    let launcher = write_native_resources(
        path.path(),
        get_launcher_bin(false)?,
        0x0409,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"localized payload"),
        ],
    )?;
    assert!(Launcher::try_from_path(path.path())?.is_some());
    write_native_resources(
        path.path(),
        &launcher,
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
        get_launcher_bin(false)?,
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

/// A Python proxy doesn't pass itself to zipimport, even if it contains an archive.
#[test]
fn python_proxy_keeps_its_optional_payload() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let mut bytes = write_resources(
        get_launcher_bin(false)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, b"C:/Python312/python.exe"),
            (RESOURCE_SCRIPT_DATA, b"optional payload"),
        ],
    )?;
    bytes.extend(script_archive("print('appended')")?);
    fs_err::write(path.path(), bytes)?;
    let launcher = Launcher::try_from_path(path.path())?.context("Missing Python proxy")?;
    assert_eq!(
        launcher.script_data.as_deref(),
        Some(b"optional payload".as_slice())
    );
    Ok(())
}

#[test]
fn python_proxy_target_is_independent_of_script_languages() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("proxy.exe");
    let python = "C:/Python312/python.exe";
    let bytes = write_native_resources(
        path.path(),
        get_launcher_bin(false)?,
        0x0407,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[2]),
            (RESOURCE_PYTHON_PATH, python.as_bytes()),
            (RESOURCE_SCRIPT_DATA, b"unused"),
        ],
    )?;
    assert!(Launcher::try_from_path(path.path())?.is_some());
    write_native_resources(
        path.path(),
        &bytes,
        0x0409,
        &[(RESOURCE_SCRIPT_DATA, b"unused")],
    )?;
    assert_eq!(
        Launcher::try_read_target(path.path())?,
        Some((LauncherKind::Python, python.into()))
    );
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

/// Python searches from the end of the file for a ZIP, including the PE overlay.
#[test]
fn appended_archive_is_not_rewritten() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let python = which::which("python")?;
    let mut bytes = windows_script_launcher("print('embedded')", false, &python)?;
    bytes.extend_from_slice(b"trailing data");
    fs_err::write(path.path(), &bytes)?;
    assert!(Launcher::try_from_path(path.path())?.is_some());
    bytes.extend(script_archive("print('appended')")?);
    fs_err::write(path.path(), bytes)?;
    Command::new(path.path())
        .assert()
        .success()
        .stdout("appended\r\n");
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn truncated_appended_end_record_is_not_rewritten() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let mut bytes = windows_script_launcher("pass", false, "python.exe")?;
    bytes.extend_from_slice(b"PK\x05\x06");
    fs_err::write(path.path(), bytes)?;
    assert!(Launcher::try_from_path(path.path())?.is_none());
    Ok(())
}

#[test]
fn archive_start_before_script_resource_is_not_rewritten() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let path = temp_dir.child("launcher.exe");
    let python = which::which("python")?;
    let archive = script_archive("print('complete')")?;
    // The ZIP is contiguous in the executable, but copying UV_SCRIPT_DATA
    // alone would leave the local file header behind.
    let bytes = write_resources(
        get_launcher_bin(false)?,
        &[
            (RESOURCE_TRAMPOLINE_KIND, &[1]),
            (
                RESOURCE_PYTHON_PATH,
                python.to_str().context("Invalid Python path")?.as_bytes(),
            ),
            ("UV_SCRIPT", &archive[..4]),
            (RESOURCE_SCRIPT_DATA, &archive[4..]),
        ],
    )?;
    fs_err::write(path.path(), bytes)?;
    Command::new(path.path())
        .assert()
        .success()
        .stdout("complete\r\n");
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
        write_resources(get_launcher_bin(false)?, resources)?,
    )?;

    let launcher = Launcher::try_from_path(path.path())?
        .context("Launcher with an empty script resource was not recognized")?;
    assert_eq!(launcher.script_data, Some(Vec::new()));

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
    assert_eq!(launcher.python_path, original_python);
    let script_data = launcher
        .script_data
        .clone()
        .context("Missing original script data")?;
    assert!(script_data.len() > 65_536);

    {
        let mut file = File::create(rewritten_path.path())?;
        launcher
            .with_python_path(python.clone())
            .write_to_file(&mut file, false)?;
    }
    let launcher = Launcher::try_from_path(rewritten_path.path())?
        .context("Rewritten launcher was not recognized")?;
    assert_eq!(launcher.python_path, python);
    assert_eq!(
        launcher.script_data.as_deref(),
        Some(script_data.as_slice())
    );
    {
        let mut file = File::create(repeated_path.path())?;
        launcher.write_to_file(&mut file, false)?;
    }

    for path in [rewritten_path.path(), repeated_path.path()] {
        Command::new(path)
            .assert()
            .success()
            .stdout("large trampoline\r\n");
    }

    Ok(())
}
