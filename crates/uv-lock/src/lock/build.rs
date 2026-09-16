use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uv_configuration::{
    BuildKind, BuildOptions, DependencyGroupsWithDefaults, ExtrasSpecification, HashCheckingMode,
    InstallOptions,
};
use uv_distribution_types::{
    BuildLockFingerprint, Dist, Name, Requirement, Resolution, ResolvedDist, SourceDist,
};
use uv_normalize::{DefaultExtras, PackageName};
use uv_pep508::MarkerEnvironment;
use uv_pypi_types::{Digest, HashDigest};
use uv_python::Interpreter;
use uv_types::{HashStrategy, ResolvedRequirements};

use super::{Installable, Lock, LockError, LockErrorKind, PackageId, Source, VERSION};

/// A build operation whose isolated dependency environment can be locked.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildOperation {
    Wheel,
    Editable,
}

/// Which dependency environment a PEP 517 frontend is constructing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStage {
    Bootstrap,
    Final,
}

impl TryFrom<BuildKind> for BuildOperation {
    type Error = BuildLockError;

    fn try_from(kind: BuildKind) -> Result<Self, Self::Error> {
        match kind {
            BuildKind::Wheel => Ok(Self::Wheel),
            BuildKind::Editable => Ok(Self::Editable),
            BuildKind::Sdist => Err(BuildLockError::UnsupportedOperation(kind)),
        }
    }
}

impl Display for BuildOperation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => f.write_str("wheel"),
            Self::Editable => f.write_str("editable"),
        }
    }
}

/// The source identity known by the parent lock, independently of installation mode.
///
/// Non-registry source versions are metadata, not a stable identity available to every build
/// caller. Their URL, path, or precise Git source identifies the source instead.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(transparent)]
pub struct BuildSourceId(pub(super) PackageId);

impl BuildSourceId {
    pub fn name(&self) -> &PackageName {
        &self.0.name
    }

    pub fn from_source_dist(source: &SourceDist, root: &Path) -> Result<Self, LockError> {
        let version = match source {
            SourceDist::Registry(source) => Some(source.version.clone()),
            SourceDist::DirectUrl(_)
            | SourceDist::Path(_)
            | SourceDist::Directory(_)
            | SourceDist::GitPath(_)
            | SourceDist::GitDirectory(_) => None,
        };
        Ok(Self::normalize(PackageId {
            name: source.name().clone(),
            version,
            source: Source::from_source_dist(source, root)?,
        }))
    }

    pub(super) fn normalize(mut id: PackageId) -> Self {
        if !matches!(id.source, Source::Registry(_)) {
            id.version = None;
        }
        id.source = match id.source {
            Source::Editable(path) => Source::Directory(path),
            source @ (Source::Registry(_)
            | Source::Git(..)
            | Source::Direct(..)
            | Source::Path(_)
            | Source::Directory(_)
            | Source::Virtual(_)) => source,
        };
        Self(id)
    }
}

impl Display for BuildSourceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The observed marker environment and interpreter ABI used for backend discovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BuildExecutor {
    marker_environment: MarkerEnvironment,
    abi_tag: Option<String>,
}

impl BuildExecutor {
    pub fn from_interpreter(
        interpreter: &Interpreter,
    ) -> Result<Self, uv_platform_tags::TagsError> {
        Ok(Self {
            marker_environment: interpreter.markers().clone(),
            abi_tag: interpreter.tags()?.abi_tag().map(|tag| tag.to_string()),
        })
    }
}

/// The build declarations observed in a source tree. `Legacy` records an absent pyproject file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum BuildSourceInput {
    Legacy,
    Pyproject { hash: HashDigest },
}

impl BuildSourceInput {
    pub fn read(source: &Path) -> Result<Self, io::Error> {
        match fs_err::read(source.join("pyproject.toml")) {
            Ok(contents) => Ok(Self::Pyproject {
                hash: HashDigest::Sha256(Digest::from_bytes(Sha256::digest(contents).into())),
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::Legacy),
            Err(err) => Err(err),
        }
    }
}

/// Independently valid resolutions for one source and build operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LockedBuild {
    #[serde(flatten)]
    pub(super) source: BuildSourceId,
    pub(super) operation: BuildOperation,
    pub(super) input: BuildSourceInput,
    pub(super) declared_requirements: Vec<Requirement>,
    pub(super) backend_requirements: Vec<Requirement>,
    pub(super) bootstrap: Lock,
    #[serde(rename = "final")]
    pub(super) final_resolution: Option<Lock>,
}

