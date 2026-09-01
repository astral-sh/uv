use std::collections::BTreeMap;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntry, ZipEntryBuilder, ZipFile};
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use clap::Parser;
use futures::io::{AllowStdIo, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use sha2::{Digest, Sha256, Sha384, Sha512};

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

    fn from_str(value: &str) -> Result<Self> {
        let Some((member, path)) = value.split_once('=') else {
            bail!("expected `MEMBER=PATH`, got `{value}`");
        };
        ensure!(!member.is_empty(), "replacement member cannot be empty");
        ensure!(!path.is_empty(), "replacement path cannot be empty");
        validate_member_name(member)?;
        Ok(Self {
            member: member.to_string(),
            path: PathBuf::from(path),
        })
    }
}

#[derive(Debug)]
struct RecordEntry {
    hash: String,
    size: u64,
}

#[derive(Debug)]
struct HashDigests {
    sha256: String,
    sha384: String,
    sha512: String,
}

/// Rewrite a trusted wheel without extracting its members to the filesystem.
///
/// Preserve exact decompressed bytes except for explicitly replaced members and RECORD. Preserve
/// each non-RECORD member's compression method, DOS timestamp, internal/external attributes and
/// entry comment. Recompression does not preserve compressed streams, local headers, compression
/// levels, arbitrary extra fields or the archive comment. The ZIP library emits Unix creator
/// metadata, including for DOS inputs. RECORD is emitted last with SHA-256 hashes, Deflate, mode
/// 0644, the ZIP epoch timestamp and no comment. Structural ZIP64 fields are generated as needed.
///
/// Flush and reopen the completed temporary archive to verify its membership, metadata and bytes
/// before atomically creating the output with no clobber. Existing output is never reused, even
/// if it has identical bytes. Failure removes the temporary file and leaves existing paths alone.
/// This is process-level atomic publication, not a promise of power-loss durability. Callers own
/// provenance, digest verification and immutable input/replacement staging.
/// Names must be portable ASCII paths with no Windows device components or case-folded aliases,
/// including implicit parent directories. Unsupported Unicode names are rejected, not normalized.
pub(crate) async fn wheel_replace(args: WheelReplaceArgs) -> Result<()> {
    ensure!(
        args.input != args.output,
        "input and output wheels must be different"
    );
    let mut replacements = BTreeMap::new();
    for replacement in args.replacements {
        let member = replacement.member;
        validate_member_name(&member)?;
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
    let (record_index, record_path) = validate_archive(archive.file())?;
    let mut record_bytes = Vec::new();
    let mut record_reader = archive.reader_with_entry(record_index).await?;
    copy_hashed(&mut record_reader, &mut record_bytes).await?;
    validate_zip_contents(&mut record_reader, record_bytes.len() as u64)?;
    let mut expected_record = read_record(&record_bytes, &record_path)?;

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
    let mut output_members = BTreeMap::new();

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

        let (hash, size) = if entry.dir()? {
            ensure!(
                !replacements.contains_key(&name),
                "cannot replace directory member `{name}`"
            );
            let mut original = archive.reader_with_entry(index).await?;
            let contents = hash_reader(&mut original).await?;
            ensure!(contents.1 == 0, "directory member `{name}` contains data");
            validate_zip_contents(&mut original, contents.1)?;
            writer.write_entry_whole(builder.clone(), &[]).await?;
            contents
        } else {
            let expected = expected_record
                .remove(&name)
                .with_context(|| format!("RECORD does not contain `{name}`"))?;
            let mut original = archive.reader_with_entry(index).await?;
            let contents = if let Some(path) = replacements.remove(&name) {
                let (hash, size) = hash_reader(&mut original).await?;
                validate_zip_contents(&mut original, size)?;
                validate_record_entry(&name, &expected, &hash, size)?;
                let replacement = fs_err::File::open(&path)
                    .with_context(|| format!("failed to open replacement `{}`", path.display()))?;
                let metadata = replacement.metadata()?;
                ensure!(
                    metadata.is_file(),
                    "replacement `{}` is not a regular file",
                    path.display()
                );
                let mut replacement = AllowStdIo::new(BufReader::new(replacement));
                let mut output_entry = writer.write_entry_seekable(builder.clone()).await?;
                let contents = copy_hashed(&mut replacement, &mut output_entry).await?;
                output_entry.close().await?;
                contents
            } else {
                let mut output_entry = writer.write_entry_seekable(builder.clone()).await?;
                let (hash, size) = copy_hashed(&mut original, &mut output_entry).await?;
                validate_zip_contents(&mut original, size)?;
                output_entry.close().await?;
                validate_record_entry(&name, &expected, &hash, size)?;
                (hash, size)
            };
            output_record.push((name.clone(), contents.0.sha256.clone(), contents.1));
            contents
        };
        output_members.insert(name, (builder, hash.sha256, size));
    }
    ensure!(
        expected_record.is_empty(),
        "RECORD contains members not present in the wheel: {}",
        expected_record
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
    ensure!(
        replacements.is_empty(),
        "replacement members not present in the wheel: {}",
        replacements.keys().cloned().collect::<Vec<_>>().join(", ")
    );

    let record_bytes = write_record(&record_path, output_record)?;
    let record_entry = ZipEntryBuilder::new(record_path.clone().into(), Compression::Deflate)
        .unix_permissions(0o100_644)
        .last_modification_date(async_zip::ZipDateTime::default())
        .build();
    writer
        .write_entry_whole(record_entry.clone(), &record_bytes)
        .await?;
    output_members.insert(
        record_path,
        (
            record_entry,
            BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&record_bytes)),
            record_bytes.len() as u64,
        ),
    );
    finish_archive(writer).await?;
    verify_output(temporary.path(), &output_members).await?;
    temporary.persist_noclobber(&args.output).with_context(|| {
        format!(
            "failed to create output wheel `{}` without overwriting",
            args.output.display()
        )
    })?;
    Ok(())
}

