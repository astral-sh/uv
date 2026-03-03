use uv_normalize::PackageName;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum NoSources {
    /// Use `tool.uv.sources` when resolving dependencies.
    #[default]
    None,

    /// Ignore `tool.uv.sources` when resolving dependencies for all packages.
    All,

    /// Ignore `tool.uv.sources` when resolving dependencies for specific packages.
    Packages(Vec<PackageName>),
}

impl NoSources {
    /// Determine the no sources strategy to use for the given arguments.
    pub fn from_args(no_sources: Option<bool>, no_sources_package: Vec<PackageName>) -> Self {
        match no_sources {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if no_sources_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(no_sources_package)
                }
            }
        }
    }

    /// Returns `true` if all sources should be ignored.
    pub fn all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns `true` if sources should be ignored for the given package.
    pub fn for_package(&self, package_name: &PackageName) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Packages(packages) => packages.contains(package_name),
        }
    }

    /// Combine a set of [`NoSources`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, Self::None) => Self::None,
            (Self::All, _) | (_, Self::All) => Self::All,
            (Self::Packages(a), Self::None) => Self::Packages(a),
            (Self::None, Self::Packages(b)) => Self::Packages(b),
            (Self::Packages(mut a), Self::Packages(b)) => {
                a.extend(b);
                Self::Packages(a)
            }
        }
    }

    /// Extend a [`NoSources`] value with another.
    pub fn extend(&mut self, other: Self) {
        match (&mut *self, other) {
            (Self::All, _) | (_, Self::All) => *self = Self::All,
            (Self::None, Self::None) => {}
            (Self::Packages(_), Self::None) => {}
            (Self::None, Self::Packages(b)) => {
                *self = Self::Packages(b);
            }
            (Self::Packages(a), Self::Packages(b)) => {
                a.extend(b);
            }
        }
    }
}

impl NoSources {
    /// Returns `true` if all sources are allowed.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}
