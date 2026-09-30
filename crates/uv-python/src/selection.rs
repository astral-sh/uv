use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use uv_cache::Cache;
use uv_platform::Arch;

use crate::{Interpreter, PythonPreference, PythonRequest};

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

/// Options used when selecting a Python interpreter.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PythonSelection {
    /// Whether to prefer managed or system Python installations.
    pub preference: PythonPreference,
    /// Require this architecture when the Python request does not specify one.
    pub arch: Option<PythonArchitecture>,
}

impl PythonSelection {
    #[must_use]
    pub fn with_preference(self, preference: PythonPreference) -> Self {
        Self { preference, ..self }
    }

    #[must_use]
    pub fn with_system_flag(self, system: bool) -> Self {
        self.with_preference(self.preference.with_system_flag(system))
    }

    /// Apply the default architecture without overriding an explicit interpreter request.
    pub fn apply_to_request(self, request: &PythonRequest) -> Cow<'_, PythonRequest> {
        request.with_arch_if_unspecified(self.arch.map(PythonArchitecture::into_inner))
    }

    /// Check whether an interpreter satisfies a request and its default architecture.
    pub fn satisfies_request(
        self,
        request: Option<&PythonRequest>,
        interpreter: &Interpreter,
        cache: &Cache,
    ) -> bool {
        self.apply_to_request(request.unwrap_or(&PythonRequest::Any))
            .satisfied(interpreter, cache)
    }
}

impl From<PythonPreference> for PythonSelection {
    fn from(preference: PythonPreference) -> Self {
        Self {
            preference,
            arch: None,
        }
    }
}