impl LockedBuild {
    pub fn new(
        source: BuildSourceId,
        operation: BuildOperation,
        input: BuildSourceInput,
        mut declared_requirements: Vec<Requirement>,
        mut backend_requirements: Vec<Requirement>,
        bootstrap: Lock,
        final_resolution: Option<Lock>,
    ) -> Result<Self, BuildLockError> {
        declared_requirements.sort();
        declared_requirements.dedup();
        backend_requirements.sort();
        backend_requirements.dedup();
        let build = Self {
            source,
            operation,
            input,
            declared_requirements,
            backend_requirements,
            bootstrap,
            final_resolution,
        };
        build.validate()?;
        Ok(build)
    }

    fn validate(&self) -> Result<(), BuildLockError> {
        let declared = self
            .declared_requirements
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if self.bootstrap.requirements() != &declared {
            return Err(BuildLockError::Invalid(
                "bootstrap roots differ from declared requirements",
            ));
        }
        let final_requirements = declared
            .iter()
            .cloned()
            .chain(self.backend_requirements.iter().cloned())
            .collect::<BTreeSet<_>>();
        match &self.final_resolution {
            Some(resolution) if resolution.requirements() != &final_requirements => {
                return Err(BuildLockError::Invalid(
                    "final roots differ from declared and backend requirements",
                ));
            }
            None if final_requirements != declared => {
                return Err(BuildLockError::Invalid(
                    "backend requirements have no final resolution",
                ));
            }
            Some(_) | None => {}
        }
        for graph in std::iter::once(&self.bootstrap).chain(self.final_resolution.iter()) {
            if graph.version != VERSION || graph.builds.is_some() {
                return Err(BuildLockError::Invalid(
                    "a build graph must be an ordinary lock resolution",
                ));
            }
            if graph.packages.iter().any(|package| {
                package.sdist.is_some()
                    || package.wheels.len() != 1
                    || package.wheels.iter().any(|wheel| wheel.hash.is_none())
            }) {
                return Err(BuildLockError::Invalid(
                    "each build package must identify one hashed wheel",
                ));
            }
        }
        Ok(())
    }

    pub fn source(&self) -> &BuildSourceId {
        &self.source
    }

    pub fn operation(&self) -> BuildOperation {
        self.operation
    }

    pub fn input(&self) -> &BuildSourceInput {
        &self.input
    }

    pub fn declared_requirements(&self) -> &[Requirement] {
        &self.declared_requirements
    }

    pub fn backend_requirements(&self) -> &[Requirement] {
        &self.backend_requirements
    }

    pub fn has_final_resolution(&self) -> bool {
        self.final_resolution.is_some()
    }

    pub fn graph(&self, stage: BuildStage) -> Option<&Lock> {
        match stage {
            BuildStage::Bootstrap => Some(&self.bootstrap),
            BuildStage::Final => self.final_resolution.as_ref(),
        }
    }

    pub fn materialize(
        &self,
        stage: BuildStage,
        requirements: &[Requirement],
        root: &Path,
        interpreter: &Interpreter,
        build_options: &BuildOptions,
    ) -> Result<ResolvedRequirements, BuildLockError> {
        let graph = self.graph(stage).ok_or(BuildLockError::MissingStage)?;
        let requested = requirements
            .iter()
            .cloned()
            .map(|requirement| requirement.relative_to(root))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if graph.requirements() != &requested {
            return Err(BuildLockError::ChangedRequirements);
        }
        let resolution = BuildGraphTarget { lock: graph, root }.to_resolution(
            &interpreter.to_resolver_marker_environment(),
            interpreter.tags()?,
            &ExtrasSpecification::default().with_defaults(DefaultExtras::default()),
            &DependencyGroupsWithDefaults::none(),
            build_options,
            &InstallOptions::default(),
        )?;
        ensure_build_wheels(&resolution)?;
        let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Require)?;
        Ok(ResolvedRequirements::new(resolution, hasher))
    }
}

/// Mandatory build coverage for one observed executor.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LockedBuilds {
    pub(super) executor: BuildExecutor,
    #[serde(rename = "resolution")]
    pub(super) resolutions: Vec<LockedBuild>,
}

impl LockedBuilds {
    pub fn resolutions(&self) -> &[LockedBuild] {
        &self.resolutions
    }

