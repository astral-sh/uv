use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use uv_cache_key::{CanonicalUrl, GitUpstreamUrl};
use uv_fs::normalize_path;
use uv_git_types::GitUrl;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::{HashDigest, ParsedUrl};
use uv_redacted::SafeUrl;

/// A unique identifier for a package. A package can either be identified by a name (e.g., `black`)
/// or a URL (e.g., `git+https://github.com/psf/black`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PackageId {
    /// The identifier consists of a package name.
    Name(PackageName),
    /// The identifier consists of a URL.
    Url(CanonicalUrl),
}

impl PackageId {
    /// Create a new [`PackageId`] from a package name and version.
    pub(crate) fn from_registry(name: PackageName) -> Self {
        Self::Name(name)
    }

    /// Create a new [`PackageId`] from a URL.
    pub(crate) fn from_url(url: SafeUrl) -> Self {
        Self::Url(CanonicalUrl::new(url))
    }
}

impl Display for PackageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(name) => write!(f, "{name}"),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// A unique identifier for a package at a specific version (e.g., `black==23.10.0`).
///
/// URL-based variants use kind-specific identity semantics. Archive URLs ignore hash fragments
/// while preserving semantic `subdirectory` information. Git URLs preserve semantic
/// `subdirectory` information while ignoring unrelated fragments. Local file URLs are keyed by
/// their resolved path and kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VersionId {
    /// The identifier consists of a package name and version.
    NameVersion(PackageName, Version),
    /// The identifier consists of an archive URL identified by its location and optional source
    /// subdirectory.
    ArchiveUrl {
        location: CanonicalUrl,
        subdirectory: Option<PathBuf>,
    },
    /// The identifier consists of a Git repository URL, its reference, and optional source
    /// subdirectory.
    Git {
        url: GitUrl,
        subdirectory: Option<PathBuf>,
    },
    /// The identifier consists of a local file path.
    Path(PathBuf),
    /// The identifier consists of a local directory path.
    Directory(PathBuf),
    /// The identifier consists of a URL whose source kind could not be determined.
    Unknown(SafeUrl),
}

impl VersionId {
    /// Create a new [`VersionId`] from a package name and version.
    pub fn from_registry(name: PackageName, version: Version) -> Self {
        Self::NameVersion(name, version)
    }

    /// Create a new [`VersionId`] from a parsed URL.
    pub fn from_parsed_url(url: ParsedUrl) -> Self {
        match url {
            ParsedUrl::Path(path) => Self::Path(path.install_path.into_path_buf()),
            ParsedUrl::Directory(directory) => {
                Self::Directory(directory.install_path.into_path_buf())
            }
            ParsedUrl::GitDirectory(git) => Self::Git {
                url: git.url,
                subdirectory: git.subdirectory.map(Path::into_path_buf),
            },
            ParsedUrl::GitPath(git) => Self::Git {
                url: git.url,
                subdirectory: Some(git.install_path),
            },
            ParsedUrl::Archive(archive) => {
                Self::from_archive(archive.url, archive.subdirectory.map(Path::into_path_buf))
            }
        }
    }

    /// Create a new [`VersionId`] from a URL.
    pub fn from_url(url: &SafeUrl) -> Self {
        match ParsedUrl::try_from(url.clone()) {
            Ok(parsed) => Self::from_parsed_url(parsed),
            Err(_) => Self::Unknown(url.clone()),
        }
    }

    /// Create a new [`VersionId`] from an archive URL.
    pub fn from_archive(location: SafeUrl, subdirectory: Option<PathBuf>) -> Self {
        // Use the same lexical normalization as the resolver's source comparison. Equivalent
        // subdirectories must not lose their trusted hashes during lowering or lockfile reads.
        let subdirectory = subdirectory
            .map(|path| normalize_path(path).into_owned())
            .filter(|path| !path.as_os_str().is_empty());
        Self::ArchiveUrl {
            location: CanonicalUrl::new(location),
            subdirectory,
        }
    }

    /// Create a new [`VersionId`] from a Git URL.
    pub fn from_git(git: &GitUrl, subdirectory: Option<&Path>) -> Self {
        Self::Git {
            url: git.clone(),
            subdirectory: subdirectory.map(Path::to_path_buf),
        }
    }

    /// Create a new [`VersionId`] from a local file path.
    pub fn from_path(path: &Path) -> Self {
        Self::Path(path.to_path_buf())
    }

    /// Create a new [`VersionId`] from a local directory path.
    pub fn from_directory(path: &Path) -> Self {
        Self::Directory(path.to_path_buf())
    }
}

