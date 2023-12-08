use std::str::FromStr;

/// A complete Git SHA, i.e., a 40-character hexadecimal representation of a Git commit.
#[derive(Debug, Copy, Clone)]
pub struct GitSha(git2::Oid);

impl GitSha {
    /// Convert the SHA to a truncated representation, i.e., the first 16 characters of the SHA.
    pub fn to_short_string(&self) -> String {
        self.0.to_string()[0..16].to_string()
    }
}

impl From<GitSha> for git2::Oid {
    fn from(value: GitSha) -> Self {
        value.0
    }
}

impl From<git2::Oid> for GitSha {
    fn from(value: git2::Oid) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for GitSha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for GitSha {
    type Err = git2::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(git2::Oid::from_str(value)?))
    }
}
