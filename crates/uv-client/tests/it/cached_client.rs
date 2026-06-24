use uv_client::{DataWithCachePolicy, ErrorKind};

#[test]
fn reject_overflowing_cache_policy_length() {
    let error = DataWithCachePolicy::from_reader(&[u8::MAX; 8][..]).unwrap_err();

    assert!(matches!(error.kind(), ErrorKind::ArchiveRead(_)));
}