impl Display for VersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameVersion(name, version) => write!(f, "{name}-{version}"),
            Self::ArchiveUrl {
                location,
                subdirectory,
            } => {
                let mut location = SafeUrl::from(location.clone());
                if let Some(subdirectory) = subdirectory {
                    location
                        .set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
                }
                write!(f, "{location}")
            }
            Self::Git { url, subdirectory } => {
                let mut git_url = SafeUrl::parse(&format!("git+{}", url.url()))
                    .expect("Git URLs should be display-safe");
                if let Some(precise) = url.precise() {
                    let path = format!("{}@{}", git_url.path(), precise);
                    git_url.set_path(&path);
                } else if let Some(reference) = url.reference().as_str() {
                    let path = format!("{}@{}", git_url.path(), reference);
                    git_url.set_path(&path);
                }

                let mut fragments = Vec::new();
                if let Some(subdirectory) = subdirectory {
                    fragments.push(format!("subdirectory={}", subdirectory.display()));
                }
                if url.lfs().enabled() {
                    fragments.push("lfs=true".to_string());
                }
                if !fragments.is_empty() {
                    git_url.set_fragment(Some(&fragments.join("&")));
                }

                write!(f, "{git_url}")
            }
            Self::Path(path) | Self::Directory(path) => {
                if let Ok(url) = SafeUrl::from_file_path(path) {
                    write!(f, "{url}")
                } else {
                    write!(f, "{}", path.display())
                }
            }
            Self::Unknown(url) => write!(f, "{url}"),
        }
    }
}

/// A unique resource identifier for the distribution, like a SHA-256 hash of the distribution's
/// contents.
///
/// A distribution is a specific archive of a package at a specific version. For a given package
/// version, there may be multiple distributions, e.g., source distribution, along with
/// multiple binary distributions (wheels) for different platforms. As a concrete example,
/// `black-23.10.0-py3-none-any.whl` would represent a (binary) distribution of the `black` package
/// at version `23.10.0`.
///
/// The distribution ID is used to uniquely identify a distribution. Ideally, the distribution
/// ID should be a hash of the distribution's contents, though in practice, it's only required
/// that the ID is unique within a single invocation of the resolver (and so, e.g., a hash of
/// the URL would also be sufficient).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DistributionId {
    Url(CanonicalUrl),
    /// A source tree at the Git repository root (`None`) or in a subdirectory (`Some`).
    GitDirectory(GitDistributionUrl, Option<PathBuf>),
    /// A wheel or source archive at a path relative to the Git repository root.
    GitPath(GitDistributionUrl, PathBuf),
    PathBuf(PathBuf),
    Digest(HashDigest),
    AbsoluteUrl(String),
    RelativeUrl(String, String),
}

/// Keys a Git distribution by its parsed reference and raw transport query and fragment.
/// Git upstream identity omits the query and fragment.
#[derive(Debug, Clone)]
pub struct GitDistributionUrl(GitUrl);

impl GitDistributionUrl {
    fn key(&self) -> (&GitUrl, Option<&str>, Option<&str>) {
        let url = self.0.url();
        (&self.0, url.query(), url.fragment())
    }
}

impl From<&GitUrl> for GitDistributionUrl {
    fn from(git: &GitUrl) -> Self {
        Self(git.clone())
    }
}

impl PartialEq for GitDistributionUrl {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for GitDistributionUrl {}

impl PartialOrd for GitDistributionUrl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GitDistributionUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl Hash for GitDistributionUrl {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// A unique identifier for a resource, like a URL or a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceId {
    Url(CanonicalUrl),
    Git(GitUpstreamUrl),
    PathBuf(PathBuf),
    Digest(HashDigest),
    AbsoluteUrl(String),
    RelativeUrl(String, String),
}

impl From<&Self> for VersionId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl From<&Self> for DistributionId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

impl From<&Self> for ResourceId {
    /// Required for `WaitMap::wait`.
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::error::Error;
    use std::str::FromStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    use fs_err as fs;
    use uv_git_types::{GitLfs, GitReference, GitUrl};
    use uv_normalize::PackageName;
    use uv_pep508::Pep508Url;
    use uv_pypi_types::VerbatimParsedUrl;
    use uv_redacted::SafeUrl;

    use super::{GitDistributionUrl, VersionId};
    use crate::{Dist, Identifier};

    #[test]
    fn http_resource_id_preserves_paths_and_queries() -> Result<(), Box<dyn Error>> {
        let original = SafeUrl::parse("https://example.com/simple/?index=public")?;
        let different_path = SafeUrl::parse("https://example.com/simple.git/?index=public")?;
        let different_query = SafeUrl::parse("https://example.com/simple/?index=private")?;
        let with_fragment = SafeUrl::parse("https://example.com/simple/?index=public#fragment")?;

        assert_ne!(original.resource_id(), different_path.resource_id());
        assert_ne!(original.resource_id(), different_query.resource_id());
        assert_eq!(original.resource_id(), with_fragment.resource_id());
        Ok(())
    }

