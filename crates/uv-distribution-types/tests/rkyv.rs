use uv_distribution_types::{ArchivedFileLocation, FileLocation, FileLocationBuilder};
use uv_small_str::SmallString;

#[test]
fn archives_shared_relative_url_bases_once() -> Result<(), Box<dyn std::error::Error>> {
    let mut locations =
        FileLocationBuilder::new(SmallString::from("https://example.com/simple/package/"));
    let locations = vec![
        locations.parse(SmallString::from("one.whl")),
        locations.parse(SmallString::from("two.whl")),
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

#[test]
fn archives_shared_absolute_url_bases_once() -> Result<(), Box<dyn std::error::Error>> {
    let mut locations =
        FileLocationBuilder::new(SmallString::from("https://example.com/simple/package/"));
    let locations = vec![
        locations.parse(SmallString::from(
            "https://files.pythonhosted.org/packages/one.whl?query#fragment",
        )),
        locations.parse(SmallString::from(
            "https://files.pythonhosted.org/packages/two.whl",
        )),
    ];

    assert_eq!(
        locations[0].to_string(),
        "https://files.pythonhosted.org/packages/one.whl?query#fragment"
    );
    assert_eq!(
        locations[0].to_url()?.as_str(),
        "https://files.pythonhosted.org/packages/one.whl?query#fragment"
    );

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&locations)?;
    let archived = rkyv::access::<rkyv::Archived<Vec<FileLocation>>, rkyv::rancor::Error>(&bytes)?;

    match (&archived[0], &archived[1]) {
        (
            ArchivedFileLocation::AbsoluteUrl(first_base, _),
            ArchivedFileLocation::AbsoluteUrl(second_base, _),
        ) => assert!(std::ptr::eq(first_base.get(), second_base.get())),
        _ => return Err(std::io::Error::other("expected absolute URLs").into()),
    }

    assert_eq!(
        rkyv::from_bytes::<Vec<FileLocation>, rkyv::rancor::Error>(&bytes)?,
        locations
    );

    Ok(())
}
