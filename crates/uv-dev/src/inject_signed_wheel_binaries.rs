use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use clap::Parser;
use futures::io::Cursor;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
pub(crate) struct InjectSignedWheelBinariesArgs {
    /// A wheel produced by uv's release build.
    #[arg(long)]
    input: PathBuf,
    /// The rewritten wheel. Existing paths are not overwritten.
    #[arg(long)]
    output: PathBuf,
    /// A directory containing code-signed executables named for their `.data/scripts` members.
    #[arg(long)]
    signed_binaries: PathBuf,
}

/// Reassemble `uv` and `uv_build` wheels with code-signed executables.
///
/// The release build produces file-only wheels with the relevant members arranged as:
///
/// ```text
/// uv-{version}.data/scripts/
/// ├── uv[.exe]
/// ├── uvx[.exe]
/// └── uvw.exe                 # Windows only
/// uv-{version}.dist-info/
/// └── RECORD
/// uv/
/// └── ...
///
/// uv_build-{version}.data/scripts/
/// └── uv-build[.exe]
/// uv_build-{version}.dist-info/
/// └── RECORD
/// uv_build/
/// └── ...
/// ```
///
/// Expects a trusted, file-only wheel and signed binaries that fit in memory. Every member under
/// `.data/scripts` is replaced by the same-named file from `signed_binaries`; unrelated files in
/// that directory are ignored. Other wheel contents and executable permissions are preserved, and
/// `RECORD` is regenerated from the output contents. The caller owns artifact provenance and
/// signature verification.
///
/// Input files are left unchanged. The output must not already exist and is only published once
/// the complete wheel has been written.
pub(crate) async fn inject_signed_wheel_binaries(
    args: InjectSignedWheelBinariesArgs,
) -> Result<()> {
    ensure!(
        args.input != args.output,
        "input and output wheels must be different"
    );
    let input = fs_err::read(&args.input)
        .with_context(|| format!("failed to read input wheel `{}`", args.input.display()))?;
    let mut archive = ZipFileReader::new(Cursor::new(input)).await?;
    let mut writer = ZipFileWriter::new(Vec::new());
    let mut record_path = None;
    let mut output_record = Vec::new();
    let mut replaced = false;

    for index in 0..archive.file().entries().len() {
        let entry = archive.file().entries()[index].clone();
        let name = entry.filename().as_str()?.to_string();
        if name.ends_with(".dist-info/RECORD") {
            ensure!(
                record_path.is_none(),
                "wheel contains multiple RECORD files"
            );
            record_path = Some(name);
            continue;
        }
        let builder = ZipEntryBuilder::new(name.clone().into(), entry.compression())
            .last_modification_date(*entry.last_modification_date())
            .internal_file_attribute(entry.internal_file_attribute())
            .external_file_attribute(entry.external_file_attribute())
            .comment(entry.comment().clone())
            .build();

        let bytes = if let Some((_, binary)) = name.split_once(".data/scripts/") {
            ensure!(
                !binary.is_empty() && !binary.contains('/') && !binary.contains('\\'),
                "unexpected executable wheel member `{name}`"
            );
            let path = args.signed_binaries.join(binary);
            replaced = true;
            fs_err::read(&path)
                .with_context(|| format!("failed to read signed binary `{}`", path.display()))?
        } else {
            let mut bytes = Vec::new();
            archive
                .reader_with_entry(index)
                .await?
                .read_to_end_checked(&mut bytes)
                .await?;
            bytes
        };
        writer.write_entry_whole(builder, &bytes).await?;
        output_record.push((
            name,
            BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes)),
            bytes.len() as u64,
        ));
    }
    ensure!(replaced, "wheel does not contain executable members");

    let record_path = record_path.context("wheel does not contain a RECORD file")?;
    let record_bytes = write_record(&record_path, output_record)?;
    let record_entry = ZipEntryBuilder::new(record_path.into(), Compression::Deflate)
        .unix_permissions(0o100_644)
        .last_modification_date(async_zip::ZipDateTime::default())
        .build();
    writer
        .write_entry_whole(record_entry, &record_bytes)
        .await?;
    let output = writer.close().await?;

    let output_directory = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs_err::create_dir_all(output_directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(output_directory)?;
    temporary
        .write_all(&output)
        .context("failed to write completed wheel")?;
    temporary.persist_noclobber(&args.output).with_context(|| {
        format!(
            "failed to create output wheel `{}` without overwriting",
            args.output.display()
        )
    })?;
    Ok(())
}