    #[test]
    fn git_distribution_id_preserves_reference_and_subdirectory() -> Result<(), Box<dyn Error>> {
        let name = PackageName::from_str("pkg")?;
        let dist = |url| -> Result<Dist, Box<dyn Error>> {
            Ok(Dist::from_url(
                name.clone(),
                VerbatimParsedUrl::parse_url(url, None)?,
            )?)
        };
        let original = dist("git+https://github.com/example/pkg@main#subdirectory=first")?;
        let alias = dist("git+https://github.com/Example/Pkg.git@main#subdirectory=first")?;
        let different_ref = dist("git+https://github.com/example/pkg@Main#subdirectory=first")?;
        let different_subdir = dist("git+https://github.com/example/pkg@main#subdirectory=second")?;

        assert_eq!(original.distribution_id(), alias.distribution_id());
        assert_ne!(original.distribution_id(), different_ref.distribution_id());
        assert_ne!(
            original.distribution_id(),
            different_subdir.distribution_id()
        );
        assert_eq!(original.resource_id(), alias.resource_id());
        assert_eq!(original.resource_id(), different_ref.resource_id());
        assert_eq!(original.resource_id(), different_subdir.resource_id());
        Ok(())
    }

    #[test]
    fn git_distribution_url_preserves_transport_query_and_fragment() -> Result<(), Box<dyn Error>> {
        let git = |url| -> Result<GitUrl, Box<dyn Error>> {
            Ok(GitUrl::from_fields(
                SafeUrl::parse(url)?,
                GitReference::Branch("main".to_owned()),
                None,
                GitLfs::Disabled,
            )?)
        };
        let original = git("https://example.com/pkg.git?tenant#source")?;
        let different_query = git("https://example.com/pkg.git?tenant=#source")?;
        let different_fragment = git("https://example.com/pkg.git?tenant#other")?;

        assert_ne!(
            GitDistributionUrl::from(&original),
            GitDistributionUrl::from(&different_query),
        );
        assert_ne!(
            GitDistributionUrl::from(&original),
            GitDistributionUrl::from(&different_fragment),
        );
        Ok(())
    }

    #[test]
    fn version_id_ignores_hash_fragments() {
        let first = SafeUrl::parse(
            "https://example.com/pkg-0.1.0.whl#sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let second = SafeUrl::parse(
            "https://example.com/pkg-0.1.0.whl#sha512=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap();

        assert_eq!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_preserves_non_hash_fragments() {
        let first =
            SafeUrl::parse("https://example.com/pkg-0.1.0.tar.gz#subdirectory=foo").unwrap();
        let second =
            SafeUrl::parse("https://example.com/pkg-0.1.0.tar.gz#subdirectory=bar").unwrap();

        assert_ne!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_ignores_hash_fragments_with_subdirectory() {
        let first = SafeUrl::parse(
            "https://example.com/pkg-0.1.0.tar.gz#subdirectory=foo&sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let second = SafeUrl::parse(
            "https://example.com/pkg-0.1.0.tar.gz#sha512=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb&subdirectory=foo",
        )
        .unwrap();

        assert_eq!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_preserves_non_archive_fragments() {
        let first = SafeUrl::parse("git+https://example.com/pkg.git#subdirectory=foo").unwrap();
        let second = SafeUrl::parse("git+https://example.com/pkg.git#subdirectory=bar").unwrap();

        assert_ne!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_ignores_irrelevant_git_fragments() {
        let first = SafeUrl::parse("git+https://example.com/pkg.git@main#egg=pkg&subdirectory=foo")
            .unwrap();
        let second =
            SafeUrl::parse("git+https://example.com/pkg.git@main#subdirectory=foo").unwrap();

        assert_eq!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_uses_file_kinds() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("uv-version-id-{nonce}"));
        let file = root.join("pkg-0.1.0.whl");
        let directory = root.join("pkg");

        fs::create_dir_all(&directory).unwrap();
        fs::write(&file, b"wheel").unwrap();

        let file_url = SafeUrl::from_file_path(&file).unwrap();
        let directory_url = SafeUrl::from_file_path(&directory).unwrap();

        assert_matches!(VersionId::from_url(&file_url), VersionId::Path(_));
        assert_matches!(VersionId::from_url(&directory_url), VersionId::Directory(_));

        fs::remove_file(file).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_id_uses_unknown_for_invalid_git_like_urls() {
        let url = SafeUrl::parse("git+ftp://example.com/pkg.git@main#subdirectory=foo").unwrap();

        assert_matches!(VersionId::from_url(&url), VersionId::Unknown(_));
    }
}
