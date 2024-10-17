use std::str::FromStr;

use anyhow::Error;

use super::*;

#[test]
fn no_build_from_args() -> Result<(), Error> {
    assert_eq!(
        NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":all:")?], false),
        NoBuild::All,
    );
    assert_eq!(
        NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":all:")?], true),
        NoBuild::All,
    );
    assert_eq!(
        NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":none:")?], true),
        NoBuild::All,
    );
    assert_eq!(
        NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":none:")?], false),
        NoBuild::None,
    );
    assert_eq!(
        NoBuild::from_pip_args(
            vec![
                PackageNameSpecifier::from_str("foo")?,
                PackageNameSpecifier::from_str("bar")?
            ],
            false
        ),
        NoBuild::Packages(vec![
            PackageName::from_str("foo")?,
            PackageName::from_str("bar")?
        ]),
    );
    assert_eq!(
        NoBuild::from_pip_args(
            vec![
                PackageNameSpecifier::from_str("test")?,
                PackageNameSpecifier::All
            ],
            false
        ),
        NoBuild::All,
    );
    assert_eq!(
        NoBuild::from_pip_args(
            vec![
                PackageNameSpecifier::from_str("foo")?,
                PackageNameSpecifier::from_str(":none:")?,
                PackageNameSpecifier::from_str("bar")?
            ],
            false
        ),
        NoBuild::Packages(vec![PackageName::from_str("bar")?]),
    );

    Ok(())
}
