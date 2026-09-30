use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use uv_cache::Cache;
use uv_platform::Arch;

use crate::{Interpreter, PythonInstallation, PythonPreference, PythonRequest};

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

/// Preferences used when selecting a Python interpreter.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PythonPreferences {
    pub source: PythonPreference,
    pub arch: Option<PythonArchitecture>,
}

impl PythonPreferences {
    #[must_use]
    pub fn with_source(self, source: PythonPreference) -> Self {
        Self { source, ..self }
    }

    #[must_use]
    pub fn with_system_flag(self, system: bool) -> Self {
        self.with_source(self.source.with_system_flag(system))
    }

    pub fn allows_installation(self, installation: &PythonInstallation) -> bool {
        self.source.allows_installation(installation)
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

impl From<PythonPreference> for PythonPreferences {
    fn from(source: PythonPreference) -> Self {
        Self { source, arch: None }
    }
}

impl Display for PythonPreferences {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}
