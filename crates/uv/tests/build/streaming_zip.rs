use assert_cmd::prelude::*;
use indoc::indoc;

/// Streaming and seekable readers can extract different bytes even when the sizes and CRC32 agree.
#[tokio::test]
async fn reject_conflicting_compression_methods() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");
    let archive = context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
            import io
            import sys
            import zipfile

            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
                # This text and its raw DEFLATE encoding have the same length and CRC32.
                archive.writestr("file.txt", b"aaaaaGFENFEGG")
                # Only the central directory, written on close, will say the entry is stored.
                archive.getinfo("file.txt").compress_type = zipfile.ZIP_STORED
            sys.stdout.buffer.write(buffer.getvalue())
        "#})
        .assert()
        .success();

    let bytes = archive.get_output().stdout.as_slice();
    let error = uv_extract::stream::unzip(bytes, tempfile::tempdir()?)
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(error, @r#"
    ConflictingCompressionMethods {
        path: "file.txt",
        offset: 0,
        local_method: 8,
        central_directory_method: 0,
    }
    "#);

    let path = context.temp_dir.path().join("archive.zip");
    fs_err::write(&path, bytes)?;
    let error =
        uv_extract::unzip(fs_err::File::open(path)?, tempfile::tempdir()?.path()).unwrap_err();
    insta::assert_debug_snapshot!(error, @r#"
    AsyncZip(
        LocalFileHeaderCompressionMismatch,
    )
    "#);
    Ok(())
}

/// A shorter compressed size in the central directory makes seekable readers truncate the entry.
#[tokio::test]
async fn reject_data_descriptor_conflicting_compressed_size() -> anyhow::Result<()> {
    let context = uv_test::test_context!("3.12");
    let archive = context
        .python_command()
        .arg("-c")
        .arg(indoc! {r#"
            import sys
            import zipfile

            # Python 3.12 can treat a Windows pipe as seekable. Expose only write and flush
            # so zipfile always emits a data descriptor.
            class StreamingOutput:
                def write(self, data):
                    return sys.stdout.buffer.write(data)

                def flush(self):
                    sys.stdout.buffer.flush()

            with zipfile.ZipFile(StreamingOutput(), "w", zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("file.txt", b"hello")
                entry = archive.getinfo("file.txt")
                # Bit 3 indicates that zipfile wrote a data descriptor after the compressed data.
                assert entry.flag_bits & 0x08
                # The descriptor already records 7 bytes. Closing the archive writes the directory.
                entry.compress_size = 1
        "#})
        .assert()
        .success();

    let error =
        uv_extract::stream::unzip(archive.get_output().stdout.as_slice(), tempfile::tempdir()?)
            .await
            .unwrap_err();
    insta::assert_debug_snapshot!(error, @r#"
    ConflictingCompressedSizes {
        path: "file.txt",
        offset: 0,
        local_compressed_size: 7,
        central_directory_compressed_size: 1,
    }
    "#);
    Ok(())
}