    pub fn new(
        executor: BuildExecutor,
        resolutions: Vec<LockedBuild>,
    ) -> Result<Self, BuildLockError> {
        let mut builds = Self {
            executor,
            resolutions,
        };
        builds.canonicalize();
        builds.validate()?;
        Ok(builds)
    }

    pub(super) fn canonicalize(&mut self) {
        for build in &mut self.resolutions {
            build.declared_requirements.sort();
            build.declared_requirements.dedup();
            build.backend_requirements.sort();
            build.backend_requirements.dedup();
        }
        self.resolutions.sort_by(|left, right| {
            (&left.source, left.operation).cmp(&(&right.source, right.operation))
        });
    }

    pub(super) fn validate(&self) -> Result<(), BuildLockError> {
        let mut seen = BTreeSet::new();
        for resolution in &self.resolutions {
            resolution.validate()?;
            if !seen.insert((&resolution.source, resolution.operation)) {
                return Err(BuildLockError::Invalid(
                    "duplicate source and operation coverage",
                ));
            }
        }
        Ok(())
    }

    pub fn get(
        &self,
        source: &BuildSourceId,
        operation: BuildOperation,
        executor: &BuildExecutor,
    ) -> Result<&LockedBuild, BuildLockError> {
        self.validate_executor(executor)?;
        self.resolutions
            .iter()
            .find(|resolution| &resolution.source == source && resolution.operation == operation)
            .ok_or_else(|| BuildLockError::UncoveredSource {
                package: Box::new(source.clone()),
                operation,
            })
    }

    pub fn validate_executor(&self, executor: &BuildExecutor) -> Result<(), BuildLockError> {
        if executor != &self.executor {
            return Err(BuildLockError::UncoveredExecutor);
        }
        Ok(())
    }

    pub fn validate_source(
        &self,
        source: &SourceDist,
        root: &Path,
        executor: &BuildExecutor,
    ) -> Result<(), BuildLockError> {
        let id = BuildSourceId::from_source_dist(source, root)?;
        let operation = if source.is_editable() {
            BuildOperation::Editable
        } else {
            BuildOperation::Wheel
        };
        let locked = self.get(&id, operation, executor)?;
        if let SourceDist::Directory(source) = source
            && BuildSourceInput::read(&source.install_path)? != locked.input
        {
            return Err(BuildLockError::ChangedSourceInput(Box::new(id)));
        }
        Ok(())
    }

    /// Check selected source builds before installed-state or built-wheel cache shortcuts.
    pub fn validate_resolution(
        &self,
        resolution: &Resolution,
        root: &Path,
        interpreter: &Interpreter,
    ) -> Result<(), BuildLockError> {
        let executor = BuildExecutor::from_interpreter(interpreter)?;
        for distribution in resolution.distributions() {
            let ResolvedDist::Installable { dist, .. } = distribution else {
                return Err(BuildLockError::Invalid(
                    "a locked project selection contains an installed candidate",
                ));
            };
            let Dist::Source(source) = dist.as_ref() else {
                continue;
            };
            self.validate_source(source, root, &executor)?;
        }
        Ok(())
    }

    /// Validate the source coverage required by any supported selection from the runtime lock.
    pub fn validate_lock_sources(
        &self,
        lock: &Lock,
        root: &Path,
        interpreter: &Interpreter,
        build_options: &BuildOptions,
    ) -> Result<(), BuildLockError> {
        let executor = BuildExecutor::from_interpreter(interpreter)?;
        self.validate_executor(&executor)?;
        for (source, _) in lock.build_sources(
            root,
            interpreter.tags()?,
            interpreter.markers(),
            build_options,
        )? {
            self.validate_source(&source, root, &executor)?;
            if let SourceDist::Directory(mut directory) = source {
                directory.editable = Some(!directory.editable.unwrap_or(false));
                self.validate_source(&SourceDist::Directory(directory), root, &executor)?;
            }
        }
        Ok(())
    }

    /// Return a content-derived identity for caches and installed-build provenance.
    pub fn fingerprint(&self) -> Result<BuildLockFingerprint, toml_edit::ser::Error> {
        let contents = super::serialize::build_lock_to_toml(self)?;
        Ok(BuildLockFingerprint::new(HashDigest::Sha256(
            Digest::from_bytes(Sha256::digest(contents).into()),
        )))
    }
}

