use std::ffi::{CStr, CString};

use crate::Error;

/// A nonempty Mach-O install name without interior NUL bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallName(CString);

impl InstallName {
    /// Validate a name, accepting non-UTF-8 Unix paths.
    pub fn new(name: &[u8]) -> Result<Self, Error> {
        let name = CString::new(name).map_err(|_| Error::InvalidName)?;
        if name.is_empty() {
            return Err(Error::InvalidName);
        }

        Ok(Self(name))
    }

    pub(crate) fn as_c_str(&self) -> &CStr {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::InstallName;
    use crate::Error;

    #[test]
    fn invalid_names() {
        for name in [b"".as_slice(), b"invalid\0name"] {
            assert_eq!(InstallName::new(name), Err(Error::InvalidName));
        }
    }
}
