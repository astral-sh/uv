use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use uv_cache_key::{CanonicalUrl, RepositoryUrl};
use uv_git_types::GitUrl;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::{
    HashDigest, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl,
};
use uv_redacted::DisplaySafeUrl;

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
    pub fn from_registry(name: PackageName) -> Self {
        Self::Name(name)
    }

    /// Create a new [`PackageId`] from a URL.
    pub fn from_url(url: &DisplaySafeUrl) -> Self {
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
    Unknown(DisplaySafeUrl),
}

impl VersionId {
    /// Create a new [`VersionId`] from a package name and version.
    pub fn from_registry(name: PackageName, version: Version) -> Self {
        Self::NameVersion(name, version)
    }

    /// Create a new [`VersionId`] from a parsed URL.
    pub fn from_parsed_url(url: &ParsedUrl) -> Self {
        match url {
            ParsedUrl::Path(path) => Self::from_path_url(path),
            ParsedUrl::Directory(directory) => Self::from_directory_url(directory),
            ParsedUrl::Git(git) => Self::from_git_url(git),
            ParsedUrl::Archive(archive) => Self::from_archive_url(archive),
        }
    }

    /// Create a new [`VersionId`] from a URL.
    pub fn from_url(url: &DisplaySafeUrl) -> Self {
        match ParsedUrl::try_from(url.clone()) {
            Ok(parsed) => Self::from_parsed_url(&parsed),
            Err(_) => Self::Unknown(url.clone()),
        }
    }

    /// Create a new [`VersionId`] from an archive URL.
    pub fn from_archive(location: &DisplaySafeUrl, subdirectory: Option<&Path>) -> Self {
        Self::ArchiveUrl {
            location: CanonicalUrl::new(location),
            subdirectory: subdirectory.map(Path::to_path_buf),
        }
    }

    /// Create a new [`VersionId`] from a Git URL.
    pub fn from_git(git: &GitUrl, subdirectory: Option<&Path>) -> Self {
        // TODO(charlie): Canonicalize repository URLs in `GitUrl` itself so `VersionId` does not
        // need to rebuild the value here.
        let git = GitUrl::from_fields(
            DisplaySafeUrl::from(CanonicalUrl::new(git.repository())),
            git.reference().clone(),
            git.precise(),
            git.lfs(),
        )
        .expect("canonical Git URLs should preserve supported schemes");

        Self::Git {
            url: git,
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

    fn from_archive_url(archive: &ParsedArchiveUrl) -> Self {
        Self::from_archive(&archive.url, archive.subdirectory.as_deref())
    }

    fn from_path_url(path: &ParsedPathUrl) -> Self {
        Self::from_path(path.install_path.as_ref())
    }

    fn from_directory_url(directory: &ParsedDirectoryUrl) -> Self {
        Self::from_directory(directory.install_path.as_ref())
    }

    fn from_git_url(git: &ParsedGitUrl) -> Self {
        Self::from_git(&git.url, git.subdirectory.as_deref())
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
                let mut location = DisplaySafeUrl::from(location.clone());
                if let Some(subdirectory) = subdirectory {
                    location
                        .set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
                }
                write!(f, "{location}")
            }
            Self::Git { url, subdirectory } => {
                let mut git_url = DisplaySafeUrl::parse(&format!("git+{}", url.repository()))
                    .expect("canonical Git URLs should be display-safe");
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
                if let Ok(url) = DisplaySafeUrl::from_file_path(path) {
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
    PathBuf(PathBuf),
    Digest(HashDigest),
    AbsoluteUrl(String),
    RelativeUrl(String, String),
}

/// A unique identifier for a resource, like a URL or a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceId {
    Url(RepositoryUrl),
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use fs_err as fs;

    use super::VersionId;
    use uv_redacted::DisplaySafeUrl;

    #[test]
    fn version_id_ignores_hash_fragments() {
        let first = DisplaySafeUrl::parse(
            "https://example.com/pkg-0.1.0.whl#sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let second = DisplaySafeUrl::parse(
            "https://example.com/pkg-0.1.0.whl#sha512=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap();

        assert_eq!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_preserves_non_hash_fragments() {
        let first =
            DisplaySafeUrl::parse("https://example.com/pkg-0.1.0.tar.gz#subdirectory=foo").unwrap();
        let second =
            DisplaySafeUrl::parse("https://example.com/pkg-0.1.0.tar.gz#subdirectory=bar").unwrap();

        assert_ne!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_ignores_hash_fragments_with_subdirectory() {
        let first = DisplaySafeUrl::parse(
            "https://example.com/pkg-0.1.0.tar.gz#subdirectory=foo&sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap();
        let second = DisplaySafeUrl::parse(
            "https://example.com/pkg-0.1.0.tar.gz#sha512=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb&subdirectory=foo",
        )
        .unwrap();

        assert_eq!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_preserves_non_archive_fragments() {
        let first =
            DisplaySafeUrl::parse("git+https://example.com/pkg.git#subdirectory=foo").unwrap();
        let second =
            DisplaySafeUrl::parse("git+https://example.com/pkg.git#subdirectory=bar").unwrap();

        assert_ne!(VersionId::from_url(&first), VersionId::from_url(&second));
    }

    #[test]
    fn version_id_ignores_irrelevant_git_fragments() {
        let first =
            DisplaySafeUrl::parse("git+https://example.com/pkg.git@main#egg=pkg&subdirectory=foo")
                .unwrap();
        let second =
            DisplaySafeUrl::parse("git+https://example.com/pkg.git@main#subdirectory=foo").unwrap();

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

        let file_url = DisplaySafeUrl::from_file_path(&file).unwrap();
        let directory_url = DisplaySafeUrl::from_file_path(&directory).unwrap();

        assert!(matches!(VersionId::from_url(&file_url), VersionId::Path(_)));
        assert!(matches!(
            VersionId::from_url(&directory_url),
            VersionId::Directory(_)
        ));

        fs::remove_file(file).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_id_uses_unknown_for_invalid_git_like_urls() {
        let url =
            DisplaySafeUrl::parse("git+ftp://example.com/pkg.git@main#subdirectory=foo").unwrap();

        assert!(matches!(VersionId::from_url(&url), VersionId::Unknown(_)));
    }
}
