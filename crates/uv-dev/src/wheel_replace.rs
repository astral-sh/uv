use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder, ZipFile};
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use clap::Parser;
use futures::io::{AllowStdIo, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use sha2::{Digest, Sha256};

const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Debug, Parser)]
pub(crate) struct WheelReplaceArgs {
    /// The input wheel.
    #[arg(long)]
    input: PathBuf,
    /// The rewritten wheel. Must not already exist (including a dangling symlink).
    #[arg(long)]
    output: PathBuf,
    /// A wheel member and its replacement file, in the form `MEMBER=PATH`.
    #[arg(long = "replace", required = true)]
    replacements: Vec<Replacement>,
}

#[derive(Clone, Debug)]
struct Replacement {
    member: String,
    path: PathBuf,
}

impl FromStr for Replacement {
    type Err = anyhow::Error;

    /// Parse `MEMBER=PATH`, keeping any later `=` characters in the filesystem path.
    fn from_str(value: &str) -> Result<Self> {
        let Some((member, path)) = value.split_once('=') else {
            bail!("expected `MEMBER=PATH`, got `{value}`");
        };
        ensure!(!member.is_empty(), "replacement member cannot be empty");
        ensure!(!path.is_empty(), "replacement path cannot be empty");
        Ok(Self {
            member: member.to_string(),
            path: PathBuf::from(path),
        })
    }
}

/// Rewrite a trusted wheel without extracting its members to the filesystem.
///
/// Preserve exact decompressed bytes except for explicitly replaced members and `RECORD`. Preserve
/// each non-`RECORD` member's compression method, DOS timestamp, internal/external attributes and
/// entry comment. Recompression does not preserve compressed streams, local headers, compression
/// levels, arbitrary extra fields or the archive comment. The ZIP library emits Unix creator
/// metadata, including for DOS inputs. `RECORD` is emitted last with SHA-256 hashes, Deflate, mode
/// 0644, the ZIP epoch timestamp and no comment. Structural ZIP64 fields are generated as needed.
///
/// Flush the completed temporary archive before atomically creating the output with no clobber.
/// Existing output is never reused, even if it has identical bytes. Failure removes the temporary
/// file and leaves existing paths alone.
/// This is process-level atomic publication, not a promise of power-loss durability. Callers own
/// provenance, digest verification and immutable input/replacement staging. The input `RECORD`
/// and replaced member contents are discarded without reading or validating them.
/// Member names are copied verbatim, not interpreted as filesystem paths.
pub(crate) async fn wheel_replace(args: WheelReplaceArgs) -> Result<()> {
    ensure!(
        args.input != args.output,
        "input and output wheels must be different"
    );
    let mut replacements = BTreeMap::new();
    for replacement in args.replacements {
        let member = replacement.member;
        ensure!(
            replacements
                .insert(member.clone(), replacement.path)
                .is_none(),
            "duplicate replacement for `{member}`"
        );
    }

    let input = fs_err::File::open(&args.input)
        .with_context(|| format!("failed to open input wheel `{}`", args.input.display()))?;
    let mut archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(input))).await?;
    let record_path = validate_archive(archive.file())?;

    let output_directory = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs_err::create_dir_all(output_directory)?;
    let temporary = tempfile::NamedTempFile::new_in(output_directory)?;
    let output = temporary.reopen()?;
    let mut writer = ZipFileWriter::new(AllowStdIo::new(BufWriter::new(output)));
    let mut output_record = Vec::new();

    for index in 0..archive.file().entries().len() {
        let entry = archive.file().entries()[index].clone();
        let name = entry.filename().as_str()?.to_string();
        if name == record_path {
            continue;
        }
        let builder = ZipEntryBuilder::new(name.clone().into(), entry.compression())
            .last_modification_date(*entry.last_modification_date())
            .internal_file_attribute(entry.internal_file_attribute())
            .external_file_attribute(entry.external_file_attribute())
            .comment(entry.comment().clone())
            .build();

        if entry.dir()? {
            ensure!(
                !replacements.contains_key(&name),
                "cannot replace directory member `{name}`"
            );
            let mut original = archive.reader_with_entry(index).await?;
            let size = futures::io::copy(&mut original, &mut futures::io::sink()).await?;
            ensure!(size == 0, "directory member `{name}` contains data");
            validate_zip_contents(&mut original, size)?;
            writer.write_entry_whole(builder, &[]).await?;
            continue;
        }
        let mut output_entry = writer.write_entry_seekable(builder).await?;
        let (hash, size) = if let Some(path) = replacements.remove(&name) {
            let replacement = fs_err::File::open(&path)
                .with_context(|| format!("failed to open replacement `{}`", path.display()))?;
            ensure!(
                replacement.metadata()?.is_file(),
                "replacement `{}` is not a regular file",
                path.display()
            );
            let mut replacement = AllowStdIo::new(BufReader::new(replacement));
            copy_hashed(&mut replacement, &mut output_entry).await?
        } else {
            let mut original = archive.reader_with_entry(index).await?;
            let (hash, size) = copy_hashed(&mut original, &mut output_entry).await?;
            validate_zip_contents(&mut original, size)?;
            (hash, size)
        };
        output_entry.close().await?;
        output_record.push((name, hash, size));
    }
    ensure!(
        replacements.is_empty(),
        "replacement members not present in the wheel: {}",
        replacements.keys().cloned().collect::<Vec<_>>().join(", ")
    );

    let record_bytes = write_record(&record_path, output_record)?;
    let record_entry = ZipEntryBuilder::new(record_path.into(), Compression::Deflate)
        .unix_permissions(0o100_644)
        .last_modification_date(async_zip::ZipDateTime::default())
        .build();
    writer
        .write_entry_whole(record_entry, &record_bytes)
        .await?;
    finish_archive(writer).await?;
    temporary.persist_noclobber(&args.output).with_context(|| {
        format!(
            "failed to create output wheel `{}` without overwriting",
            args.output.display()
        )
    })?;
    Ok(())
}

