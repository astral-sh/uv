use itertools::Either;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_parser::Source;
use toml_parser::lexer::TokenKind;
use tracing::info_span;

use uv_auth::CredentialsCache;
use uv_cache::Cache;
use uv_configuration::{DependencyGroupsWithDefaults, ExcludeDependency, NoSources};
use uv_distribution::LoweredRequirement;
use uv_distribution_types::{Index, IndexLocations, Requirement, RequiresPython};
use uv_normalize::{GroupName, PackageName};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::{Conflicts, SupportedEnvironments, VerbatimParsedUrl};
use uv_resolver::{Lock, LockVersion, VERSION};
use uv_scripts::Pep723Script;
use uv_workspace::dependency_groups::{DependencyGroupError, FlatDependencyGroup};
use uv_workspace::pyproject::OverrideDependency;
use uv_workspace::{Editability, Workspace, WorkspaceCache, WorkspaceMember};

use crate::commands::project::{ProjectError, find_requires_python};

/// A target that can be resolved into a lockfile.
#[derive(Debug, Copy, Clone)]
pub(crate) enum LockTarget<'lock> {
    Workspace(&'lock Workspace),
    Script(&'lock Pep723Script),
}

impl<'lock> From<&'lock Workspace> for LockTarget<'lock> {
    fn from(workspace: &'lock Workspace) -> Self {
        Self::Workspace(workspace)
    }
}

impl<'lock> From<&'lock Pep723Script> for LockTarget<'lock> {
    fn from(script: &'lock Pep723Script) -> Self {
        LockTarget::Script(script)
    }
}