/// Closing ZIP writes the central directory but does not flush the underlying buffered writer.
async fn finish_archive<W: AsyncWrite + Unpin>(writer: ZipFileWriter<W>) -> Result<()> {
    let mut output = writer.close().await?;
    output
        .flush()
        .await
        .context("failed to flush completed wheel")?;
    Ok(())
}

fn validate_archive(file: &ZipFile) -> Result<(usize, String)> {
    let mut names = BTreeMap::new();
    let mut portable_names = BTreeMap::new();
    let mut record = None;
    for (index, entry) in file.entries().iter().enumerate() {
        let name = entry
            .filename()
            .as_str()
            .context("wheel member name is not valid UTF-8")?;
        validate_member_name(name)?;
        validate_member_type(name, entry.unix_permissions(), entry.dir()?)?;
        let path = name.trim_end_matches('/');
        for end in path
            .match_indices('/')
            .map(|(index, _)| index)
            .chain([path.len()])
        {
            let prefix = &path[..end];
            if let Some(previous) = portable_names.insert(prefix.to_ascii_lowercase(), prefix) {
                ensure!(
                    previous == prefix,
                    "wheel members alias `{previous}` and `{prefix}`"
                );
            }
        }
        ensure!(
            names
                .insert(name.trim_end_matches('/').to_string(), entry.dir()?)
                .is_none(),
            "duplicate or aliased wheel member `{name}`"
        );
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
            record = Some((index, name.to_string()));
        }
    }
    for name in names.keys() {
        for (index, _) in name.match_indices('/') {
            ensure!(
                names.get(&name[..index]) != Some(&false),
                "wheel member `{name}` has a file as a parent"
            );
        }
    }
    record.context("wheel does not contain a RECORD file")
}

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

