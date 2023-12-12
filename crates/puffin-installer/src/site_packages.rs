use std::collections::BTreeMap;

use anyhow::{Context, Result};
use fs_err as fs;

use distribution_types::{InstalledDist, Metadata, VersionOrUrl};
use pep440_rs::Version;
use pep508_rs::Requirement;
use puffin_interpreter::Virtualenv;
use puffin_normalize::PackageName;
use pypi_types::Metadata21;

#[derive(Debug)]
pub struct SitePackages<'a> {
    venv: &'a Virtualenv,
    index: BTreeMap<PackageName, InstalledDist>,
}

impl<'a> SitePackages<'a> {
    /// Build an index of installed packages from the given Python executable.
    pub fn try_from_executable(venv: &'a Virtualenv) -> Result<SitePackages<'a>> {
        let mut index = BTreeMap::new();

        for entry in fs::read_dir(venv.site_packages())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(dist_info) =
                    InstalledDist::try_from_path(&entry.path()).with_context(|| {
                        format!("Failed to read metadata: from {}", entry.path().display())
                    })?
                {
                    if let Some(existing) = index.insert(dist_info.name().clone(), dist_info) {
                        anyhow::bail!(
                            "Found duplicate package in environment: {} ({} vs. {})",
                            existing.name(),
                            existing.path().display(),
                            entry.path().display()
                        );
                    }
                }
            }
        }

        Ok(Self { venv, index })
    }

    /// Returns an iterator over the installed distributions.
    pub fn distributions(&self) -> impl Iterator<Item = &InstalledDist> {
        self.index.values()
    }

    /// Returns an iterator over the the installed distributions, represented as requirements.
    pub fn requirements(&self) -> impl Iterator<Item = pep508_rs::Requirement> + '_ {
        self.distributions().map(|dist| pep508_rs::Requirement {
            name: dist.name().clone(),
            extras: None,
            version_or_url: Some(match dist.version_or_url() {
                VersionOrUrl::Version(version) => {
                    pep508_rs::VersionOrUrl::VersionSpecifier(pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(version.clone()),
                    ))
                }
                VersionOrUrl::Url(url) => pep508_rs::VersionOrUrl::Url(url.clone()),
            }),
            marker: None,
        })
    }

    /// Returns the version of the given package, if it is installed.
    pub fn get(&self, name: &PackageName) -> Option<&InstalledDist> {
        self.index.get(name)
    }

    /// Remove the given package from the index, returning its version if it was installed.
    pub fn remove(&mut self, name: &PackageName) -> Option<InstalledDist> {
        self.index.remove(name)
    }

    /// Returns `true` if there are no installed packages.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Returns the number of installed packages.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        for (package, distribution) in &self.index {
            // Determine the dependencies for the given package.
            let path = distribution.path().join("METADATA");
            let contents = fs::read(&path)?;
            let metadata = Metadata21::parse(&contents)
                .with_context(|| format!("Failed to parse METADATA file at: {}", path.display()))?;

            // Verify that the dependencies are installed.
            for requirement in &metadata.requires_dist {
                if !requirement.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                    continue;
                }

                let Some(installed) = self.index.get(&requirement.name) else {
                    diagnostics.push(Diagnostic::MissingDependency {
                        package: package.clone(),
                        requirement: requirement.clone(),
                    });
                    continue;
                };

                match &requirement.version_or_url {
                    None | Some(pep508_rs::VersionOrUrl::Url(_)) => {
                        // Nothing to do (accept any installed version).
                    }
                    Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                        if !version_specifier.contains(installed.version()) {
                            diagnostics.push(Diagnostic::IncompatibleDependency {
                                package: package.clone(),
                                version: installed.version().clone(),
                                requirement: requirement.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}

impl IntoIterator for SitePackages<'_> {
    type Item = (PackageName, InstalledDist);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, InstalledDist>;

    fn into_iter(self) -> Self::IntoIter {
        self.index.into_iter()
    }
}

#[derive(Debug)]
pub enum Diagnostic {
    MissingDependency {
        /// The package that is missing a dependency.
        package: PackageName,
        /// The dependency that is missing.
        requirement: Requirement,
    },
    IncompatibleDependency {
        /// The package that has an incompatible dependency.
        package: PackageName,
        /// The version of the package that is installed.
        version: Version,
        /// The dependency that is incompatible.
        requirement: Requirement,
    },
}

impl Diagnostic {
    /// Convert the diagnostic into a user-facing message.
    pub fn message(&self) -> String {
        match self {
            Self::MissingDependency {
                package,
                requirement,
            } => {
                format!("The package `{package}` requires `{requirement}` but it is not installed.")
            }
            Self::IncompatibleDependency {
                package,
                version,
                requirement,
            } => format!(
                "The package `{package}` requires `{requirement}` but `{version}` is installed."
            ),
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    pub fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingDependency { package, .. } => name == package,
            Self::IncompatibleDependency {
                package,
                requirement,
                ..
            } => name == package || &requirement.name == name,
        }
    }
}