/// Write the central directory and flush all buffered output before publishing the archive.
///
/// [`ZipFileWriter::close`] alone does not flush the underlying writer.
async fn finish_archive<W: AsyncWrite + Unpin>(writer: ZipFileWriter<W>) -> Result<()> {
    let mut output = writer.close().await?;
    output
        .flush()
        .await
        .context("failed to flush completed wheel")?;
    Ok(())
}

/// Reject duplicate names, unsupported member types, and signatures; return the unique `RECORD` path.
///
/// This inspects ZIP metadata only; it does not read member contents or interpret names as paths.
fn validate_archive(file: &ZipFile) -> Result<String> {
    let mut names = BTreeSet::new();
    let mut record = None;
    for entry in file.entries() {
        let name = entry
            .filename()
            .as_str()
            .context("wheel member name is not valid UTF-8")?;
        validate_member_type(name, entry.unix_permissions(), entry.dir()?)?;
        ensure!(names.insert(name), "duplicate wheel member `{name}`");
        ensure!(
            !entry.dir()? || entry.uncompressed_size() == 0,
            "directory member `{name}` contains data"
        );
        ensure!(
            !name.ends_with(".dist-info/RECORD.jws") && !name.ends_with(".dist-info/RECORD.p7s"),
            "wheel contains unsupported RECORD signature `{name}`"
        );
        if name.ends_with(".dist-info/RECORD") {
            ensure!(record.is_none(), "wheel contains multiple RECORD files");
            record = Some(name.to_string());
        }
    }
    record.context("wheel does not contain a RECORD file")
}