async fn verify_output(
    path: &Path,
    expected: &BTreeMap<String, (ZipEntry, String, u64)>,
) -> Result<()> {
    let file = fs_err::File::open(path)?;
    let mut archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(file))).await?;
    let (record_index, record_path) = validate_archive(archive.file())?;
    ensure!(
        archive.file().entries().len() == expected.len(),
        "output membership changed"
    );
    ensure!(
        archive.file().comment().as_bytes().is_empty(),
        "unexpected output archive comment"
    );
    let mut record_bytes = Vec::new();
    let mut reader = archive.reader_with_entry(record_index).await?;
    copy_hashed(&mut reader, &mut record_bytes).await?;
    validate_zip_contents(&mut reader, record_bytes.len() as u64)?;
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
        let (hash, size) = hash_reader(&mut reader).await?;
        validate_zip_contents(&mut reader, size)?;
        ensure!(
            &hash.sha256 == expected_hash && size == *expected_size,
            "output bytes changed for `{name}`"
        );
        if !entry.dir()? && name != record_path {
            let expected = record
                .remove(name)
                .context("output RECORD is missing a member")?;
            validate_record_entry(name, &expected, &hash, size)?;
        }
    }
    ensure!(record.is_empty(), "output RECORD contains extra members");
    Ok(())
}

fn read_record(bytes: &[u8], record_path: &str) -> Result<BTreeMap<String, RecordEntry>> {
    let mut entries = BTreeMap::new();
    let mut record_seen = false;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes);
    for row in reader.records() {
        let row = row.context("failed to parse RECORD")?;
        ensure!(
            row.len() == 3,
            "RECORD rows must contain exactly three fields"
        );
        let path = row.get(0).context("RECORD row has no path")?;
        validate_member_name(path)?;
        if path == record_path {
            ensure!(!record_seen, "duplicate RECORD entry `{path}`");
            record_seen = true;
            ensure!(
                row.get(1) == Some("") && row.get(2) == Some(""),
                "RECORD entry for itself must not contain a hash or size"
            );
            continue;
        }
        let hash = row.get(1).context("RECORD row has no hash")?;
        ensure!(
            hash.starts_with("sha256=")
                || hash.starts_with("sha384=")
                || hash.starts_with("sha512="),
            "RECORD entry `{path}` must use a secure hash"
        );
        let size = row
            .get(2)
            .context("RECORD row has no size")?
            .parse::<u64>()
            .with_context(|| format!("RECORD entry `{path}` has an invalid size"))?;
        ensure!(
            entries
                .insert(
                    path.to_string(),
                    RecordEntry {
                        hash: hash.to_string(),
                        size,
                    },
                )
                .is_none(),
            "duplicate RECORD entry `{path}`"
        );
    }
    ensure!(record_seen, "RECORD does not contain an entry for itself");
    Ok(entries)
}

fn write_record(record_path: &str, entries: Vec<(String, String, u64)>) -> Result<Vec<u8>> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    for (path, hash, size) in entries {
        writer.write_record([path, format!("sha256={hash}"), size.to_string()])?;
    }
    writer.write_record([record_path, "", ""])?;
    writer.flush()?;
    writer.into_inner().context("failed to finish RECORD")
}

fn validate_record_entry(
    name: &str,
    expected: &RecordEntry,
    hash: &HashDigests,
    size: u64,
) -> Result<()> {
    let actual = if expected.hash.starts_with("sha256=") {
        format!("sha256={}", hash.sha256)
    } else if expected.hash.starts_with("sha384=") {
        format!("sha384={}", hash.sha384)
    } else {
        format!("sha512={}", hash.sha512)
    };
    ensure!(
        expected.hash == actual,
        "RECORD hash for `{name}` does not match its contents"
    );
    ensure!(
        expected.size == size,
        "RECORD size for `{name}` does not match its contents"
    );
    Ok(())
}

fn validate_member_name(name: &str) -> Result<()> {
    ensure!(!name.is_empty(), "wheel member name cannot be empty");
    ensure!(
        name.is_ascii(),
        "wheel member `{name}` is not a portable ASCII path"
    );
    ensure!(
        !name.starts_with('/'),
        "absolute wheel member `{name}` is invalid"
    );
    ensure!(
        !name.contains('\\'),
        "wheel member `{name}` contains a backslash"
    );
    ensure!(
        !name.chars().any(char::is_control),
        "wheel member `{name}` contains a control character"
    );
    ensure!(name.len() <= 4096, "wheel member name is too long");
    // A single trailing slash denotes a directory. Do not normalize any other component:
    // extraction must not collapse two different ZIP names to the same portable path.
    for component in name.strip_suffix('/').unwrap_or(name).split('/') {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "wheel member `{name}` contains an empty or dot component"
        );
        ensure!(
            component.len() <= 255
                && !component.contains([':', '<', '>', '"', '|', '?', '*', '~'])
                && !component.ends_with(['.', ' ']),
            "wheel member `{name}` has a non-portable component"
        );
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        ensure!(
            !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "wheel member `{name}` uses a reserved device name"
        );
    }
    Ok(())
}

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

