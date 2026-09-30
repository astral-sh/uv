use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use uv_platform::Arch;

/// The architecture to use when a Python request does not specify one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonArchitecture(Arch);

impl PythonArchitecture {
    pub fn into_inner(self) -> Arch {
        self.0
    }
}

impl FromStr for PythonArchitecture {
    type Err = uv_platform::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl Display for PythonArchitecture {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
