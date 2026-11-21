//! A script trampoline passes its executable to Python's `zipimport`, which
//! searches back from EOF for an archive. Copying the named resource into a
//! new image can change which archive is visible to the interpreter.
//!
//! See [the search in CPython 3.12](https://github.com/python/cpython/blob/v3.12.0/Lib/zipimport.py#L327)
//! and [ZIP64 in CPython 3.14](https://github.com/python/cpython/blob/v3.14.0/Lib/zipimport.py#L333).

use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;

use zerocopy::FromBytes;

use super::resource_reader::ImageFile;

// zipimport allows a 16-bit comment and a 22-byte end record. Versions
// supporting ZIP64 look back another 76 bytes. See Lib/zipimport.py.
const ZIP_MAX_COMMENT: u64 = 65_535;
const ZIP_RECORDS_SIZE: u64 = 22 + 76;
const ZIP_SEARCH_SIZE: u64 = ZIP_MAX_COMMENT + ZIP_RECORDS_SIZE;

#[derive(PartialEq, Eq)]
struct Archive {
    range: Range<u64>,
    zip64: bool,
}

/// Whether extracting the named payload retains the ZIP selected in the file.
pub(super) fn can_extract(
    file: &mut ImageFile,
    file_size: u64,
    payload: &Range<u64>,
) -> io::Result<bool> {
    let source = last_archive(file, file_size, 0..file_size)?;
    if let Some(archive) = &source
        && (archive.range.start < payload.start
            || archive.range.end > payload.end
            || file_size - archive.range.end > ZIP_MAX_COMMENT)
    {
        return Ok(false);
    }
    // Dropping trailing data can expose a hidden end record or change
    // whether zipimport selects the ZIP64 end record.
    Ok(source == last_archive(file, file_size, payload.clone())?)
}

fn last_archive(
    file: &mut ImageFile,
    file_size: u64,
    range: Range<u64>,
) -> io::Result<Option<Archive>> {
    let start = range.start.max(range.end.saturating_sub(ZIP_SEARCH_SIZE));
    // Reading the resource table of a small launcher often buffers the ZIP
    // through EOF. Use it if it includes room for all the end records.
    let buffer = file.get_ref().buffer();
    let buffered = file_size - file.limit();
    if buffered >= start && buffered + buffer.len() as u64 == range.end {
        if let Some(archive) = archive_range(buffer, buffered)
            && archive.range.end - buffered >= ZIP_RECORDS_SIZE
        {
            return Ok(Some(archive));
        }
    }
    file.seek(SeekFrom::Start(start))?;
    let mut tail = vec![0; usize::try_from(range.end - start).expect("bounded ZIP search")];
    file.read_exact(&mut tail)?;
    Ok(archive_range(&tail, start))
}

/// Whether the archive in the payload is still visible in the generated image.
pub(super) fn can_embed(payload: &[u8], image: &[u8]) -> bool {
    visible_archive(payload) == visible_archive(image)
}

/// Return the archive bytes visible within the search window common to supported Pythons.
fn visible_archive(bytes: &[u8]) -> Option<(&[u8], bool)> {
    let len = bytes.len() as u64;
    let start = len.saturating_sub(ZIP_SEARCH_SIZE);
    let Archive { range, zip64 } = archive_range(&bytes[usize::try_from(start).ok()?..], start)?;
    if len.checked_sub(range.end)? > ZIP_MAX_COMMENT {
        return None;
    }
    let data = bytes.get(usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?)?;
    Some((data, zip64))
}

/// Locate the last ZIP's start by subtracting the central directory size and
/// its archive-relative offset from the position of the end record.
fn archive_range(tail: &[u8], start: u64) -> Option<Archive> {
    let pos = tail.windows(4).rposition(|bytes| bytes == b"PK\x05\x06")?;
    let end = start + pos as u64 + 22;
    if tail.len() - pos < 22 {
        // zipimport selects the last signature before checking its length.
        // The required record extends beyond EOF and thus beyond the payload.
        return Some(Archive {
            range: start + pos as u64..end,
            zip64: false,
        });
    }
    // zipimport uses the ZIP64 record only if the *last* ZIP64 signature is
    // 76 bytes before the ordinary end record.
    let zip64 = pos >= 76
        && tail[pos - 76..]
            .windows(4)
            .rposition(|bytes| bytes == b"PK\x06\x06")
            == Some(0);
    let (header, size, offset) = if zip64 {
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
    Some(Archive {
        range: base..end,
        zip64,
    })
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use anyhow::{Context, Result};
    use assert_cmd::prelude::OutputAssertExt;
    use assert_fs::prelude::PathChild;

    use crate::{
        Launcher, LauncherKind, RESOURCE_PYTHON_PATH, RESOURCE_SCRIPT_DATA,
        RESOURCE_TRAMPOLINE_KIND, get_launcher_bin, script_archive, windows_script_launcher,
        write_resources,
    };

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
    fn archive_hidden_by_trailing_data_is_not_rewritten() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let path = temp_dir.child("launcher.exe");
        let mut bytes = windows_script_launcher("pass", false, "python.exe")?;
        let end = bytes
            .windows(4)
            .rposition(|bytes| bytes == b"PK\x05\x06")
            .context("Missing ZIP end record")?
            + 22;
        for (trailing, visible) in [(65_535, true), (65_536, false), (70_000, false)] {
            bytes.resize(end + trailing, 0);
            fs_err::write(path.path(), &bytes)?;
            assert_eq!(Launcher::try_from_path(path.path())?.is_some(), visible);
        }
        Ok(())
    }

    #[test]
    fn rewrite_declines_a_zip_hidden_by_section_padding() -> Result<()> {
        let mut archive = script_archive("pass")?;
        let end = archive.len();
        assert_eq!(&archive[end - 22..end - 18], b"PK\x05\x06");
        archive[end - 2..].copy_from_slice(&u16::MAX.to_le_bytes());
        archive.resize(end + usize::from(u16::MAX), 0);
        let launcher = Launcher {
            kind: LauncherKind::Script,
            python_path: "python.exe".into(),
            script_data: Some(archive),
        };
        // The payload's end record is exactly at the edge of the search window.
        // Any bytes following the payload in the new executable will hide it.
        assert!(launcher.into_bytes(false)?.is_none());
        Ok(())
    }

    #[test]
    fn overlay_shadowing_zip64_record_is_not_rewritten() -> Result<()> {
        use async_zip::base::write::ZipFileWriter;
        use async_zip::{Compression, ZipEntryBuilder};
        use futures_lite::future::block_on;
        use futures_lite::io::Cursor;

        let mut archive = ZipFileWriter::new(Cursor::new(Vec::new())).force_zip64();
        let entry = ZipEntryBuilder::new("__main__.py".to_string().into(), Compression::Stored);
        block_on(archive.write_entry_whole(entry, b"pass"))?;
        let payload = block_on(archive.close())?.into_inner();
        assert_eq!(&payload[payload.len() - 98..][..4], b"PK\x06\x06");

        let temp_dir = assert_fs::TempDir::new()?;
        let path = temp_dir.child("launcher.exe");
        let mut bytes = write_resources(
            get_launcher_bin(false)?,
            &[
                (RESOURCE_TRAMPOLINE_KIND, &[1]),
                (RESOURCE_PYTHON_PATH, b"python.exe"),
                (RESOURCE_SCRIPT_DATA, &payload),
            ],
        )?;
        fs_err::write(path.path(), &bytes)?;
        assert!(Launcher::try_from_path(path.path())?.is_some());
        bytes.extend_from_slice(b"PK\x06\x06");
        fs_err::write(path.path(), bytes)?;
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
}