async fn hash_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<(HashDigests, u64)> {
    copy_hashed(reader, &mut futures::io::sink()).await
}

async fn copy_hashed(
    reader: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> Result<(HashDigests, u64)> {
    let mut sha256 = Sha256::new();
    let mut sha384 = Sha384::new();
    let mut sha512 = Sha512::new();
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
        sha384.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
    }
    Ok((
        HashDigests {
            sha256: BASE64_URL_SAFE_NO_PAD.encode(sha256.finalize()),
            sha384: BASE64_URL_SAFE_NO_PAD.encode(sha384.finalize()),
            sha512: BASE64_URL_SAFE_NO_PAD.encode(sha512.finalize()),
        },
        size,
    ))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Write};

    use async_zip::ZipDateTimeBuilder;

    use super::*;

    const BINARY: &str = "uv-1.2.3.data/scripts/uv";
    const RECORD: &str = "uv-1.2.3.dist-info/RECORD";
    const METADATA: &str = "uv-1.2.3.dist-info/METADATA";

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

    #[test]
    fn rejects_noncanonical_names() {
        for name in [
            "",
            "/uv",
            "../uv",
            "a/../uv",
            "./uv",
            "a/./uv",
            "a//uv",
            "a//",
            "C:uv",
            "C:/uv",
            "a/C:uv",
            "a\\uv",
            "a\0uv",
            "a\nuv",
            "a\u{7f}uv",
            "a./uv",
            "a /uv",
        ] {
            assert!(validate_member_name(name).is_err(), "{name:?}");
        }
        for name in [
            BINARY,
            RECORD,
            "uv-1.2.3.data/scripts/",
            "package/comma,name.py",
        ] {
            validate_member_name(name).expect("canonical wheel member");
        }
    }

    #[tokio::test]
    async fn rejects_bad_members_and_cleans_up() -> Result<()> {
        for (name, mode, contents) in [
            ("a/./b", 0o100_644, b"data".as_slice()),
            ("a//b", 0o100_644, b"data"),
            ("a/../b", 0o100_644, b"data"),
            (BINARY, 0o100_644, b"duplicate"),
            ("link", 0o120_777, b"target"),
            ("fifo", 0o010_644, b""),
            ("socket", 0o140_644, b""),
            ("dir/", 0o040_755, b"not empty"),
            ("uv-1.2.3.dist-info/RECORD.jws", 0o100_644, b"signature"),
            ("uv-1.2.3.dist-info/RECORD.p7s", 0o100_644, b"signature"),
            ("uv-1.2.3.data/scripts", 0o100_644, b"file parent"),
            ("uv-1.2.3.data/scripts/uv/", 0o040_755, b""),
            ("uv-1.2.3.data/scripts/UV", 0o100_755, b"case alias"),
            ("UV-1.2.3.data/another", 0o100_644, b"parent alias"),
            ("NUL.txt", 0o100_644, b"device"),
            ("package/é.py", 0o100_644, b"unicode"),
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

    #[tokio::test]
    async fn rejects_invalid_record_rows() -> Result<()> {
        let members = fixture_members();
        let record = String::from_utf8(fixture_record(&members)?)?;
        let rows: Vec<_> = record.lines().collect();
        let malformed = [
            record.replace(
                &BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(b"metadata")),
                "invalid",
            ),
            record.replace(",8\n", ",9\n"),
            format!("{record}{}\n", rows[1]),
            format!("{}\n{}\n", rows[0], rows[2]),
            format!("{record}missing,sha256=missing,1\n"),
            format!("{}\n{}\n", rows[0], rows[1]),
            record.replace(
                &format!("{RECORD},,"),
                &format!("{RECORD},sha256=invalid,1"),
            ),
            format!("{record}{RECORD},,\n"),
            record.replace(",8\n", ",8,extra\n"),
        ];
        for record in malformed {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            fixture(
                &args.input,
                fixture_members(),
                Some(record.into_bytes()),
                false,
                false,
            )
            .await?;
            assert!(wheel_replace(args).await.is_err());
            assert_eq!(fs_err::read_dir(directory.path())?.count(), 2);
        }
        Ok(())
    }

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
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                Err(io::Error::other("injected buffered write failure"))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }

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

    #[tokio::test]
    async fn replaces_in_a_wheel_with_many_members() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        let mut members = fixture_members();
        // Wheel replacement must not impose an entry-count limit on trusted build artifacts.
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

    #[tokio::test]
    async fn streams_across_buffer_boundaries() -> Result<()> {
        let bytes = vec![42; BUFFER_SIZE + 1];
        let mut input = futures::io::Cursor::new(&bytes);
        let mut output = Vec::new();
        let (hash, size) = copy_hashed(&mut input, &mut output).await?;
        assert_eq!(output, bytes);
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(
            hash.sha256,
            BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(&bytes))
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_corrupt_zip_contents() -> Result<()> {
        for (field, replacement) in [
            (16, 0_u32), // CRC32
            (24, 9_u32), // uncompressed size
        ] {
            let directory = tempfile::tempdir()?;
            let args = args(directory.path());
            fs_err::write(&args.replacements[0].path, b"signed")?;
            fixture(&args.input, fixture_members(), None, false, false).await?;
            let mut bytes = fs_err::read(&args.input)?;
            let position = bytes
                .windows(4)
                .position(|bytes| bytes == b"PK\x01\x02")
                .context("missing ZIP header")?;
            bytes[position + field..position + field + 4]
                .copy_from_slice(&replacement.to_le_bytes());
            fs_err::write(&args.input, bytes)?;
            assert!(wheel_replace(args).await.is_err());
            assert_eq!(fs_err::read_dir(directory.path())?.count(), 2);
        }
        Ok(())
    }

    #[tokio::test]
    async fn preserves_declared_metadata_and_bytes() -> Result<()> {
        for (compression, streaming, zip64) in [
            (Compression::Stored, false, false),
            (Compression::Deflate, true, false),
            (Compression::Bz, false, true),
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

    #[tokio::test]
    async fn verifies_completed_output_against_the_transform() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let args = args(directory.path());
        fs_err::write(&args.replacements[0].path, b"signed")?;
        fixture(&args.input, fixture_members(), None, false, false).await?;
        wheel_replace(args).await?;
        let output = directory.path().join("output.whl");
        let mut archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(fs_err::File::open(
            &output,
        )?)))
        .await?;
        let mut expected = BTreeMap::new();
        for index in 0..archive.file().entries().len() {
            let entry = archive.file().entries()[index].clone();
            let mut reader = archive.reader_with_entry(index).await?;
            let (hash, size) = hash_reader(&mut reader).await?;
            expected.insert(
                entry.filename().as_str()?.to_string(),
                ((*entry).clone(), hash.sha256, size),
            );
        }
        verify_output(&output, &expected).await?;
        expected
            .get_mut(BINARY)
            .context("missing expected binary")?
            .1 = "different signed bytes".to_string();
        assert_eq!(
            verify_output(&output, &expected)
                .await
                .expect_err("reject substituted bytes")
                .to_string(),
            format!("output bytes changed for `{BINARY}`")
        );
        expected.remove(BINARY);
        assert_eq!(
            verify_output(&output, &expected)
                .await
                .expect_err("reject extra member")
                .to_string(),
            "output membership changed"
        );
        Ok(())
    }

    async fn write_wheel(
        path: &Path,
        tamper_record: bool,
        executable_mode: u16,
        algorithm: &str,
    ) -> Result<()> {
        let executable = b"unsigned executable";
        let metadata = b"Metadata-Version: 2.4\nName: uv\nVersion: 1.2.3\n";
        let digest = |bytes: &[u8]| -> Result<String> {
            match algorithm {
                "sha256" => Ok(BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(bytes))),
                "sha384" => Ok(BASE64_URL_SAFE_NO_PAD.encode(Sha384::digest(bytes))),
                "sha512" => Ok(BASE64_URL_SAFE_NO_PAD.encode(Sha512::digest(bytes))),
                _ => bail!("unsupported test hash algorithm `{algorithm}`"),
            }
        };
        let executable_hash = digest(executable)?;
        let metadata_hash = digest(metadata)?;
        let record = format!(
            "uv-1.2.3.data/scripts/uv,{algorithm}={},{}\nuv-1.2.3.dist-info/METADATA,{algorithm}={metadata_hash},{}\nuv-1.2.3.dist-info/RECORD,,\n",
            if tamper_record {
                "invalid"
            } else {
                &executable_hash
            },
            executable.len(),
            metadata.len()
        );

        let output = fs_err::File::create(path)?;
        let mut writer = ZipFileWriter::new(AllowStdIo::new(BufWriter::new(output)));
        writer
            .write_entry_whole(
                ZipEntryBuilder::new("uv-1.2.3.data/scripts/uv".into(), Compression::Deflate)
                    .unix_permissions(executable_mode),
                executable,
            )
            .await?;
        writer
            .write_entry_whole(
                ZipEntryBuilder::new("uv-1.2.3.dist-info/METADATA".into(), Compression::Stored)
                    .unix_permissions(0o100_644),
                metadata,
            )
            .await?;
        writer
            .write_entry_whole(
                ZipEntryBuilder::new("uv-1.2.3.dist-info/RECORD".into(), Compression::Deflate)
                    .unix_permissions(0o100_644),
                record.as_bytes(),
            )
            .await?;
        finish_archive(writer).await?;
        Ok(())
    }

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
            .read_to_end(&mut bytes)
            .await?;
        Ok((bytes, compression, permissions))
    }

    #[tokio::test]
    async fn replaces_executable_and_regenerates_record() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let input = temporary.path().join("unsigned.whl");
        let output = temporary.path().join("signed.whl");
        let replacement = temporary.path().join("uv");
        write_wheel(&input, false, 0o100_755, "sha256").await?;
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

    #[tokio::test]
    async fn rejects_an_invalid_input_record() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let input = temporary.path().join("unsigned.whl");
        let output = temporary.path().join("signed.whl");
        let replacement = temporary.path().join("uv");
        write_wheel(&input, true, 0o100_755, "sha256").await?;
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
        .expect_err("invalid RECORD should be rejected");

        assert_eq!(
            error.to_string(),
            "RECORD hash for `uv-1.2.3.data/scripts/uv` does not match its contents"
        );
        assert!(!output.exists());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_symlink_member() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let input = temporary.path().join("unsigned.whl");
        let output = temporary.path().join("signed.whl");
        let replacement = temporary.path().join("uv");
        write_wheel(&input, false, 0o120_777, "sha256").await?;
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

    #[tokio::test]
    async fn accepts_secure_input_record_hashes() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        for algorithm in ["sha384", "sha512"] {
            let input = temporary.path().join(format!("unsigned-{algorithm}.whl"));
            let output = temporary.path().join(format!("signed-{algorithm}.whl"));
            let replacement = temporary.path().join(format!("uv-{algorithm}"));
            write_wheel(&input, false, 0o100_755, algorithm).await?;
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

            let (record, _, _) = read_entry(&output, "uv-1.2.3.dist-info/RECORD").await?;
            let record = String::from_utf8(record)?;
            assert!(record.contains(",sha256="));
            assert!(!record.contains(&format!(",{algorithm}=")));
        }
        Ok(())
    }

    #[test]
    fn validates_replacement_arguments() {
        let valid = Replacement::from_str("uv-1.2.3.data/scripts/uv=/signed/uv")
            .expect("valid replacement should parse");
        assert_eq!(valid.member, "uv-1.2.3.data/scripts/uv");
        assert_eq!(valid.path, Path::new("/signed/uv"));
        assert!(Replacement::from_str("missing-separator").is_err());
        assert!(Replacement::from_str("../uv=/signed/uv").is_err());
        assert!(Replacement::from_str("/absolute/uv=/signed/uv").is_err());
    }
}
