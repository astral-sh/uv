use std::ffi::CStr;

use crate::Error;

/// A borrowed, nonempty Mach-O install name without interior NUL bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallName<'a>(&'a CStr);

impl<'a> InstallName<'a> {
    /// Validate a C string without requiring UTF-8.
    pub fn new(name: &'a CStr) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::InvalidName);
        }

        Ok(Self(name))
    }

    pub(crate) fn as_c_str(self) -> &'a CStr {
        self.0
    }
}

/// A borrowed, nonempty code-signing identifier without interior NUL bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigningIdentifier<'a>(&'a CStr);

impl<'a> SigningIdentifier<'a> {
    /// Validate a C string without requiring UTF-8.
    pub fn new(identifier: &'a CStr) -> Result<Self, Error> {
        if identifier.is_empty() {
            return Err(Error::InvalidIdentifier);
        }

        Ok(Self(identifier))
    }

    pub(crate) fn as_c_str(self) -> &'a CStr {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SigningIdentifier;
    use crate::Error;

    #[test]
    fn invalid_identifier() {
        assert_eq!(SigningIdentifier::new(c""), Err(Error::InvalidIdentifier));
    }
}

#[cfg(test)]
mod tests {
    use super::InstallName;
    use crate::Error;

    #[test]
    fn invalid_name() {
        assert_eq!(InstallName::new(c""), Err(Error::InvalidName));
    }
}
