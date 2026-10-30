//! ZIP fixtures from <https://github.com/fastzip/malo/tree/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/zip>.

use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/archives/malo")
        .join(name)
}

async fn unzip(name: &str) -> Result<(), uv_extract::Error> {
    let archive = fs_err::tokio::File::open(fixture_path(name))
        .await
        .map_err(uv_extract::Error::Io)?;
    let target = tempfile::tempdir().map_err(uv_extract::Error::Io)?;
    uv_extract::stream::unzip(archive, target).await?;
    Ok(())
}

fn unzip_seekable(name: &str) -> Result<(), uv_extract::Error> {
    let archive = fs_err::File::open(fixture_path(name)).map_err(uv_extract::Error::Io)?;
    let target = tempfile::tempdir().map_err(uv_extract::Error::Io)?;
    uv_extract::unzip(archive, target.path())?;
    Ok(())
}

#[tokio::test]
async fn malo_accept_comment() {
    unzip("accept/comment.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_data_descriptor_zip64() {
    unzip("accept/data_descriptor_zip64.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_data_descriptor() {
    unzip("accept/data_descriptor.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_deflate() {
    unzip("accept/deflate.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[test]
fn malo_seekable_accept_deflate() {
    unzip_seekable("accept/deflate.zip").unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_normal_deflate_zip64_extra() {
    unzip("accept/normal_deflate_zip64_extra.zip")
        .await
        .unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_normal_deflate() {
    unzip("accept/normal_deflate.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_store() {
    unzip("accept/store.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_subdir() {
    unzip("accept/subdir.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_zip64_eocd() {
    unzip("accept/zip64_eocd.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_iffy_8bitcomment() {
    let result = unzip("iffy/8bitcomment.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        ZipInZip,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_extra3byte() {
    let result = unzip("iffy/extra3byte.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Ok(
        (),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_non_ascii_original_name() {
    let result = unzip("iffy/non_ascii_original_name.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        LocalHeaderNotUtf8 {
            offset: 0,
        },
    )
    ");
}

#[tokio::test]
async fn malo_iffy_nosubdir() {
    let result = unzip("iffy/nosubdir.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Ok(
        (),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_prefix() {
    let result = unzip("iffy/prefix.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        AsyncZip(
            UnexpectedHeaderError(
                1482184792,
                67324752,
            ),
        ),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_suffix_not_comment() {
    let result = unzip("iffy/suffix_not_comment.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        TrailingContents,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_zip64_eocd_extensible_data() {
    let result = unzip("iffy/zip64_eocd_extensible_data.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        ExtensibleData,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_zip64_extra_too_long() {
    let result = unzip("iffy/zip64_extra_too_long.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        AsyncZip(
            Zip64ExtendedInformationFieldTooLong {
                expected: 16,
                actual: 8,
            },
        ),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_zip64_extra_too_short() {
    let result = unzip("iffy/zip64_extra_too_short.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        AsyncZip(
            Zip64ExtendedFieldIncomplete,
        ),
    )
    ");
}

#[tokio::test]
async fn malo_reject_cd_extra_entry() {
    let result = unzip("reject/cd_extra_entry.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    MissingLocalFileHeader {
        path: "fixme",
        offset: 0,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_cd_missing_entry() {
    let result = unzip("reject/cd_missing_entry.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    MissingCentralDirectoryEntry {
        path: "two",
        offset: 42,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_crc_0() {
    let result = unzip("reject/data_descriptor_bad_crc_0.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCrc32 {
        path: "fixme",
        computed: 2183870971,
        expected: 0,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_crc() {
    let result = unzip("reject/data_descriptor_bad_crc.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCrc32 {
        path: "fixme",
        computed: 907060870,
        expected: 1,
    }
    "#);
}

#[test]
fn malo_seekable_malicious_short_usize() {
    let result = unzip_seekable("malicious/short_usize.zip").unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "file",
        computed: 51,
        expected: 9,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_csize() {
    let result = unzip("reject/data_descriptor_bad_csize.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCompressedSize {
        path: "fixme",
        computed: 7,
        expected: 8,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_usize_no_sig() {
    let result = unzip("reject/data_descriptor_bad_usize_no_sig.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "fixme",
        computed: 5,
        expected: 6,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_usize() {
    let result = unzip("reject/data_descriptor_bad_usize.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "fixme",
        computed: 5,
        expected: 6,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_zip64_csize() {
    let result = unzip("reject/data_descriptor_zip64_csize.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCompressedSize {
        path: "fixme",
        computed: 7,
        expected: 8,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_zip64_usize() {
    let result = unzip("reject/data_descriptor_zip64_usize.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "fixme",
        computed: 5,
        expected: 6,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_dupe_eocd() {
    let result = unzip("reject/dupe_eocd.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"TrailingContents");
}

#[tokio::test]
async fn malo_reject_shortextra() {
    let result = unzip("reject/shortextra.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"
    AsyncZip(
        InvalidExtraFieldHeader(
            9,
        ),
    )
    ");
}

#[tokio::test]
async fn malo_reject_zip64_extra_csize() {
    let result = unzip("reject/zip64_extra_csize.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCompressedSize {
        path: "fixme",
        computed: 7,
        expected: 8,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_zip64_extra_usize() {
    let result = unzip("reject/zip64_extra_usize.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "fixme",
        computed: 5,
        expected: 6,
    }
    "#);
}

#[tokio::test]
async fn malo_malicious_second_unicode_extra() {
    let result = unzip("malicious/second_unicode_extra.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @"
    AsyncZip(
        DuplicateExtraFieldHeader(
            28789,
        ),
    )
    ");
}

#[tokio::test]
async fn malo_malicious_short_usize_zip64() {
    let result = unzip("malicious/short_usize_zip64.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"
    AsyncZip(
        Zip64ExtendedInformationFieldTooLong {
            expected: 16,
            actual: 0,
        },
    )
    ");
}

#[tokio::test]
async fn malo_malicious_short_usize() {
    let result = unzip("malicious/short_usize.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadUncompressedSize {
        path: "file",
        computed: 51,
        expected: 9,
    }
    "#);
}

#[tokio::test]
async fn malo_malicious_zip64_eocd_confusion() {
    let result = unzip("malicious/zip64_eocd_confusion.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @"ExtensibleData");
}

#[tokio::test]
async fn malo_malicious_unicode_extra_chain() {
    let result = unzip("malicious/unicode_extra_chain.zip")
        .await
        .unwrap_err();
    insta::assert_debug_snapshot!(result, @"
    AsyncZip(
        DuplicateExtraFieldHeader(
            28789,
        ),
    )
    ");
}

#[tokio::test]
async fn malo_malicious_zipinzip() {
    let result = unzip("malicious/zipinzip.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"ZipInZip");
}
