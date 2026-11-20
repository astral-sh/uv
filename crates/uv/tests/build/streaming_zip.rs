use assert_cmd::prelude::*;
use indoc::indoc;

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