/// Compare a fully consumed member's byte counts and CRC32 with its ZIP metadata.
fn validate_zip_contents<R: futures::io::AsyncBufRead + Unpin>(
    reader: &mut async_zip::base::read::ZipEntryReader<'_, R, async_zip::base::read::WithEntry<'_>>,
    size: u64,
) -> Result<()> {
    ensure!(
        size == reader.entry().uncompressed_size(),
        "ZIP member size does not match its contents"
    );
    ensure!(
        reader.bytes_read() == reader.entry().compressed_size(),
        "ZIP member compressed size does not match its contents"
    );
    ensure!(
        reader.compute_hash() == reader.entry().crc32(),
        "ZIP member CRC32 does not match its contents"
    );
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

/// Accept regular files and directories with matching Unix types, or an unspecified type.
fn validate_member_type(name: &str, permissions: Option<u16>, directory: bool) -> Result<()> {
    let Some(permissions) = permissions else {
        return Ok(());
    };
    let file_type = permissions & 0o170_000;
    let expected_type = if directory { 0o040_000 } else { 0o100_000 };
    ensure!(
        file_type == 0 || file_type == expected_type,
        "wheel member `{name}` is not a regular file or directory"
    );
    Ok(())
}

/// Copy bytes through a fixed-size buffer and return their unpadded URL-safe Base64 SHA-256 and size.
async fn copy_hashed(
    reader: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> Result<(String, u64)> {
    let mut sha256 = Sha256::new();
    let mut size: u64 = 0;
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .context("wheel member size overflowed")?;
        writer.write_all(&buffer[..read]).await?;
        sha256.update(&buffer[..read]);
    }
    Ok((BASE64_URL_SAFE_NO_PAD.encode(sha256.finalize()), size))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Write};

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

    /// Generate a fixture `RECORD` from file contents, excluding directory entries.
    fn fixture_record(members: &[(ZipEntry, Vec<u8>)]) -> Result<Vec<u8>> {
        write_record(
            RECORD,
            members
                .iter()
                .filter(|(entry, _)| !entry.filename().as_bytes().ends_with(b"/"))
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

    /// Write members and a supplied or generated `RECORD`, with optional streaming headers or ZIP64.
    async fn fixture(
        path: &Path,
        members: Vec<(ZipEntry, Vec<u8>)>,
        record: Option<Vec<u8>>,
        streaming: bool,
        zip64: bool,
    ) -> Result<()> {
        let record = record.map_or_else(|| fixture_record(&members), Ok)?;
        let mut writer =
            ZipFileWriter::new(AllowStdIo::new(BufWriter::new(fs_err::File::create(path)?)));
        if zip64 {
            writer = writer.force_zip64();
        }
        writer.comment("discard this archive comment".to_string());
        for (entry, bytes) in members.into_iter().chain([(
            ZipEntryBuilder::new(RECORD.into(), Compression::Deflate)
                .unix_permissions(0o100_644)
                .build(),
            record,
        )]) {
            if streaming {
                let mut stream = writer.write_entry_stream(entry).await?;
                stream.write_all(&bytes).await?;
                stream.close().await?;
            } else {
                writer.write_entry_whole(entry, &bytes).await?;
            }
        }
        finish_archive(writer).await
    }

    /// Use conventional fixture paths and replace the executable with the `signed` file.
    fn args(directory: &Path) -> WheelReplaceArgs {
        WheelReplaceArgs {
            input: directory.join("input.whl"),
            output: directory.join("output.whl"),
            replacements: vec![Replacement {
                member: BINARY.to_string(),
                path: directory.join("signed"),
            }],
        }
    }

    /// ZIP names retain Unicode and case distinctions without filesystem portability restrictions.
    #[tokio::test]
    async fn preserves_member_names() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        let mut members = fixture_members();
        let names = [
            "package/café.txt",
            "package/CON.txt",
            "package/Case.txt",
            "package/case.txt",
            "Package/another.txt",
            "package/tilde~.txt",
        ];
        for name in names {
            members.push((
                ZipEntryBuilder::new(name.into(), Compression::Stored).build(),
                b"untouched".to_vec(),
            ));
        }
        // Replacement uses the exact ZIP name, including Unicode and case distinctions.
        args.replacements.push(Replacement {
            member: names[0].to_string(),
            path: args.replacements[0].path.clone(),
        });
        fixture(&args.input, members, None, false, false).await?;
        wheel_replace(args).await?;
        let output = directory.path().join("output.whl");
        let (record, _, _) = read_entry(&output, RECORD).await?;
        let record = read_record(&record, RECORD)?;
        for name in names {
            let expected: &[u8] = if name == names[0] {
                b"signed"
            } else {
                b"untouched"
            };
            assert_eq!(read_entry(&output, name).await?.0, expected);
            assert!(record.contains_key(name));
        }
        Ok(())
    }

    /// Duplicate names, special files, nonempty directories, and signatures leave no output behind.
    #[tokio::test]
    async fn rejects_bad_members_and_cleans_up() -> Result<()> {
        for (name, mode, contents) in [
            (BINARY, 0o100_644, b"duplicate".as_slice()),
            ("link", 0o120_777, b"target"),
            ("fifo", 0o010_644, b""),
            ("socket", 0o140_644, b""),
            ("dir/", 0o040_755, b"not empty"),
            ("uv-1.2.3.dist-info/RECORD.jws", 0o100_644, b"signature"),
            ("uv-1.2.3.dist-info/RECORD.p7s", 0o100_644, b"signature"),
        ] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            let mut members = fixture_members();
            members.push((
                ZipEntryBuilder::new(name.into(), Compression::Stored)
                    .unix_permissions(mode)
                    .build(),
                contents.to_vec(),
            ));
            fixture(&args.input, members, None, false, false).await?;
            assert!(wheel_replace(args).await.is_err(), "accepted {name}");
            assert!(!directory.path().join("output.whl").exists());
            assert_eq!(
                fs_err::read_dir(directory.path())?.count(),
                2,
                "temporary file leaked"
            );
        }
        Ok(())
    }

    /// Each replacement must select one existing non-`RECORD` member exactly once.
    #[tokio::test]
    async fn rejects_unmatched_and_duplicate_replacements() -> Result<()> {
        for (names, expected) in [
            (
                vec!["missing"],
                "replacement members not present in the wheel: missing",
            ),
            (
                vec![BINARY, BINARY],
                "duplicate replacement for `uv-1.2.3.data/scripts/uv`",
            ),
            (
                vec![RECORD],
                "replacement members not present in the wheel: uv-1.2.3.dist-info/RECORD",
            ),
        ] {
            let directory = tempfile::tempdir()?;
            let mut args = args(directory.path());
            let replacement = args.replacements[0].path.clone();
            fs_err::write(&replacement, b"signed")?;
            fixture(&args.input, fixture_members(), None, false, false).await?;
            args.replacements = names
                .into_iter()
                .map(|name| Replacement {
                    member: name.to_string(),
                    path: replacement.clone(),
                })
                .collect();
            let error = wheel_replace(args).await.expect_err("invalid replacement");
            assert_eq!(error.to_string(), expected);
            assert_eq!(fs_err::read_dir(directory.path())?.count(), 2);
        }
        Ok(())
    }

    /// A fresh `RECORD` describes the bytes written, regardless of the old CSV or hashes.
    #[tokio::test]
    async fn regenerates_record_without_reading_the_original() -> Result<()> {
        for record in [b"".as_slice(), b"stale,sha256=wrong,123", b"\xff"] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            fixture(
                &args.input,
                fixture_members(),
                Some(record.to_vec()),
                false,
                false,
            )
            .await?;
            wheel_replace(args).await?;
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
        let args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        fixture(&args.input, fixture_members(), None, false, false).await?;
        fs_err::write(&args.output, b"existing output")?;
        assert!(wheel_replace(args).await.is_err());
        assert_eq!(
            fs_err::read(directory.path().join("output.whl"))?,
            b"existing output"
        );
        assert_eq!(fs_err::read_dir(directory.path())?.count(), 3);
        // The contract also rejects a repeat of a successful identical transformation.
        let mut first = self::args(directory.path());
        first.output = directory.path().join("first.whl");
        wheel_replace(first).await?;
        let before = fs_err::read(directory.path().join("first.whl"))?;
        let mut repeated = self::args(directory.path());
        repeated.output = directory.path().join("first.whl");
        assert!(wheel_replace(repeated).await.is_err());
        assert_eq!(fs_err::read(directory.path().join("first.whl"))?, before);
        Ok(())
    }

    /// Output symlinks and their targets stay untouched, including dangling symlinks.
    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_output_symlinks() -> Result<()> {
        for dangling in [false, true] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            fixture(&args.input, fixture_members(), None, false, false).await?;
            let target = directory.path().join("target");
            if !dangling {
                fs_err::write(&target, b"sentinel")?;
            }
            fs_err::os::unix::fs::symlink(&target, &args.output)?;
            assert!(wheel_replace(args).await.is_err());
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

    struct FailingWriter {
        fail_write: bool,
    }

    impl Write for FailingWriter {
        /// Inject a write failure when requested; otherwise discard the bytes successfully.
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                Err(io::Error::other("injected buffered write failure"))
            } else {
                Ok(bytes.len())
            }
        }
        /// Always fail so the test can distinguish final flushing from ZIP closure.
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }

    /// Buffered write and flush failures must propagate before the temporary file is published.
    #[tokio::test]
    async fn propagates_final_buffered_failures_before_persistence() -> Result<()> {
        for fail_write in [true, false] {
            let directory = tempfile::tempdir()?;
            let output = directory.path().join("output.whl");
            let result: Result<()> = async {
                let temporary = tempfile::NamedTempFile::new_in(directory.path())?;
                // The empty ZIP's end record fits entirely in this buffer. ZIP close succeeds;
                // only the explicit final flush reaches the failing underlying writer.
                let writer = ZipFileWriter::new(AllowStdIo::new(BufWriter::with_capacity(
                    1024,
                    FailingWriter { fail_write },
                )));
                finish_archive(writer).await?;
                temporary.persist_noclobber(&output)?;
                Ok(())
            }
            .await;
            assert_eq!(
                result
                    .expect_err("buffered error must propagate")
                    .to_string(),
                "failed to flush completed wheel"
            );
            assert!(!output.exists());
            assert_eq!(fs_err::read_dir(directory.path())?.count(), 0);
        }
        Ok(())
    }

    /// Trusted build artifacts are not rejected merely for containing many members.
    #[tokio::test]
    async fn replaces_in_a_wheel_with_many_members() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        let mut members = fixture_members();
        for index in 0..10_000 {
            members.push((
                ZipEntryBuilder::new(format!("package/{index}.txt").into(), Compression::Stored)
                    .build(),
                b"untouched".to_vec(),
            ));
        }
        fixture(&args.input, members, None, false, false).await?;
        wheel_replace(args).await?;
        let output = directory.path().join("output.whl");
        assert_eq!(read_entry(&output, BINARY).await?.0, b"signed");
        assert_eq!(
            read_entry(&output, "package/9999.txt").await?.0,
            b"untouched"
        );
        let (record, _, _) = read_entry(&output, RECORD).await?;
        assert_eq!(read_record(&record, RECORD)?.len(), 10_002);
        Ok(())
    }

    /// Copying and hashing include the final partial buffer.
    #[tokio::test]
    async fn streams_across_buffer_boundaries() -> Result<()> {
        let bytes = vec![42; BUFFER_SIZE + 1];
        let mut input = futures::io::Cursor::new(&bytes);
        let mut output = Vec::new();
        let (hash, size) = copy_hashed(&mut input, &mut output).await?;
        assert_eq!(output, bytes);
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(hash, BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes)));
        Ok(())
    }

    /// Invalid copied CRC32 or size metadata is rejected, but discarded replacement data is ignored.
    #[tokio::test]
    async fn validates_only_copied_zip_contents() -> Result<()> {
        for (member_index, field, replacement) in [
            (0, 16, 0_u32), // Replaced member: the old CRC32 is irrelevant.
            (1, 16, 0_u32), // Copied member: CRC32 must match.
            (1, 24, 9_u32), // Copied member: uncompressed size must match.
        ] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            fixture(&args.input, fixture_members(), None, false, false).await?;
            let mut bytes = fs_err::read(&args.input)?;
            let position = bytes
                .windows(4)
                .enumerate()
                .filter(|(_, bytes)| *bytes == b"PK\x01\x02")
                .nth(member_index)
                .map(|(position, _)| position)
                .context("missing ZIP header")?;
            bytes[position + field..position + field + 4]
                .copy_from_slice(&replacement.to_le_bytes());
            fs_err::write(&args.input, bytes)?;
            if member_index == 0 {
                wheel_replace(args).await?;
                let output = directory.path().join("output.whl");
                assert_eq!(read_entry(&output, BINARY).await?.0, b"signed");
            } else {
                assert!(wheel_replace(args).await.is_err());
                assert_eq!(fs_err::read_dir(directory.path())?.count(), 2);
            }
        }
        Ok(())
    }

    /// Supported compression, streaming headers, and ZIP64 preserve member bytes and metadata.
    #[tokio::test]
    async fn preserves_declared_metadata_and_bytes() -> Result<()> {
        for (compression, streaming, zip64) in [
            (Compression::Stored, false, false),
            (Compression::Deflate, true, false),
            (Compression::Zstd, false, true),
        ] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            let stamp = ZipDateTimeBuilder::new()
                .year(2001)
                .month(2)
                .day(3)
                .hour(4)
                .minute(5)
                .second(6)
                .build();
            let mut members = fixture_members();
            let extra = ZipEntryBuilder::new("package/comma,name.py".into(), compression)
                .unix_permissions(0o100_640)
                .last_modification_date(stamp)
                .internal_file_attribute(1)
                .comment("entry comment".into())
                .build();
            members.push((extra, b"untouched".to_vec()));
            members.push((
                ZipEntryBuilder::new("package/".into(), Compression::Stored)
                    .unix_permissions(0o040_755)
                    .build(),
                Vec::new(),
            ));
            fixture(&args.input, members, None, streaming, zip64).await?;
            wheel_replace(args).await?;
            let output = directory.path().join("output.whl");
            let (bytes, method, mode) = read_entry(&output, "package/comma,name.py").await?;
            assert_eq!(
                (bytes, method, mode),
                (b"untouched".to_vec(), compression, Some(0o100_640))
            );
            let archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(fs_err::File::open(
                &output,
            )?)))
            .await?;
            let entry = archive
                .file()
                .entries()
                .iter()
                .find(|entry| entry.filename().as_bytes() == b"package/comma,name.py")
                .context("missing metadata fixture")?;
            assert_eq!(entry.last_modification_date(), &stamp);
            assert_eq!(entry.internal_file_attribute(), 1);
            assert_eq!(entry.comment().as_bytes(), b"entry comment");
            assert!(archive.file().comment().as_bytes().is_empty());
            let (record, method, mode) = read_entry(&output, RECORD).await?;
            assert_eq!(method, Compression::Deflate);
            assert_eq!(mode, Some(0o100_644));
            let entries = read_record(&record, RECORD)?;
            assert_eq!(entries["package/comma,name.py"].size, 9);
        }
        Ok(())
    }

    /// Rewriting intentionally discards unknown extra fields and emits Unix creator metadata.
    #[tokio::test]
    async fn drops_unknown_extras_and_normalizes_creator_host() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        fixture(&args.input, fixture_members(), None, false, false).await?;
        let mut bytes = fs_err::read(&args.input)?;
        let central = bytes
            .windows(4)
            .position(|bytes| bytes == b"PK\x01\x02")
            .context("missing central header")?;
        let end = bytes
            .windows(4)
            .rposition(|bytes| bytes == b"PK\x05\x06")
            .context("missing end record")?;
        let name_size = u16::from_le_bytes(bytes[central + 28..central + 30].try_into()?) as usize;
        // Add one unknown central extra field, leaving local offsets and payload bytes intact.
        bytes[central + 5] = 0; // DOS creator host, normalized to Unix by the reader/writer.
        bytes[central + 30..central + 32].copy_from_slice(&8_u16.to_le_bytes());
        let directory_size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into()?);
        bytes[end + 12..end + 16].copy_from_slice(&(directory_size + 8).to_le_bytes());
        bytes.splice(
            central + 46 + name_size..central + 46 + name_size,
            [0xfe, 0xca, 4, 0, 1, 2, 3, 4],
        );
        fs_err::write(&args.input, bytes)?;
        wheel_replace(args).await?;
        let bytes = fs_err::read(directory.path().join("output.whl"))?;
        let central = bytes
            .windows(4)
            .position(|bytes| bytes == b"PK\x01\x02")
            .context("missing output central header")?;
        assert_eq!(bytes[central + 5], 3);
        assert_eq!(
            u16::from_le_bytes(bytes[central + 30..central + 32].try_into()?),
            0
        );
        Ok(())
    }

    /// Multiple replacements produce the expected bytes, metadata, and `RECORD` entries together.
    #[tokio::test]
    async fn preserves_multiple_replacements() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut args = args(directory.path());
        let mut members = fixture_members();
        let second_binary = "uv-1.2.3.data/scripts/uvx";
        members.push((
            ZipEntryBuilder::new(second_binary.into(), Compression::Deflate)
                .unix_permissions(0o100_755)
                .build(),
            b"unsigned uvx".to_vec(),
        ));
        fixture(&args.input, members.clone(), None, false, false).await?;
        fs_err::write(&args.replacements[0].path, b"signed uv")?;
        let second_path = directory.path().join("signed-uvx");
        fs_err::write(&second_path, b"signed uvx")?;
        args.replacements.push(Replacement {
            member: second_binary.to_string(),
            path: second_path,
        });
        // Derive expected bytes from the fixture and replacements, not from the output archive.
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
        wheel_replace(args).await?;
        verify_output(&directory.path().join("output.whl"), &expected).await?;
        Ok(())
    }

    /// Check output membership, metadata, hashes, sizes, and `RECORD` against fixture expectations.
    async fn verify_output(
        path: &Path,
        expected: &BTreeMap<String, (ZipEntry, String, u64)>,
    ) -> Result<()> {
        let file = fs_err::File::open(path)?;
        let mut archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(file))).await?;
        let record_path = validate_archive(archive.file())?;
        ensure!(
            archive.file().entries().len() == expected.len(),
            "output membership changed"
        );
        ensure!(
            archive.file().comment().as_bytes().is_empty(),
            "unexpected output archive comment"
        );
        let (record_bytes, _, _) = read_entry(path, &record_path).await?;
        let mut record = read_record(&record_bytes, &record_path)?;
        for index in 0..archive.file().entries().len() {
            let entry = archive.file().entries()[index].clone();
            let name = entry.filename().as_str()?;
            let (metadata, expected_hash, expected_size) =
                expected.get(name).context("unexpected output member")?;
            ensure!(
                entry
                    .extra_fields()
                    .iter()
                    .all(|field| matches!(field.header_id().0, 0x0001 | 0x6375 | 0x7075)),
                "unexpected output extra field for `{name}`"
            );
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
            reader.read_to_end(&mut bytes).await?;
            let size = bytes.len() as u64;
            let hash = BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes));
            validate_zip_contents(&mut reader, size)?;
            ensure!(
                &hash == expected_hash && size == *expected_size,
                "output bytes changed for `{name}`"
            );
            if !entry.dir()? && name != record_path {
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

    /// Write a basic wheel fixture with the requested Unix file type and mode for its executable.
    async fn write_wheel(path: &Path, executable_mode: u16) -> Result<()> {
        fixture(
            path,
            vec![
                (
                    ZipEntryBuilder::new(BINARY.into(), Compression::Deflate)
                        .unix_permissions(executable_mode)
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
            false,
            false,
        )
        .await
    }

    /// Read an exact member name with CRC32 checking, returning its bytes, compression, and mode.
    async fn read_entry(path: &Path, name: &str) -> Result<(Vec<u8>, Compression, Option<u16>)> {
        let bytes = fs_err::read(path)?;
        let mut archive =
            ZipFileReader::new(AllowStdIo::new(BufReader::new(Cursor::new(bytes)))).await?;
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
        let replacement = temporary.path().join("uv");
        write_wheel(&input, 0o100_755).await?;
        fs_err::write(&replacement, b"signed executable")?;

        wheel_replace(WheelReplaceArgs {
            input,
            output: output.clone(),
            replacements: vec![Replacement {
                member: "uv-1.2.3.data/scripts/uv".to_string(),
                path: replacement,
            }],
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

    /// A symlink member cannot become a regular file merely because its contents are replaced.
    #[tokio::test]
    async fn rejects_a_symlink_member() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let input = temporary.path().join("unsigned.whl");
        let output = temporary.path().join("signed.whl");
        let replacement = temporary.path().join("uv");
        write_wheel(&input, 0o120_777).await?;
        fs_err::write(&replacement, b"signed executable")?;

        let error = wheel_replace(WheelReplaceArgs {
            input,
            output: output.clone(),
            replacements: vec![Replacement {
                member: "uv-1.2.3.data/scripts/uv".to_string(),
                path: replacement,
            }],
        })
        .await
        .expect_err("symlink wheel members should be rejected");

        assert_eq!(
            error.to_string(),
            "wheel member `uv-1.2.3.data/scripts/uv` is not a regular file or directory"
        );
        assert!(!output.exists());
        Ok(())
    }

    /// Replacement arguments require a separator and nonempty member and path components.
    #[test]
    fn validates_replacement_arguments() {
        let valid = Replacement::from_str("uv-1.2.3.data/scripts/uv=/signed/uv")
            .expect("valid replacement should parse");
        assert_eq!(valid.member, "uv-1.2.3.data/scripts/uv");
        assert_eq!(valid.path, Path::new("/signed/uv"));
        assert!(Replacement::from_str("missing-separator").is_err());
        assert!(Replacement::from_str("=/signed/uv").is_err());
        assert!(Replacement::from_str("uv=").is_err());
    }
}
