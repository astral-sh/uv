use std::str::FromStr;

use anyhow::Result;
use url::Url;

use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::VerbatimUrl;
use uv_pypi_types::ParsedUrl;
use uv_pypi_types::RequirementSource;

use super::{from_source, Locals};

#[test]
fn extract_locals() -> Result<()> {
    // Extract from a source distribution in a URL.
    let url = VerbatimUrl::from_url(Url::parse("https://example.com/foo-1.0.0+local.tar.gz")?);
    let source =
        RequirementSource::from_parsed_url(ParsedUrl::try_from(url.to_url()).unwrap(), url);
    let locals: Vec<_> = from_source(&source).into_iter().collect();
    assert_eq!(locals, vec![Version::from_str("1.0.0+local")?]);

    // Extract from a wheel in a URL.
    let url = VerbatimUrl::from_url(Url::parse(
        "https://example.com/foo-1.0.0+local-cp39-cp39-linux_x86_64.whl",
    )?);
    let source =
        RequirementSource::from_parsed_url(ParsedUrl::try_from(url.to_url()).unwrap(), url);
    let locals: Vec<_> = from_source(&source).into_iter().collect();
    assert_eq!(locals, vec![Version::from_str("1.0.0+local")?]);

    // Don't extract anything if the URL is opaque.
    let url = VerbatimUrl::from_url(Url::parse("git+https://example.com/foo/bar")?);
    let source =
        RequirementSource::from_parsed_url(ParsedUrl::try_from(url.to_url()).unwrap(), url);
    let locals: Vec<_> = from_source(&source).into_iter().collect();
    assert!(locals.is_empty());

    // Extract from `==` specifiers.
    let version = VersionSpecifiers::from_iter([
        VersionSpecifier::from_version(Operator::GreaterThan, Version::from_str("1.0.0")?)?,
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+local")?)?,
    ]);
    let source = RequirementSource::Registry {
        specifier: version,
        index: None,
    };
    let locals: Vec<_> = from_source(&source).into_iter().collect();
    assert_eq!(locals, vec![Version::from_str("1.0.0+local")?]);

    // Ignore other specifiers.
    let version = VersionSpecifiers::from_iter([VersionSpecifier::from_version(
        Operator::NotEqual,
        Version::from_str("1.0.0+local")?,
    )?]);
    let source = RequirementSource::Registry {
        specifier: version,
        index: None,
    };
    let locals: Vec<_> = from_source(&source).into_iter().collect();
    assert!(locals.is_empty());

    Ok(())
}

#[test]
fn map_version() -> Result<()> {
    // Given `==1.0.0`, if the local version is `1.0.0+local`, map to `==1.0.0+local`.
    let local = Version::from_str("1.0.0+local")?;
    let specifier = VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+local")?)?
    );

    // Given `!=1.0.0`, if the local version is `1.0.0+local`, map to `!=1.0.0+local`.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::NotEqual, Version::from_str("1.0.0")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::NotEqual, Version::from_str("1.0.0+local")?)?
    );

    // Given `<=1.0.0`, if the local version is `1.0.0+local`, map to `==1.0.0+local`.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::LessThanEqual, Version::from_str("1.0.0")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+local")?)?
    );

    // Given `>1.0.0`, `1.0.0+local` is already (correctly) disallowed.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::GreaterThan, Version::from_str("1.0.0")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::GreaterThan, Version::from_str("1.0.0")?)?
    );

    // Given `===1.0.0`, `1.0.0+local` is already (correctly) disallowed.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::ExactEqual, Version::from_str("1.0.0")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::ExactEqual, Version::from_str("1.0.0")?)?
    );

    // Given `==1.0.0+local`, `1.0.0+local` is already (correctly) allowed.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+local")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+local")?)?
    );

    // Given `==1.0.0+other`, `1.0.0+local` is already (correctly) disallowed.
    let local = Version::from_str("1.0.0+local")?;
    let specifier =
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+other")?)?;
    assert_eq!(
        Locals::map(&local, &specifier)?,
        VersionSpecifier::from_version(Operator::Equal, Version::from_str("1.0.0+other")?)?
    );

    Ok(())
}
