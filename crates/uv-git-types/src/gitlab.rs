use tracing::debug;
use url::Url;

/// A reference to a repository on GitLab (gitlab.com or self-hosted).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitLabRepository<'a> {
    /// The host of the GitLab instance (e.g., "gitlab.com" or "gitlab.example.com").
    pub host: &'a str,
    /// The full project path (e.g., "group/subgroup/project" or "user/project").
    pub project_path: String,
}

impl<'a> GitLabRepository<'a> {
    /// Parse a GitLab repository from a URL.
    ///
    /// Supports both gitlab.com and self-hosted GitLab instances.
    /// Expects URLs like:
    /// - `https://gitlab.com/group/project`
    /// - `https://gitlab.com/group/subgroup/project`
    /// - `https://gitlab.example.com/org/project`
    pub fn parse(url: &'a Url) -> Option<Self> {
        let host = url.host_str()?;

        // Check if this looks like a GitLab instance
        if !Self::is_gitlab_host(host) {
            return None;
        }

        // Get the project path from URL segments
        let segments: Vec<&str> = url.path_segments()?.collect();

        // Need at least user/project or group/project
        if segments.len() < 2 {
            debug!("GitLab URL has too few path segments: {url}");
            return None;
        }

        // Filter out empty segments and join them
        let project_path: Vec<&str> = segments.into_iter().filter(|s| !s.is_empty()).collect();

        if project_path.len() < 2 {
            debug!("GitLab URL has too few path components: {url}");
            return None;
        }

        // Join the path and trim .git suffix if present
        let mut path = project_path.join("/");
        if let Some(stripped) = path.strip_suffix(".git") {
            path = stripped.to_string();
        }

        Some(Self {
            host,
            project_path: path,
        })
    }

    /// Check if a host looks like a GitLab instance.
    fn is_gitlab_host(host: &str) -> bool {
        // Official GitLab
        if host == "gitlab.com" {
            return true;
        }

        // Self-hosted GitLab instances often have "gitlab" in the hostname
        if host.contains("gitlab") {
            return true;
        }

        false
    }

    /// Get the URL-encoded project path for API calls.
    ///
    /// GitLab API requires the project path to be URL-encoded, e.g.,
    /// `group/subgroup/project` becomes `group%2Fsubgroup%2Fproject`.
    pub fn encoded_project_path(&self) -> String {
        self.project_path.replace('/', "%2F")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gitlab_com() {
        let url = Url::parse("https://gitlab.com/user/project").unwrap();
        let repo = GitLabRepository::parse(&url).unwrap();
        assert_eq!(repo.host, "gitlab.com");
        assert_eq!(repo.project_path, "user/project");
    }

    #[test]
    fn test_parse_gitlab_com_with_subgroups() {
        let url = Url::parse("https://gitlab.com/group/subgroup/project").unwrap();
        let repo = GitLabRepository::parse(&url).unwrap();
        assert_eq!(repo.host, "gitlab.com");
        assert_eq!(repo.project_path, "group/subgroup/project");
    }

    #[test]
    fn test_parse_self_hosted() {
        let url = Url::parse("https://gitlab.example.com/org/team/project").unwrap();
        let repo = GitLabRepository::parse(&url).unwrap();
        assert_eq!(repo.host, "gitlab.example.com");
        assert_eq!(repo.project_path, "org/team/project");
    }

    #[test]
    fn test_parse_with_git_suffix() {
        let url = Url::parse("https://gitlab.com/user/project.git").unwrap();
        let repo = GitLabRepository::parse(&url).unwrap();
        assert_eq!(repo.project_path, "user/project");
    }

    #[test]
    fn test_encoded_project_path() {
        let url = Url::parse("https://gitlab.com/group/subgroup/project").unwrap();
        let repo = GitLabRepository::parse(&url).unwrap();
        assert_eq!(repo.encoded_project_path(), "group%2Fsubgroup%2Fproject");
    }

    #[test]
    fn test_parse_non_gitlab() {
        let url = Url::parse("https://github.com/user/project").unwrap();
        assert!(GitLabRepository::parse(&url).is_none());
    }

    #[test]
    fn test_parse_too_few_segments() {
        let url = Url::parse("https://gitlab.com/user").unwrap();
        assert!(GitLabRepository::parse(&url).is_none());
    }
}
