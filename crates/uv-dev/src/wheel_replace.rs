use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use async_zip::base::read::seek::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
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
    /// The rewritten wheel.
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
    let mut archive = ZipFileReader::new(AllowStdIo::new(BufReader::new(input)))
        .await
        .with_context(|| format!("failed to read input wheel `{}`", args.input.display()))?;

    let mut names = BTreeSet::new();
    let mut record_index = None;
    let mut record_path = None;
    for (index, entry) in archive.file().entries().iter().enumerate() {
        let name = entry
            .filename()
            .as_str()
            .context("wheel member name is not valid UTF-8")?;
        validate_member_name(name)?;
        validate_member_type(name, entry.unix_permissions(), entry.dir()?)?;
        ensure!(
            names.insert(name.to_string()),
            "duplicate wheel member `{name}`"
        );
        ensure!(
            !name.ends_with(".dist-info/RECORD.jws") && !name.ends_with(".dist-info/RECORD.p7s"),
            "wheel contains unsupported RECORD signature `{name}`"
        );
        if name.ends_with(".dist-info/RECORD") {
            ensure!(
                record_index.is_none(),
                "wheel contains multiple RECORD files"
            );
            record_index = Some(index);
            record_path = Some(name.to_string());
        }
    }

    let record_index = record_index.context("wheel does not contain a RECORD file")?;
    let record_path = record_path.context("wheel does not contain a RECORD file")?;
    let mut record_bytes = Vec::new();
    archive
        .reader_with_entry(record_index)
        .await?
        .read_to_end(&mut record_bytes)
        .await
        .context("failed to read RECORD")?;
    let mut expected_record = read_record(&record_bytes, &record_path)?;

    let output_directory = args.output.parent().unwrap_or_else(|| Path::new("."));
    fs_err::create_dir_all(output_directory).with_context(|| {
        format!(
            "failed to create output directory `{}`",
            output_directory.display()
        )
    })?;
    let temporary = tempfile::NamedTempFile::new_in(output_directory).with_context(|| {
        format!(
            "failed to create temporary wheel in `{}`",
            output_directory.display()
        )
    })?;
    let output = temporary
        .reopen()
        .context("failed to reopen temporary wheel")?;
    let mut writer = ZipFileWriter::new(AllowStdIo::new(BufWriter::new(output)));
    let mut output_record = Vec::new();

    for index in 0..archive.file().entries().len() {
        let entry = archive.file().entries()[index].clone();
        let name = entry
            .filename()
            .as_str()
            .context("wheel member name is not valid UTF-8")?
            .to_string();

        if name == record_path {
            continue;
        }

        let mut builder = ZipEntryBuilder::new(name.clone().into(), entry.compression())
            .attribute_compatibility(entry.attribute_compatibility())
            .last_modification_date(*entry.last_modification_date())
            .internal_file_attribute(entry.internal_file_attribute())
            .external_file_attribute(entry.external_file_attribute())
            .comment(entry.comment().clone());

        if entry.dir()? {
            ensure!(
                !replacements.contains_key(&name),
                "cannot replace directory member `{name}`"
            );
            writer.write_entry_whole(builder, &[]).await?;
            continue;
        }

        let expected = expected_record
            .remove(&name)
            .with_context(|| format!("RECORD does not contain `{name}`"))?;

        if let Some(path) = replacements.remove(&name) {
            let mut original = archive.reader_with_entry(index).await?;
            let (hash, size) = hash_reader(&mut original).await?;
            validate_record_entry(&name, &expected, &hash, size)?;

            let replacement = fs_err::File::open(&path)
                .with_context(|| format!("failed to open replacement `{}`", path.display()))?;
            let replacement_size = replacement
                .metadata()
                .with_context(|| format!("failed to stat replacement `{}`", path.display()))?
                .len();
            builder = builder.size(replacement_size, replacement_size);
            let mut replacement = AllowStdIo::new(BufReader::new(replacement));
            let mut output_entry = writer.write_entry_seekable(builder).await?;
            let (hash, size) = copy_hashed(&mut replacement, &mut output_entry).await?;
            output_entry.close().await?;
            output_record.push((name, hash, size));
        } else {
            builder = builder.size(entry.compressed_size(), entry.uncompressed_size());
            let mut original = archive.reader_with_entry(index).await?;
            let mut output_entry = writer.write_entry_seekable(builder).await?;
            let (hash, size) = copy_hashed(&mut original, &mut output_entry).await?;
            output_entry.close().await?;
            validate_record_entry(&name, &expected, &hash, size)?;
            output_record.push((name, hash, size));
        }
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
    let record_entry =
        ZipEntryBuilder::new(record_path.into(), Compression::Deflate).unix_permissions(0o100_644);
    writer
        .write_entry_whole(record_entry, &record_bytes)
        .await?;
    writer.close().await?;
    temporary
        .persist(&args.output)
        .with_context(|| format!("failed to persist output wheel `{}`", args.output.display()))?;
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
            hash.starts_with("sha256="),
            "RECORD entry `{path}` must use a sha256 hash"
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

fn validate_record_entry(name: &str, expected: &RecordEntry, hash: &str, size: u64) -> Result<()> {
    ensure!(
        expected.hash == format!("sha256={hash}"),
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
    ensure!(
        !name.split('/').any(|component| component == ".."),
        "wheel member `{name}` contains a parent-directory component"
    );
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

async fn hash_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<(String, u64)> {
    copy_hashed(reader, &mut futures::io::sink()).await
}

async fn copy_hashed(
    reader: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read]).await?;
        hasher.update(&buffer[..read]);
        size = size
            .checked_add(read as u64)
            .context("wheel member size overflowed")?;
    }
    Ok((BASE64_URL_SAFE_NO_PAD.encode(hasher.finalize()), size))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    async fn write_wheel(path: &Path, tamper_record: bool, executable_mode: u16) -> Result<()> {
        let executable = b"unsigned executable";
        let metadata = b"Metadata-Version: 2.4\nName: uv\nVersion: 1.2.3\n";
        let executable_hash = BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(executable));
        let metadata_hash = BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(metadata));
        let record = format!(
            "uv-1.2.3.data/scripts/uv,sha256={},{}\nuv-1.2.3.dist-info/METADATA,sha256={metadata_hash},{}\nuv-1.2.3.dist-info/RECORD,,\n",
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
        writer.close().await?;
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
        write_wheel(&input, false, 0o100_755).await?;
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
        write_wheel(&input, true, 0o100_755).await?;
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
        write_wheel(&input, false, 0o120_777).await?;
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