impl<'lock> LockTarget<'lock> {
    /// Return the set of requirements that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn requirements(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.requirements(),
            Self::Script(script) => script.metadata.dependencies.clone().unwrap_or_default(),
        }
    }

    /// Returns the set of overrides for the [`LockTarget`].
    pub(crate) fn overrides(self) -> Vec<OverrideDependency> {
        match self {
            Self::Workspace(workspace) => workspace.overrides(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.override_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of dependency exclusions for the [`LockTarget`].
    pub(crate) fn exclude_dependencies(self) -> Vec<ExcludeDependency> {
        match self {
            Self::Workspace(workspace) => workspace.exclude_dependencies(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.exclude_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of constraints for the [`LockTarget`].
    pub(crate) fn constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of build constraints for the [`LockTarget`].
    pub(crate) fn build_constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.build_constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.build_constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Return the dependency groups that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn dependency_groups(
        self,
    ) -> Result<BTreeMap<GroupName, FlatDependencyGroup>, DependencyGroupError> {
        match self {
            Self::Workspace(workspace) => workspace.workspace_dependency_groups(),
            Self::Script(_) => Ok(BTreeMap::new()),
        }
    }

    /// Returns the set of all members within the target.
    pub(crate) fn members_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.members_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Returns the set of all dependency groups within the target.
    pub(crate) fn group_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.group_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Return the list of members to include in the [`Lock`].
    pub(crate) fn members(self) -> Vec<PackageName> {
        match self {
            Self::Workspace(workspace) => {
                let mut members = workspace.packages().keys().cloned().collect::<Vec<_>>();
                members.sort();

                // If this is a non-virtual project with a single member, we can omit it from the lockfile.
                // If any members are added or removed, it will inherently mismatch. If the member is
                // renamed, it will also mismatch.
                if members.len() == 1 && !workspace.is_non_project() {
                    members.clear();
                }

                members
            }
            Self::Script(_) => Vec::new(),
        }
    }

    /// Return the list of packages.
    pub(crate) fn packages(self) -> &'lock BTreeMap<PackageName, WorkspaceMember> {
        match self {
            Self::Workspace(workspace) => workspace.packages(),
            Self::Script(_) => {
                static EMPTY: BTreeMap<PackageName, WorkspaceMember> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Return the set of required workspace members, i.e., those that are required by other
    /// members.
    pub(crate) fn required_members(self) -> &'lock BTreeMap<PackageName, Editability> {
        match self {
            Self::Workspace(workspace) => workspace.required_members(),
            Self::Script(_) => {
                static EMPTY: BTreeMap<PackageName, Editability> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Returns the set of supported environments for the [`LockTarget`].
    pub(crate) fn environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of required platforms for the [`LockTarget`].
    pub(crate) fn required_environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.required_environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of conflicts for the [`LockTarget`].
    pub(crate) fn conflicts(self) -> Result<Conflicts, ProjectError> {
        match self {
            Self::Workspace(workspace) => Ok(workspace.conflicts()?),
            Self::Script(_) => Ok(Conflicts::empty()),
        }
    }

    /// Return an iterator over the [`Index`] definitions in the [`LockTarget`].
    pub(crate) fn indexes(self) -> impl Iterator<Item = &'lock Index> {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.indexes().iter().chain(
                workspace.packages().values().flat_map(|member| {
                    member
                        .pyproject_toml()
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.index.as_ref())
                        .into_iter()
                        .flatten()
                }),
            )),
            Self::Script(script) => Either::Right(
                script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .into_iter()
                    .flatten(),
            ),
        }
    }

    /// Return the `Requires-Python` bound for the [`LockTarget`].
    pub(crate) fn requires_python(self) -> Result<Option<RequiresPython>, ProjectError> {
        match self {
            Self::Workspace(workspace) => {
                // When locking, don't try to enforce requires-python bounds that appear on groups
                let groups = DependencyGroupsWithDefaults::none();
                find_requires_python(workspace, &groups)
            }
            Self::Script(script) => Ok(script
                .metadata
                .requires_python
                .as_ref()
                .map(|specifiers| RequiresPython::from_specifiers(specifiers.clone()))),
        }
    }

    /// Return the path to the lock root.
    pub(crate) fn install_path(self) -> &'lock Path {
        match self {
            Self::Workspace(workspace) => workspace.install_path(),
            Self::Script(script) => script.path.parent().unwrap(),
        }
    }

    /// Return the filename of the lockfile, for use in user-facing messages.
    pub(crate) fn lock_filename(self) -> PathBuf {
        PathBuf::from(self.lock_path().file_name().unwrap())
    }

    /// Return the path to the lockfile.
    pub(crate) fn lock_path(self) -> PathBuf {
        match self {
            // `uv.lock`
            Self::Workspace(workspace) => workspace.install_path().join("uv.lock"),
            // `script.py.lock`
            Self::Script(script) => {
                let mut file_name = match script.path.file_name() {
                    Some(f) => f.to_os_string(),
                    None => panic!("Script path has no file name"),
                };
                file_name.push(".lock");
                script.path.with_file_name(file_name)
            }
        }
    }

    /// Read the lockfile from the workspace.
    ///
    /// Returns `Ok(None)` if the lockfile does not exist.
    pub(crate) async fn read(self) -> Result<Option<Lock>, ProjectError> {
        Ok(self
            .read_with_contents()
            .await?
            .map(|(lock, _contents)| lock))
    }

    /// Read the lockfile and return the exact contents that were parsed.
    ///
    /// Returns `Ok(None)` if the lockfile does not exist.
    pub(crate) async fn read_with_contents(self) -> Result<Option<(Lock, String)>, ProjectError> {
        let lock_path = self.lock_path();
        match fs_err::tokio::read_to_string(&lock_path).await {
            Ok(encoded) => {
                let result = info_span!("toml::from_str lock", path = %lock_path.display())
                    .in_scope(|| toml::from_str::<Lock>(&encoded));
                match result {
                    Ok(lock) => {
                        // If the lockfile uses an unsupported version, raise an error.
                        if lock.version() != VERSION {
                            return Err(ProjectError::UnsupportedLockVersion(
                                VERSION,
                                lock.version(),
                            ));
                        }
                        Ok(Some((lock, encoded)))
                    }
                    Err(err) => {
                        // If we failed to parse the lockfile, determine whether it's a supported
                        // version.
                        if let Ok(lock) = toml::from_str::<LockVersion>(&encoded) {
                            if lock.version() != VERSION {
                                return Err(ProjectError::UnparsableLockVersion(
                                    VERSION,
                                    lock.version(),
                                    err,
                                ));
                            }
                        }
                        Err(ProjectError::UvLockParse(err))
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Read the lockfile from the workspace as bytes.
    pub(crate) async fn read_bytes(self) -> Result<Option<Vec<u8>>, std::io::Error> {
        match fs_err::tokio::read(self.lock_path()).await {
            Ok(encoded) => Ok(Some(encoded)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Write the lockfile to disk.
    pub(crate) async fn commit(self, lock: &Lock) -> Result<(), ProjectError> {
        let encoded = lock.to_toml()?;
        fs_err::tokio::write(self.lock_path(), encoded).await?;
        Ok(())
    }

    /// Lower the requirements for the [`LockTarget`], relative to the target root.
    pub(crate) async fn lower(
        self,
        requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
        locations: &IndexLocations,
        sources: &NoSources,
        cache: &Cache,
        workspace_cache: &WorkspaceCache,
        credentials_cache: &CredentialsCache,
    ) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
        match self {
            Self::Workspace(workspace) => {
                let name = workspace
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .map(|project| project.name.clone());

                // We model these as `build-requires`, since, like build requirements, it doesn't define extras
                // or dependency groups.
                let metadata = uv_distribution::BuildRequires::from_workspace(
                    uv_pypi_types::BuildRequires {
                        name,
                        requires_dist: requirements,
                    },
                    workspace,
                    locations,
                    sources,
                    cache,
                    workspace_cache,
                    credentials_cache,
                )
                .await?;

                Ok(metadata
                    .requires_dist
                    .into_iter()
                    .map(|requirement| requirement.with_origin(RequirementOrigin::Workspace))
                    .collect::<Vec<_>>())
            }
            Self::Script(script) => {
                // Collect any `tool.uv.index` from the script.
                let empty = Vec::default();
                let indexes = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .unwrap_or(&empty);

                // Collect any `tool.uv.sources` from the script.
                let empty = BTreeMap::default();
                let sources_map = script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.as_ref())
                    .unwrap_or(&empty);

                let mut lowered = Vec::new();
                for requirement in requirements {
                    if sources.for_package(&requirement.name) {
                        lowered.push(Requirement::from(requirement));
                        continue;
                    }

                    let requirement_name = requirement.name.clone();
                    lowered.extend(
                        LoweredRequirement::from_non_workspace_requirement(
                            requirement,
                            script.path.parent().unwrap(),
                            sources_map,
                            indexes,
                            locations,
                            cache,
                            workspace_cache,
                            credentials_cache,
                        )
                        .await
                        .map(|requirement| {
                            requirement
                                .map(LoweredRequirement::into_inner)
                                .map_err(|err| {
                                    uv_distribution::MetadataError::LoweringError(
                                        requirement_name.clone(),
                                        Box::new(err),
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    );
                }
                Ok(lowered)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    Header,
    Array { multiline: bool },
}

/// Return the first line that does not match the whitespace emitted by the lock writer.
///
/// This only checks the serialization shape. TOML validity and lockfile semantics are checked by
/// the regular lockfile deserializer.
pub(crate) fn find_lock_format_error(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut brackets = Vec::with_capacity(4);
    let mut previous = None;
    let mut line_start = true;
    let mut indented = false;
    let mut line = 1;

    for token in Source::new(source).lex() {
        let kind = token.kind();
        let start = token.span().start();
        let end = token.span().end();

        if kind == TokenKind::Eof {
            return (!source.ends_with('\n')).then_some(line);
        }

        if kind == TokenKind::Whitespace {
            if line_start {
                if &bytes[start..end] != b"    " {
                    return Some(line);
                }
                indented = true;
            } else if &bytes[start..end] != b" " {
                return Some(line);
            }
            previous = Some(kind);
            continue;
        }

        if kind == TokenKind::Newline {
            if end - start != 1 || start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
                return Some(line);
            }
            line_start = true;
            indented = false;
            previous = Some(kind);
            line += 1;
            continue;
        }

        let at_line_start = line_start;
        if line_start {
            let multiline_array = brackets
                .iter()
                .any(|bracket| matches!(bracket, Bracket::Array { multiline: true }));
            if multiline_array && kind != TokenKind::RightSquareBracket && !indented
                || (!multiline_array || kind == TokenKind::RightSquareBracket) && indented
            {
                return Some(line);
            }
            line_start = false;
        }

        match kind {
            TokenKind::Equals => {
                if start == 0
                    || end >= bytes.len()
                    || bytes[start - 1] != b' '
                    || bytes[end] != b' '
                {
                    return Some(line);
                }
            }
            TokenKind::Comma => {
                if start > 0 && matches!(bytes[start - 1], b' ' | b'\t')
                    || end < bytes.len() && !matches!(bytes[end], b' ' | b'\n')
                {
                    return Some(line);
                }
            }
            TokenKind::LeftCurlyBracket => {
                if end < bytes.len() && !matches!(bytes[end], b' ' | b'}') {
                    return Some(line);
                }
            }
            TokenKind::RightCurlyBracket => {
                if start > 0 && !matches!(bytes[start - 1], b' ' | b'{') {
                    return Some(line);
                }
            }
            TokenKind::LeftSquareBracket => {
                if end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
                    return Some(line);
                }
                let header = at_line_start
                    || previous == Some(TokenKind::LeftSquareBracket)
                        && brackets.last() == Some(&Bracket::Header);
                brackets.push(if header {
                    Bracket::Header
                } else {
                    Bracket::Array {
                        multiline: bytes.get(end) == Some(&b'\n'),
                    }
                });
            }
            TokenKind::RightSquareBracket => {
                if start > 0 && matches!(bytes[start - 1], b' ' | b'\t') || brackets.pop().is_none()
                {
                    return Some(line);
                }
            }
            TokenKind::LiteralString | TokenKind::MlLiteralString | TokenKind::MlBasicString => {
                return Some(line);
            }
            _ => {}
        }

        previous = Some(kind);
    }

    Some(line)
}

#[cfg(test)]
mod tests {
    use super::find_lock_format_error;

    const FORMATTED: &str = r#"version = 1
revision = 3
requires-python = ">=3.12"
resolution-markers = [
    "sys_platform == 'darwin'",
    "sys_platform != 'darwin'",
]
conflicts = [[
    { package = "project", extra = "cpu" },
    { package = "project", extra = "gpu" },
], [
    { package = "project", group = "test" },
    { package = "project", group = "lint" },
]]

[options]
exclude-newer = "2024-03-25T00:00:00Z" # Generated comment

[[package]]
name = "project"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
    { name = "sniffio", marker = "python_full_version < '3.13'" },
]

[package.metadata]
requires-dist = [{ name = "sniffio" }]
"#;

    #[test]
    fn accepts_lock_format() {
        assert_eq!(find_lock_format_error(FORMATTED), None);
    }

    #[test]
    fn rejects_lock_format_changes() {
        for unformatted in [
            FORMATTED.replacen("\n    {", "\n{", 1),
            FORMATTED.replacen("\n    \"", "\n\"", 1),
            FORMATTED.replacen(" = ", "  = ", 1),
            FORMATTED.replacen(" = ", "=", 1),
            FORMATTED.replacen(", ", ",  ", 1),
            FORMATTED.replacen("{ ", "{", 1),
            FORMATTED.replacen(" }", "}", 1),
            FORMATTED.replacen('\n', "\r\n", 1),
            FORMATTED.replacen('\n', " \n", 1),
        ] {
            assert!(find_lock_format_error(&unformatted).is_some());
        }

        assert!(find_lock_format_error(FORMATTED.trim_end_matches('\n')).is_some());
    }
}
