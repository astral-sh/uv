use either::Either;
use uv_normalize::GroupName;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DevMode {
    /// Include development dependencies.
    #[default]
    Include,
    /// Exclude development dependencies.
    Exclude,
    /// Only include development dependencies, excluding all other dependencies.
    Only,
}

impl DevMode {
    /// Determine the [`DevMode`] policy from the command-line arguments.
    pub fn from_args(dev: bool, no_dev: bool, only_dev: bool) -> Self {
        if only_dev {
            Self::Only
        } else if no_dev {
            Self::Exclude
        } else if dev {
            Self::Include
        } else {
            Self::default()
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum DevSpecification<'group> {
    /// Include dev dependencies from the specified group.
    Include(&'group [GroupName]),
    /// Do not include dev dependencies.
    Exclude,
    /// Include dev dependencies from the specified group, and exclude all non-dev dependencies.
    Only(&'group [GroupName]),
}

impl<'group> DevSpecification<'group> {
    /// Returns an [`Iterator`] over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        match self {
            Self::Exclude => Either::Left(std::iter::empty()),
            Self::Include(groups) | Self::Only(groups) => Either::Right(groups.iter()),
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        matches!(self, Self::Exclude | Self::Include(_))
    }
}
