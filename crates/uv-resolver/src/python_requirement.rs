use distribution_types::{CompatibleDist, Dist};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::MarkerEnvironment;
use uv_interpreter::Interpreter;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PythonRequirement {
    /// The installed version of Python.
    installed: Version,
    /// The target version of Python; that is, the version of Python for which we are resolving
    /// dependencies. This is typically the same as the installed version, but may be different
    /// when specifying an alternate Python version for the resolution.
    target: Version,
}

impl PythonRequirement {
    pub fn new(interpreter: &Interpreter, markers: &MarkerEnvironment) -> Self {
        Self {
            installed: interpreter.python_version().clone(),
            target: markers.python_full_version.version.clone(),
        }
    }

    /// Return the installed version of Python.
    pub(crate) fn installed(&self) -> &Version {
        &self.installed
    }

    /// Return the target version of Python.
    pub(crate) fn target(&self) -> &Version {
        &self.target
    }

    /// If the dist doesn't match the given Python requirement, return the version specifiers.
    pub(crate) fn validate_dist<'a>(
        &self,
        dist: &'a CompatibleDist,
    ) -> Option<&'a VersionSpecifiers> {
        // Validate the _installed_ file.
        let requires_python = dist.for_installation().requires_python.as_ref()?;

        // If the dist doesn't support the target Python version, return the failing version
        // specifiers.
        if !requires_python.contains(self.target()) {
            return Some(requires_python);
        }

        // If the dist is a source distribution, and doesn't support the installed Python
        // version, return the failing version specifiers, since we won't be able to build it.
        if matches!(dist.for_installation().dist, Dist::Source(_)) {
            if !requires_python.contains(self.installed()) {
                return Some(requires_python);
            }
        }

        // Validate the resolved file.
        let requires_python = dist.for_resolution().requires_python.as_ref()?;

        // If the dist is a source distribution, and doesn't support the installed Python
        // version, return the failing version specifiers, since we won't be able to build it.
        // This isn't strictly necessary, since if `dist.resolve_metadata()` is a source distribution, it
        // should be the same file as `dist.install_metadata()` (validated above).
        if matches!(dist.for_resolution().dist, Dist::Source(_)) {
            if !requires_python.contains(self.installed()) {
                return Some(requires_python);
            }
        }

        None
    }
}
