use resolvo::VersionSet;

/// A wrapper around [`pep508_rs::VersionOrUrl`] that implements [`VersionSet`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResolvoVersionSet(Option<pep508_rs::VersionOrUrl>);

impl From<Option<pep508_rs::VersionOrUrl>> for ResolvoVersionSet {
    fn from(value: Option<pep508_rs::VersionOrUrl>) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for ResolvoVersionSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            None => write!(f, "*"),
            Some(pep508_rs::VersionOrUrl::VersionSpecifier(specifiers)) => {
                write!(f, "{specifiers}")
            }
            Some(pep508_rs::VersionOrUrl::Url(url)) => write!(f, "{url}"),
        }
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ResolvoVersion {
    Version(pep440_rs::Version),
    Url(url::Url),
}

impl VersionSet for ResolvoVersionSet {
    type V = ResolvoVersion;

    fn contains(&self, version: &Self::V) -> bool {
        match (self.0.as_ref(), version) {
            (
                Some(pep508_rs::VersionOrUrl::VersionSpecifier(specifiers)),
                ResolvoVersion::Version(version),
            ) => specifiers.contains(version),
            (Some(pep508_rs::VersionOrUrl::Url(url_a)), ResolvoVersion::Url(url_b)) => {
                url_a == url_b
            }
            (None, _) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for ResolvoVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvoVersion::Version(v) => write!(f, "{v}"),
            ResolvoVersion::Url(u) => write!(f, "{u}"),
        }
    }
}
