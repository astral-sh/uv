#![cfg(feature = "test-r2")]

use backon::{BackoffBuilder, Retryable};
use futures::TryStreamExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;

async fn unzip(url: &str) -> anyhow::Result<(), uv_extract::Error> {
    let backoff = backon::ExponentialBuilder::default()
        .with_min_delay(std::time::Duration::from_millis(500))
        .with_max_times(5)
        .build();

    let download = || async {
        let response = reqwest::get(url).await?;
        Ok::<_, reqwest::Error>(response)
    };

    let response = download.retry(backoff).await.unwrap();

    let reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read();

    let target = tempfile::TempDir::new().map_err(uv_extract::Error::Io)?;
    uv_extract::stream::unzip(url, reader.compat(), target.path()).await
}

#[tokio::test]
async fn malo_accept_comment() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/comment.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_data_descriptor_zip64() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/data_descriptor_zip64.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_data_descriptor() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/data_descriptor.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_deflate() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/deflate.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_normal_deflate_zip64_extra() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/normal_deflate_zip64_extra.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_normal_deflate() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/normal_deflate.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_store() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/store.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_subdir() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/subdir.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_accept_zip64_eocd() {
    unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/accept/zip64_eocd.zip").await.unwrap();
    insta::assert_debug_snapshot!((), @"()");
}

#[tokio::test]
async fn malo_iffy_8bitcomment() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/8bitcomment.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        ZipInZip,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_extra3byte() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/extra3byte.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Ok(
        (),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_non_ascii_original_name() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/non_ascii_original_name.zip").await;
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/nosubdir.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Ok(
        (),
    )
    ");
}

#[tokio::test]
async fn malo_iffy_prefix() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/prefix.zip").await;
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/suffix_not_comment.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        TrailingContents,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_zip64_eocd_extensible_data() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/zip64_eocd_extensible_data.zip").await;
    insta::assert_debug_snapshot!(result, @"
    Err(
        ExtensibleData,
    )
    ");
}

#[tokio::test]
async fn malo_iffy_zip64_extra_too_long() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/zip64_extra_too_long.zip").await;
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/iffy/zip64_extra_too_short.zip").await;
    insta::assert_debug_snapshot!(result, @r#"
    Err(
        BadCompressedSize {
            path: "fixme",
            computed: 7,
            expected: 4294967295,
        },
    )
    "#);
}

#[tokio::test]
async fn malo_reject_cd_extra_entry() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/cd_extra_entry.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    MissingLocalFileHeader {
        path: "fixme",
        offset: 0,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_cd_missing_entry() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/cd_missing_entry.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    MissingCentralDirectoryEntry {
        path: "two",
        offset: 42,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_crc_0() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_bad_crc_0.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_bad_crc.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @r#"
    BadCrc32 {
        path: "fixme",
        computed: 907060870,
        expected: 1,
    }
    "#);
}

#[tokio::test]
async fn malo_reject_data_descriptor_bad_csize() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_bad_csize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_bad_usize_no_sig.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_bad_usize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_zip64_csize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/data_descriptor_zip64_usize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/dupe_eocd.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"TrailingContents");
}

#[tokio::test]
async fn malo_reject_shortextra() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/shortextra.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/zip64_extra_csize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/reject/zip64_extra_usize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/second_unicode_extra.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/short_usize_zip64.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/short_usize.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/zip64_eocd_confusion.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"ExtensibleData");
}

#[tokio::test]
async fn malo_malicious_unicode_extra_chain() {
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/unicode_extra_chain.zip").await.unwrap_err();
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
    let result = unzip("https://pub-c6f28d316acd406eae43501e51ad30fa.r2.dev/0723f54ceb33a4fdc7f2eddc19635cd704d61c84/malicious/zipinzip.zip").await.unwrap_err();
    insta::assert_debug_snapshot!(result, @"ZipInZip");
}
