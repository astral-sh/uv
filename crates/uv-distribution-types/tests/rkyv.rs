use uv_distribution_types::{ArchivedFileLocation, FileLocation};
use uv_small_str::SmallString;

#[test]
fn archives_shared_relative_url_bases_once() -> Result<(), Box<dyn std::error::Error>> {
    let base = SmallString::from("https://example.com/simple/package/");
    let locations = vec![
        FileLocation::new(SmallString::from("one.whl"), &base),
        FileLocation::new(SmallString::from("two.whl"), &base),
    ];

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&locations)?;
    let archived = rkyv::access::<rkyv::Archived<Vec<FileLocation>>, rkyv::rancor::Error>(&bytes)?;

    match (&archived[0], &archived[1]) {
        (
            ArchivedFileLocation::RelativeUrl(first_base, _),
            ArchivedFileLocation::RelativeUrl(second_base, _),
        ) => assert!(std::ptr::eq(first_base.get(), second_base.get())),
        _ => return Err(std::io::Error::other("expected relative URLs").into()),
    }

    assert_eq!(
        rkyv::from_bytes::<Vec<FileLocation>, rkyv::rancor::Error>(&bytes)?,
        locations
    );

    Ok(())
}
