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

            # Writing to a pipe makes zipfile emit a data descriptor after the compressed data.
            with zipfile.ZipFile(sys.stdout.buffer, "w", zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("file.txt", b"hello")
                # The descriptor already records 7 bytes. Closing the archive writes the directory.
                archive.getinfo("file.txt").compress_size = 1
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
