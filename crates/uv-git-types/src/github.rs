use tracing::debug;
use url::Url;

/// A reference to a repository on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubRepository<'a> {
    /// The `owner` field for the repository, i.e., the user or organization that owns the
    /// repository, like `astral-sh`.
    pub owner: &'a str,
    /// The `repo` field for the repository, i.e., the name of the repository, like `uv`.
    pub repo: &'a str,
}

impl<'a> GitHubRepository<'a> {
    /// Parse a GitHub repository from a URL.
    ///
    /// Expects to receive a URL of the form: `https://github.com/{user}/{repo}[.git]`, e.g.,
    /// `https://github.com/astral-sh/uv`. Otherwise, returns `None`.
    pub fn parse(url: &'a Url) -> Option<Self> {
        // The fast path is only available for GitHub repositories.
        if url.host_str() != Some("github.com") {
            return None;
        };

        // The GitHub URL must take the form: `https://github.com/{user}/{repo}`, e.g.,
        // `https://github.com/astral-sh/uv`.
        let Some(mut segments) = url.path_segments() else {
            debug!("GitHub URL is missing path segments: {url}");
            return None;
        };
        let Some(owner) = segments.next() else {
            debug!("GitHub URL is missing owner: {url}");
            return None;
        };
        let Some(repo) = segments.next() else {
            debug!("GitHub URL is missing repo: {url}");
            return None;
        };
        if segments.next().is_some() {
            debug!("GitHub URL has too many path segments: {url}");
            return None;
        }

        // Trim off the `.git` from the repository, if present.
        let repo = repo.strip_suffix(".git").unwrap_or(repo);

        Some(Self { owner, repo })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_url() {
        let url = Url::parse("https://github.com/astral-sh/uv").unwrap();
        let repo = GitHubRepository::parse(&url).unwrap();
        assert_eq!(repo.owner, "astral-sh");
        assert_eq!(repo.repo, "uv");
    }

    #[test]
    fn test_parse_with_git_suffix() {
        let url = Url::parse("https://github.com/astral-sh/uv.git").unwrap();
        let repo = GitHubRepository::parse(&url).unwrap();
        assert_eq!(repo.owner, "astral-sh");
        assert_eq!(repo.repo, "uv");
    }

    #[test]
    fn test_parse_invalid_host() {
        let url = Url::parse("https://gitlab.com/astral-sh/uv").unwrap();
        assert!(GitHubRepository::parse(&url).is_none());
    }

    #[test]
    fn test_parse_invalid_path() {
        let url = Url::parse("https://github.com/astral-sh").unwrap();
        assert!(GitHubRepository::parse(&url).is_none());

        let url = Url::parse("https://github.com/astral-sh/uv/extra").unwrap();
        assert!(GitHubRepository::parse(&url).is_none());
    }
}
