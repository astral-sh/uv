use std::borrow::Cow;
use std::fmt::{Display, Formatter};

use uv_normalize::{DistInfoName, PackageName};

use crate::Error;

/// A `.dist-info` directory name whose normalized form starts with the expected package name.
///
/// Stores the original name, without the `.dist-info` suffix, so it can be used to locate files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDistInfoName<'a>(Cow<'a, str>);

impl<'a> ValidatedDistInfoName<'a> {
    /// Validate a `.dist-info` directory name against its package name.
    ///
    /// Like `pip`, only require the normalized directory name to start with the canonical package
    /// name. Some wheels use names that do not follow the current name and version rules.
    pub fn new(name: impl Into<Cow<'a, str>>, package_name: &PackageName) -> Result<Self, Error> {
        let name = name.into();
        if !DistInfoName::new(&name)
            .as_ref()
            .starts_with(package_name.as_str())
        {
            return Err(Error::MissingDistInfoPackageName(
                name.into_owned(),
                package_name.to_string(),
            ));
        }
        Ok(Self(name))
    }

    /// Return the original directory name, without the `.dist-info` suffix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ValidatedDistInfoName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
