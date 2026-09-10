use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use assert_cmd::prelude::OutputAssertExt;
use assert_fs::prelude::PathChild;
use editpe::{Image, ResourceEntry, ResourceEntryName};
use fs_err::File;
use goblin::pe::PE;
use windows::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};
use windows::core::PCWSTR;

use super::{
    Launcher, LauncherKind, RESOURCE_PYTHON_PATH, RESOURCE_SCRIPT_DATA, RESOURCE_TRAMPOLINE_KIND,
    RT_RCDATA, get_launcher_bin, windows_script_launcher, write_resources,
};

/// Use the Windows resource editor as an independent reference for resource readback.
fn write_native_resources(
    path: &Path,
    launcher: &[u8],
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
        let handle = BeginUpdateResourceW(PCWSTR(path_wide.as_ptr()), false)?;
        let result = resources.iter().try_for_each(|(name, data, size)| {
            UpdateResourceW(
                handle,
                PCWSTR(RT_RCDATA as usize as *const u16),
                PCWSTR(name.as_ptr()),
                0,
                Some(data.as_ptr().cast()),
                *size,
            )
        });
        let finish = EndUpdateResourceW(handle, result.is_err());
        result?;
        finish?;
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
        let mut sizes = vec![0, 1, 65_535, 65_536, 65_537];
        for alignment in [file_alignment, section_alignment] {
            // Account for the resource tables and existing resources so the complete directory
            // crosses the boundary, not just the script payload.
            let boundary = alignment - resource_overhead % alignment;
            sizes.extend([boundary - 1, boundary, boundary + 1]);
        }
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
            let native = write_native_resources(native_path.path(), launcher, resources)?;
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
