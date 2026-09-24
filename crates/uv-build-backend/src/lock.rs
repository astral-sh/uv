use std::io;
use std::path::Path;

use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions};
use uv_lock::{Installable, Lock, PylockToml};
use uv_normalize::{DefaultExtras, PackageName};
use uv_preview::PreviewFeature;
use uv_static::{EnvVars, parse_boolish_environment_variable};

use crate::{Error, PyProjectToml};

/// Export runtime dependencies without the project itself, which is supplied by the wheel.
pub(crate) fn export_lock(root: &Path, pyproject: &PyProjectToml) -> Result<Option<String>, Error> {
    let enabled = parse_boolish_environment_variable(EnvVars::UV_BUILD_BACKEND_EXPORT_LOCK)?
        .or_else(|| {
            pyproject
                .settings()
                .and_then(|settings| settings.export_lock)
        });
    if enabled == Some(false) {
        return Ok(None);
    }
    if !uv_preview::is_enabled(PreviewFeature::LockedTools) {
        if enabled == Some(true) {
            return Err(Error::InvalidBuildLock(
                "Exporting locks requires the `locked-tools` preview feature".to_string(),
            ));
        }
        return Ok(None);
    }

    let contents = match fs_err::read_to_string(root.join("uv.lock")) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound && enabled.is_none() => return Ok(None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(Error::InvalidBuildLock(
                "Cannot export a lock: `uv.lock` was not found; run `uv lock` before building"
                    .to_string(),
            ));
        }
        Err(err) => return Err(err.into()),
    };
    let lock = Lock::from_toml(&contents)?;
    if enabled.is_none()
        && lock
            .packages()
            .iter()
            .any(|package| package.name() != pyproject.name() && !package.is_from_pypi_registry())
    {
        return Ok(None);
    }
    let package = lock
        .find_by_name(pyproject.name())
        .map_err(Error::InvalidBuildLock)?
        .ok_or_else(|| {
            Error::InvalidBuildLock(format!("`uv.lock` does not contain `{}`", pyproject.name()))
        })?;
    if package.version() != Some(pyproject.version()) {
        return Err(Error::InvalidBuildLock(format!(
            "`uv.lock` does not match version {} of `{}`; run `uv lock` before building",
            pyproject.version(),
            pyproject.name()
        )));
    }

    let target = BuildLock {
        root,
        lock: &lock,
        name: pyproject.name(),
    };
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let pylock = PylockToml::from_lock(
        &target,
        &[],
        &ExtrasSpecification::default().with_defaults(DefaultExtras::default()),
        &DependencyGroupsWithDefaults::none(),
        false,
        None,
        &install_options,
    )?;
    Ok(Some(pylock.to_toml()?))
}

struct BuildLock<'lock> {
    root: &'lock Path,
    lock: &'lock Lock,
    name: &'lock PackageName,
}

impl<'lock> Installable<'lock> for BuildLock<'lock> {
    fn install_path(&self) -> &'lock Path {
        self.root
    }

    fn lock(&self) -> &'lock Lock {
        self.lock
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::once(self.name)
    }

    fn project_name(&self) -> Option<&PackageName> {
        Some(self.name)
    }
}