/// Serialize file names, encoded SHA-256 hashes, and sizes, then append the empty `RECORD` self-row.
fn write_record(record_path: &str, entries: Vec<(String, String, u64)>) -> Result<Vec<u8>> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for (path, hash, size) in entries {
        writer.write_record([path, format!("sha256={hash}"), size.to_string()])?;
    }
    writer.write_record([record_path, "", ""])?;
    writer.flush()?;
    writer.into_inner().context("failed to finish RECORD")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use async_zip::{ZipDateTimeBuilder, ZipEntry};

    use super::*;

    const BINARY: &str = "uv-1.2.3.data/scripts/uv";
    const RECORD: &str = "uv-1.2.3.dist-info/RECORD";
    const METADATA: &str = "uv-1.2.3.dist-info/METADATA";

    #[derive(Debug)]
    struct RecordEntry {
        hash: String,
        size: u64,
    }

    /// Parse output `RECORD` rows, asserting unique names and an empty self-row omitted from the map.
    fn read_record(bytes: &[u8], record_path: &str) -> Result<BTreeMap<String, RecordEntry>> {
        let mut entries = BTreeMap::new();
        let mut record_seen = false;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(bytes);
        for row in reader.deserialize::<(String, String, String)>() {
            let (path, hash, size) = row?;
            if path == record_path {
                assert!(!record_seen);
                record_seen = true;
                assert_eq!((hash.as_str(), size.as_str()), ("", ""));
            } else {
                let entry = RecordEntry {
                    hash,
                    size: size.parse()?,
                };
                assert!(entries.insert(path, entry).is_none());
            }
        }
        assert!(record_seen);
        Ok(entries)
    }

    /// Create an unsigned executable and metadata member with distinct Unix permissions.
    fn fixture_members() -> Vec<(ZipEntry, Vec<u8>)> {
        [
            (BINARY, b"unsigned".as_slice(), 0o100_755),
            (METADATA, b"metadata", 0o100_644),
        ]
        .into_iter()
        .map(|(name, bytes, mode)| {
            (
                ZipEntryBuilder::new(name.into(), Compression::Stored)
                    .unix_permissions(mode)
                    .build(),
                bytes.to_vec(),
            )
        })
        .collect()
    }

    /// Generate a fixture `RECORD` from member contents.
    fn fixture_record(members: &[(ZipEntry, Vec<u8>)]) -> Result<Vec<u8>> {
        write_record(
            RECORD,
            members
                .iter()
                .map(|(entry, bytes)| {
                    Ok((
                        entry.filename().as_str()?.to_string(),
                        BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(bytes)),
                        bytes.len() as u64,
                    ))
                })
                .collect::<Result<_>>()?,
        )
    }

    /// Write fixture members and a supplied or generated `RECORD`.
    async fn fixture(
        path: &Path,
        members: Vec<(ZipEntry, Vec<u8>)>,
        record: Option<Vec<u8>>,
    ) -> Result<()> {
        let record = record.map_or_else(|| fixture_record(&members), Ok)?;
        let mut writer = ZipFileWriter::new(Vec::new());
        writer.comment("discard this archive comment".to_string());
        for (entry, bytes) in members.into_iter().chain([(
            ZipEntryBuilder::new(RECORD.into(), Compression::Deflate)
                .unix_permissions(0o100_644)
                .build(),
            record,
        )]) {
            writer.write_entry_whole(entry, &bytes).await?;
        }
        fs_err::write(path, writer.close().await?)?;
        Ok(())
    }

    /// Use conventional fixture paths and a shared directory of signed binaries.
    fn fixture_args(directory: &Path) -> InjectSignedWheelBinariesArgs {
        InjectSignedWheelBinariesArgs {
            input: directory.join("input.whl"),
            output: directory.join("output.whl"),
            signed_binaries: directory.join("signed"),
        }
    }

    /// Write one signed binary under the name used by its wheel member.
    fn write_signed_binary(
        args: &InjectSignedWheelBinariesArgs,
        name: &str,
        contents: &[u8],
    ) -> Result<()> {
        fs_err::create_dir_all(&args.signed_binaries)?;
        fs_err::write(args.signed_binaries.join(name), contents)?;
        Ok(())
    }

    /// Every executable member must be flat and have a same-named signed binary.
    #[tokio::test]
    async fn requires_expected_signed_binaries() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = fixture_args(directory.path());
        let output = args.output.clone();
        fixture(&args.input, fixture_members(), None).await?;
        let error = inject_signed_wheel_binaries(args)
            .await
            .expect_err("missing signed binary");
        assert!(
            error
                .to_string()
                .starts_with("failed to read signed binary")
        );
        assert!(!output.exists());

        let directory = tempfile::tempdir()?;
        let args = fixture_args(directory.path());
        write_signed_binary(&args, "uv", b"signed")?;
        let mut members = fixture_members();
        members[0].0 = ZipEntryBuilder::new(
            "uv-1.2.3.data/scripts/nested/uv".into(),
            Compression::Stored,
        )
        .unix_permissions(0o100_755)
        .build();
        fixture(&args.input, members, None).await?;
        let error = inject_signed_wheel_binaries(args)
            .await
            .expect_err("nested executable member");
        assert_eq!(
            error.to_string(),
            "unexpected executable wheel member `uv-1.2.3.data/scripts/nested/uv`"
        );

        let directory = tempfile::tempdir()?;
        let args = fixture_args(directory.path());
        let members = fixture_members()
            .into_iter()
            .filter(|(entry, _)| entry.filename().as_bytes() != BINARY.as_bytes())
            .collect();
        fixture(&args.input, members, None).await?;
        let error = inject_signed_wheel_binaries(args)
            .await
            .expect_err("wheel without executable members");
        assert_eq!(
            error.to_string(),
            "wheel does not contain executable members"
        );
        Ok(())
    }

    /// A missing or ambiguous `RECORD` prevents publication.
    #[tokio::test]
    async fn requires_one_record() -> Result<()> {
        for (records, expected) in [
            (vec![], "wheel does not contain a RECORD file"),
            (
                vec![RECORD, "uv_build-1.2.3.dist-info/RECORD"],
                "wheel contains multiple RECORD files",
            ),
        ] {
            let directory = tempfile::tempdir()?;
            let args = fixture_args(directory.path());
            write_signed_binary(&args, "uv", b"signed")?;
            let mut writer = ZipFileWriter::new(Vec::new());
            for (entry, bytes) in fixture_members() {
                writer.write_entry_whole(entry, &bytes).await?;
            }
            for name in records {
                writer
                    .write_entry_whole(
                        ZipEntryBuilder::new(name.into(), Compression::Stored).build(),
                        b"",
                    )
                    .await?;
            }
            fs_err::write(&args.input, writer.close().await?)?;
            let error = inject_signed_wheel_binaries(args)
                .await
                .expect_err("invalid RECORD count");
            assert_eq!(error.to_string(), expected);
            assert!(!directory.path().join("output.whl").exists());
        }
        Ok(())
    }

    /// A fresh `RECORD` describes the bytes written, regardless of the old CSV or hashes.
    #[tokio::test]
    async fn regenerates_record_without_reading_the_original() -> Result<()> {
        for record in [b"".as_slice(), b"stale,sha256=wrong,123", b"\xff"] {
            let directory = tempfile::tempdir()?;
            let args = fixture_args(directory.path());
            write_signed_binary(&args, "uv", b"signed")?;
            fixture(&args.input, fixture_members(), Some(record.to_vec())).await?;
            inject_signed_wheel_binaries(args).await?;
            let output = directory.path().join("output.whl");
            assert_eq!(read_entry(&output, BINARY).await?.0, b"signed");
            assert_eq!(read_entry(&output, METADATA).await?.0, b"metadata");
            let (record, _, _) = read_entry(&output, RECORD).await?;
            let record = read_record(&record, RECORD)?;
            assert_eq!(record.len(), 2);
            for (name, contents) in [(BINARY, b"signed".as_slice()), (METADATA, b"metadata")] {
                assert_eq!(record[name].size, contents.len() as u64);
                assert_eq!(
                    record[name].hash,
                    format!(
                        "sha256={}",
                        BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(contents))
                    )
                );
            }
        }
        Ok(())
    }

    /// Existing outputs stay untouched, even when repeating an identical successful transformation.
    #[tokio::test]
    async fn never_overwrites_existing_output() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = fixture_args(directory.path());
        write_signed_binary(&args, "uv", b"signed")?;
        fixture(&args.input, fixture_members(), None).await?;
        fs_err::write(&args.output, b"existing output")?;
        assert!(inject_signed_wheel_binaries(args).await.is_err());
        assert_eq!(
            fs_err::read(directory.path().join("output.whl"))?,
            b"existing output"
        );
        assert_eq!(fs_err::read_dir(directory.path())?.count(), 3);
        // The contract also rejects a repeat of a successful identical transformation.
        let mut first = self::fixture_args(directory.path());
        first.output = directory.path().join("first.whl");
        inject_signed_wheel_binaries(first).await?;
        let before = fs_err::read(directory.path().join("first.whl"))?;
        let mut repeated = self::fixture_args(directory.path());
        repeated.output = directory.path().join("first.whl");
        assert!(inject_signed_wheel_binaries(repeated).await.is_err());
        assert_eq!(fs_err::read(directory.path().join("first.whl"))?, before);
        Ok(())
    }

    /// Output symlinks and their targets stay untouched, including dangling symlinks.
    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_output_symlinks() -> Result<()> {
        for dangling in [false, true] {
            let directory = tempfile::tempdir()?;
            let args = fixture_args(directory.path());
            write_signed_binary(&args, "uv", b"signed")?;
            fixture(&args.input, fixture_members(), None).await?;
            let target = directory.path().join("target");
            if !dangling {
                fs_err::write(&target, b"sentinel")?;
            }
            fs_err::os::unix::fs::symlink(&target, &args.output)?;
            assert!(inject_signed_wheel_binaries(args).await.is_err());
            assert_eq!(
                fs_err::read_link(directory.path().join("output.whl"))?,
                target
            );
            if dangling {
                assert!(!target.exists());
            } else {
                assert_eq!(fs_err::read(target)?, b"sentinel");
            }
        }
        Ok(())
    }

    /// CRC32 is checked for copied contents, but not for discarded executable data.
    #[tokio::test]
    async fn checks_crc32_only_for_copied_members() -> Result<()> {
        for member_index in [0, 1] {
            let directory = tempfile::tempdir()?;
            let args = fixture_args(directory.path());
            write_signed_binary(&args, "uv", b"signed")?;
            fixture(&args.input, fixture_members(), None).await?;
            let mut bytes = fs_err::read(&args.input)?;
            let position = bytes
                .windows(4)
                .enumerate()
                .filter(|(_, bytes)| *bytes == b"PK\x01\x02")
                .nth(member_index)
                .map(|(position, _)| position)
                .context("missing ZIP header")?;
            bytes[position + 16..position + 20].copy_from_slice(&0_u32.to_le_bytes());
            fs_err::write(&args.input, bytes)?;
            if member_index == 0 {
                inject_signed_wheel_binaries(args).await?;
                let output = directory.path().join("output.whl");
                assert_eq!(read_entry(&output, BINARY).await?.0, b"signed");
            } else {
                assert!(inject_signed_wheel_binaries(args).await.is_err());
                assert_eq!(fs_err::read_dir(directory.path())?.count(), 2);
            }
        }
        Ok(())
    }

    /// Untouched members retain their bytes and metadata with stored or deflated compression.
    #[tokio::test]
    async fn preserves_declared_metadata_and_bytes() -> Result<()> {
        for compression in [Compression::Stored, Compression::Deflate] {
            let directory = tempfile::tempdir()?;
            let args = fixture_args(directory.path());
            write_signed_binary(&args, "uv", b"signed")?;
            let stamp = ZipDateTimeBuilder::new()
                .year(2001)
                .month(2)
                .day(3)
                .hour(4)
                .minute(5)
                .second(6)
                .build();
            let mut members = fixture_members();
            let extra = ZipEntryBuilder::new("uv/_find_uv.py".into(), compression)
                .unix_permissions(0o100_640)
                .last_modification_date(stamp)
                .internal_file_attribute(1)
                .comment("entry comment".into())
                .build();
            members.push((extra, b"untouched".to_vec()));
            fixture(&args.input, members, None).await?;
            inject_signed_wheel_binaries(args).await?;
            let output = directory.path().join("output.whl");
            let (bytes, method, mode) = read_entry(&output, "uv/_find_uv.py").await?;
            assert_eq!(
                (bytes, method, mode),
                (b"untouched".to_vec(), compression, Some(0o100_640))
            );
            let archive = ZipFileReader::new(Cursor::new(fs_err::read(&output)?)).await?;
            let entry = archive
                .file()
                .entries()
                .iter()
                .find(|entry| entry.filename().as_bytes() == b"uv/_find_uv.py")
                .context("missing metadata fixture")?;
            assert_eq!(entry.last_modification_date(), &stamp);
            assert_eq!(entry.internal_file_attribute(), 1);
            assert_eq!(entry.comment().as_bytes(), b"entry comment");
            assert!(archive.file().comment().as_bytes().is_empty());
            let (record, method, mode) = read_entry(&output, RECORD).await?;
            assert_eq!(method, Compression::Deflate);
            assert_eq!(mode, Some(0o100_644));
            let entries = read_record(&record, RECORD)?;
            assert_eq!(entries["uv/_find_uv.py"].size, 9);
        }
        Ok(())
    }

    /// A shared signing directory contributes only the binaries required by this wheel.
    #[tokio::test]
    async fn uses_only_signed_binaries_required_by_wheel() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = fixture_args(directory.path());
        let mut members = fixture_members();
        let second_binary = "uv-1.2.3.data/scripts/uvx";
        members.push((
            ZipEntryBuilder::new(second_binary.into(), Compression::Deflate)
                .unix_permissions(0o100_755)
                .build(),
            b"unsigned uvx".to_vec(),
        ));
        fixture(&args.input, members.clone(), None).await?;
        write_signed_binary(&args, "uv", b"signed uv")?;
        write_signed_binary(&args, "uvx", b"signed uvx")?;
        write_signed_binary(&args, "uv-build", b"signed uv-build")?;
        write_signed_binary(&args, "certificate.pem", b"signing certificate")?;
        // Derive expected bytes from the fixture and signed inputs, not from the output archive.
        for (entry, bytes) in &mut members {
            match entry.filename().as_str()? {
                BINARY => *bytes = b"signed uv".to_vec(),
                name if name == second_binary => *bytes = b"signed uvx".to_vec(),
                _ => {}
            }
        }
        let record = fixture_record(&members)?;
        members.push((
            ZipEntryBuilder::new(RECORD.into(), Compression::Deflate)
                .unix_permissions(0o100_644)
                .build(),
            record,
        ));
        let expected = members
            .into_iter()
            .map(|(entry, bytes)| {
                Ok((
                    entry.filename().as_str()?.to_string(),
                    (
                        entry,
                        BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes)),
                        bytes.len() as u64,
                    ),
                ))
            })
            .collect::<Result<_>>()?;
        inject_signed_wheel_binaries(args).await?;
        verify_output(&directory.path().join("output.whl"), &expected).await?;
        Ok(())
    }

    /// Check output membership, metadata, hashes, sizes, and `RECORD` against fixture expectations.
    async fn verify_output(
        path: &Path,
        expected: &BTreeMap<String, (ZipEntry, String, u64)>,
    ) -> Result<()> {
        let mut archive = ZipFileReader::new(Cursor::new(fs_err::read(path)?)).await?;
        ensure!(
            archive.file().entries().len() == expected.len(),
            "output membership changed"
        );
        ensure!(
            archive.file().comment().as_bytes().is_empty(),
            "unexpected output archive comment"
        );
        let (record_bytes, _, _) = read_entry(path, RECORD).await?;
        let mut record = read_record(&record_bytes, RECORD)?;
        for index in 0..archive.file().entries().len() {
            let entry = archive.file().entries()[index].clone();
            let name = entry.filename().as_str()?;
            let (metadata, expected_hash, expected_size) =
                expected.get(name).context("unexpected output member")?;
            ensure!(
                entry.compression() == metadata.compression()
                    && entry.last_modification_date() == metadata.last_modification_date()
                    && entry.internal_file_attribute() == metadata.internal_file_attribute()
                    && entry.external_file_attribute() == metadata.external_file_attribute()
                    && entry.comment().as_bytes() == metadata.comment().as_bytes()
                    && entry.attribute_compatibility() == metadata.attribute_compatibility(),
                "output metadata changed for `{name}`"
            );
            let mut reader = archive.reader_with_entry(index).await?;
            let mut bytes = Vec::new();
            reader.read_to_end_checked(&mut bytes).await?;
            let size = bytes.len() as u64;
            let hash = BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes));
            ensure!(
                &hash == expected_hash && size == *expected_size,
                "output bytes changed for `{name}`"
            );
            if name != RECORD {
                let expected = record
                    .remove(name)
                    .context("output RECORD is missing a member")?;
                assert_eq!(expected.hash, format!("sha256={hash}"));
                assert_eq!(expected.size, size);
            }
        }
        ensure!(record.is_empty(), "output RECORD contains extra members");
        Ok(())
    }

    /// Write a basic wheel fixture with an executable and metadata.
    async fn write_wheel(path: &Path) -> Result<()> {
        fixture(
            path,
            vec![
                (
                    ZipEntryBuilder::new(BINARY.into(), Compression::Deflate)
                        .unix_permissions(0o100_755)
                        .build(),
                    b"unsigned executable".to_vec(),
                ),
                (
                    ZipEntryBuilder::new(METADATA.into(), Compression::Stored)
                        .unix_permissions(0o100_644)
                        .build(),
                    b"Metadata-Version: 2.4\nName: uv\nVersion: 1.2.3\n".to_vec(),
                ),
            ],
            None,
        )
        .await
    }

    /// Read an exact member name with CRC32 checking, returning its bytes, compression, and mode.
    async fn read_entry(path: &Path, name: &str) -> Result<(Vec<u8>, Compression, Option<u16>)> {
        let bytes = fs_err::read(path)?;
        let mut archive = ZipFileReader::new(Cursor::new(bytes)).await?;
        let (index, compression, permissions) = archive
            .file()
            .entries()
            .iter()
            .enumerate()
            .find_map(|(index, entry)| {
                (entry.filename().as_str().ok()? == name).then_some((
                    index,
                    entry.compression(),
                    entry.unix_permissions(),
                ))
            })
            .context("missing output entry")?;
        let mut bytes = Vec::new();
        archive
            .reader_with_entry(index)
            .await?
            .read_to_end_checked(&mut bytes)
            .await?;
        Ok((bytes, compression, permissions))
    }

    /// Replacing an executable preserves modes and untouched metadata while regenerating `RECORD`.
    #[tokio::test]
    async fn replaces_executable_and_regenerates_record() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let input = temporary.path().join("unsigned.whl");
        let output = temporary.path().join("signed.whl");
        let signed_binaries = temporary.path().join("signed");
        write_wheel(&input).await?;
        fs_err::create_dir(&signed_binaries)?;
        fs_err::write(signed_binaries.join("uv"), b"signed executable")?;

        inject_signed_wheel_binaries(InjectSignedWheelBinariesArgs {
            input,
            output: output.clone(),
            signed_binaries,
        })
        .await?;

        let (executable, compression, permissions) =
            read_entry(&output, "uv-1.2.3.data/scripts/uv").await?;
        assert_eq!(executable, b"signed executable");
        assert_eq!(compression, Compression::Deflate);
        assert_eq!(permissions, Some(0o100_755));
        let (metadata, compression, permissions) =
            read_entry(&output, "uv-1.2.3.dist-info/METADATA").await?;
        assert_eq!(
            metadata,
            b"Metadata-Version: 2.4\nName: uv\nVersion: 1.2.3\n"
        );
        assert_eq!(compression, Compression::Stored);
        assert_eq!(permissions, Some(0o100_644));

        let (record, _, _) = read_entry(&output, "uv-1.2.3.dist-info/RECORD").await?;
        insta::assert_snapshot!(String::from_utf8(record)?, @r###"
        uv-1.2.3.data/scripts/uv,sha256=5_eL4Xt8puSyE212q8t1kpEHv52KHMCFuqi72XFsTAI,17
        uv-1.2.3.dist-info/METADATA,sha256=xIJR2rCm0gl4ZA_dDnXvZFPY7qxwTnekSnzuMDye80k,46
        uv-1.2.3.dist-info/RECORD,,
        "###);
        Ok(())
    }
}