pub fn ensure_build_wheels(resolution: &Resolution) -> Result<(), BuildLockError> {
    for distribution in resolution.distributions() {
        match distribution {
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(_) => {}
                Dist::Source(_) => {
                    return Err(BuildLockError::NestedSource(distribution.to_string()));
                }
            },
            ResolvedDist::Installed { .. } => {
                return Err(BuildLockError::Invalid(
                    "an installed package cannot identify a locked build artifact",
                ));
            }
        }
    }
    Ok(())
}

struct BuildGraphTarget<'lock> {
    lock: &'lock Lock,
    root: &'lock Path,
}

impl<'lock> Installable<'lock> for BuildGraphTarget<'lock> {
    fn install_path(&self) -> &'lock Path {
        self.root
    }
    fn lock(&self) -> &'lock Lock {
        self.lock
    }
    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::empty()
    }
    fn project_name(&self) -> Option<&PackageName> {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildLockError {
    #[error("Invalid build lock: {0}")]
    Invalid(&'static str),
    #[error("The build lock does not cover this executor")]
    UncoveredExecutor,
    #[error("The build lock does not cover the {operation} build of `{package}`")]
    UncoveredSource {
        package: Box<BuildSourceId>,
        operation: BuildOperation,
    },
    #[error("The build lock has no coverage for this stage")]
    MissingStage,
    #[error("The requested build requirements differ from the locked graph")]
    ChangedRequirements,
    #[error("The build declarations for `{0}` changed; update the build lock")]
    ChangedSourceInput(Box<BuildSourceId>),
    #[error("Nested source builds are not supported by build dependency locking: {0}")]
    NestedSource(String),
    #[error("Build dependency locking does not support {0} builds")]
    UnsupportedOperation(BuildKind),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Hash(#[from] uv_types::HashStrategyError),
    #[error(transparent)]
    Tags(#[from] uv_platform_tags::TagsError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<BuildLockError> for LockError {
    fn from(error: BuildLockError) -> Self {
        LockErrorKind::InvalidBuildLock(error.to_string()).into()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use uv_pep508::MarkerEnvironmentBuilder;
    use uv_pypi_types::VerbatimParsedUrl;

    use super::*;

    fn executor() -> Result<BuildExecutor, Box<dyn Error>> {
        Ok(BuildExecutor {
            marker_environment: MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
                implementation_name: "cpython",
                implementation_version: "3.12.0",
                os_name: "posix",
                platform_machine: "x86_64",
                platform_python_implementation: "CPython",
                platform_release: "test",
                platform_system: "Linux",
                platform_version: "test",
                python_full_version: "3.12.0",
                python_version: "3.12",
                sys_platform: "linux",
            })?,
            abi_tag: Some("cp312".to_owned()),
        })
    }

    fn runtime() -> Result<Lock, Box<dyn Error>> {
        Ok(Lock::from_toml(
            r#"
version = 1
revision = 3
requires-python = ">=3.12"

[[package]]
name = "first"
version = "1.0.0"
source = { directory = "first" }

[[package]]
name = "second"
version = "1.0.0"
source = { directory = "second" }
"#,
        )?)
    }

    fn graph(helper: &str) -> Result<Lock, Box<dyn Error>> {
        Ok(Lock::from_toml(&format!(
            r#"
version = 1
revision = 3
requires-python = ">=3.12"

[manifest]
requirements = [{{ name = "builder", specifier = "==1.0.0" }}]

[[package]]
name = "builder"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}
dependencies = [{{ name = "helper" }}]
wheels = [{{ url = "https://example.com/builder-1.0.0-py3-none-any.whl", hash = "sha256:{}" }}]

[[package]]
name = "helper"
version = "{helper}"
source = {{ registry = "https://example.com/simple" }}
wheels = [{{ url = "https://example.com/helper-{helper}-py3-none-any.whl", hash = "sha256:{}" }}]
"#,
            "0".repeat(64),
            "1".repeat(64)
        ))?)
    }

    fn captured(source: BuildSourceId, helper: &str) -> Result<LockedBuild, Box<dyn Error>> {
        Ok(LockedBuild::new(
            source,
            BuildOperation::Wheel,
            BuildSourceInput::Legacy,
            vec![Requirement::from(uv_pep508::Requirement::<
                VerbatimParsedUrl,
            >::from_str("builder==1.0.0")?)],
            vec![],
            graph(helper)?,
            None,
        )?)
    }

    #[test]
    fn independent_graphs_roundtrip() -> Result<(), Box<dyn Error>> {
        let runtime = runtime()?;
        let ordinary = runtime.to_toml()?;
        let builds = LockedBuilds::new(
            executor()?,
            vec![
                captured(
                    BuildSourceId::normalize(runtime.packages[0].id.clone()),
                    "1.0.0",
                )?,
                captured(
                    BuildSourceId::normalize(runtime.packages[1].id.clone()),
                    "2.0.0",
                )?,
            ],
        )?;
        let lock = runtime.with_build_lock(builds)?;
        let serialized = lock.to_toml()?;
        assert_eq!(Lock::from_toml(&serialized)?, lock);
        assert_eq!(lock.clone().without_build_lock().to_toml()?, ordinary);
        insta::assert_snapshot!(serialized.lines().filter(|line| line.starts_with("version =") || line.starts_with("[build-lock") || line.starts_with("[[build-lock")).collect::<Vec<_>>().join("\n"), @r#"
        version = 2
        version = "1.0.0"
        version = "1.0.0"
        [build-lock]
        [[build-lock.resolution]]
        [build-lock.resolution.bootstrap]
        version = 1
        [build-lock.resolution.bootstrap.manifest]
        [[build-lock.resolution.bootstrap.package]]
        version = "1.0.0"
        [[build-lock.resolution.bootstrap.package]]
        version = "1.0.0"
        [[build-lock.resolution]]
        [build-lock.resolution.bootstrap]
        version = 1
        [build-lock.resolution.bootstrap.manifest]
        [[build-lock.resolution.bootstrap.package]]
        version = "1.0.0"
        [[build-lock.resolution.bootstrap.package]]
        version = "2.0.0"
        "#);
        Ok(())
    }

    #[test]
    fn build_reachability_preserves_markers_and_conflicts() -> Result<(), Box<dyn Error>> {
        let lock = Lock::from_toml(
            r#"
version = 1
revision = 3
requires-python = ">=3.12"
conflicts = [[{ package = "project", extra = "left" }, { package = "project", extra = "right" }]]

[[package]]
name = "project"
version = "1.0.0"
source = { virtual = "." }
dependencies = [{ name = "shared", extra = ["windows"], marker = "sys_platform == 'win32'" }]
[package.optional-dependencies]
left = [{ name = "shared", extra = ["linux"] }]
right = [{ name = "right" }]

[[package]]
name = "shared"
version = "1.0.0"
source = { directory = "shared" }
[package.optional-dependencies]
windows = [{ name = "windows" }]
linux = [{ name = "linux" }, { name = "impossible", marker = "extra == 'extra-7-project-right'" }]

[[package]]
name = "windows"
version = "1.0.0"
source = { directory = "windows" }

[[package]]
name = "linux"
version = "1.0.0"
source = { directory = "linux" }

[[package]]
name = "right"
version = "1.0.0"
source = { directory = "right" }

[[package]]
name = "impossible"
version = "1.0.0"
source = { directory = "impossible" }
"#,
        )?;
        let reached = lock.build_reachability(&executor()?.marker_environment);
        let mut names = reached
            .into_iter()
            .map(|index| lock.package(index).name().to_string())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["linux", "project", "right", "shared"]);
        Ok(())
    }

    #[test]
    fn build_contract_requires_a_version_fence() -> Result<(), Box<dyn Error>> {
        let lock = runtime()?.with_build_lock(LockedBuilds::new(executor()?, vec![])?)?;
        let serialized = lock.to_toml()?;
        assert_eq!(Lock::from_toml(&serialized)?, lock);
        assert_eq!(Lock::from_toml_if_build_locked(&serialized)?, Some(lock));
        let unfenced = serialized.replacen("version = 2", "version = 1", 1);
        assert!(Lock::from_toml(&unfenced).is_err());
        assert!(Lock::from_toml_if_build_locked(&unfenced).is_err());
        let missing = "version = 2\nrequires-python = \">=3.12\"\n";
        assert!(Lock::from_toml(missing).is_err());
        assert!(Lock::from_toml_if_build_locked(missing).is_err());
        assert!(Lock::from_toml_if_build_locked("version = 3\n").is_err());
        assert_eq!(
            Lock::from_toml_if_build_locked("version = 1\npackage = 'not a runtime lock'\n")?,
            None
        );
        Ok(())
    }
}
