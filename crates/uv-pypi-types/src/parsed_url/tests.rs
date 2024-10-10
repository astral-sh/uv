use anyhow::Result;
use url::Url;

use crate::parsed_url::ParsedUrl;

#[test]
fn direct_url_from_url() -> Result<()> {
    let expected = Url::parse("git+https://github.com/pallets/flask.git")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_eq!(expected, actual);

    let expected = Url::parse("git+https://github.com/pallets/flask.git#subdirectory=pkg_dir")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_eq!(expected, actual);

    let expected = Url::parse("git+https://github.com/pallets/flask.git@2.0.0")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_eq!(expected, actual);

    let expected =
        Url::parse("git+https://github.com/pallets/flask.git@2.0.0#subdirectory=pkg_dir")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_eq!(expected, actual);

    // TODO(charlie): Preserve other fragments.
    let expected =
        Url::parse("git+https://github.com/pallets/flask.git#egg=flask&subdirectory=pkg_dir")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_ne!(expected, actual);

    Ok(())
}

#[test]
#[cfg(unix)]
fn direct_url_from_url_absolute() -> Result<()> {
    let expected = Url::parse("file:///path/to/directory")?;
    let actual = Url::from(ParsedUrl::try_from(expected.clone())?);
    assert_eq!(expected, actual);
    Ok(())
}
