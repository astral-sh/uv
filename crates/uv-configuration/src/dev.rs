use either::Either;
use uv_normalize::{GroupName, DEV_DEPENDENCIES};

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

#[derive(Debug, Clone)]
pub enum DevSpecification {
    /// Include dev dependencies from the specified group.
    Include(Vec<GroupName>),
    /// Do not include dev dependencies.
    Exclude,
    /// Include dev dependencies from the specified groups, and exclude all non-dev dependencies.
    Only(Vec<GroupName>),
}

impl DevSpecification {
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

impl From<DevMode> for DevSpecification {
    fn from(mode: DevMode) -> Self {
        match mode {
            DevMode::Include => Self::Include(vec![DEV_DEPENDENCIES.clone()]),
            DevMode::Exclude => Self::Exclude,
            DevMode::Only => Self::Only(vec![DEV_DEPENDENCIES.clone()]),
        }
    }
}

impl DevSpecification {
    /// Determine the [`DevSpecification`] policy from the command-line arguments.
    pub fn from_args(
        dev: bool,
        no_dev: bool,
        only_dev: bool,
        group: Vec<GroupName>,
        only_group: Vec<GroupName>,
    ) -> Self {
        let from_mode = DevSpecification::from(DevMode::from_args(dev, no_dev, only_dev));
        if !group.is_empty() {
            match from_mode {
                DevSpecification::Exclude => Self::Include(group),
                DevSpecification::Include(dev) => {
                    Self::Include(group.into_iter().chain(dev).collect())
                }
                DevSpecification::Only(_) => {
                    unreachable!("cannot specify both `--only-dev` and `--group`")
                }
            }
        } else if !only_group.is_empty() {
            match from_mode {
                DevSpecification::Exclude => Self::Only(only_group),
                DevSpecification::Only(dev) => {
                    Self::Only(only_group.into_iter().chain(dev).collect())
                }
                // TODO(zanieb): `dev` defaults to true we can't tell if `--dev` was provided in
                // conflict with `--only-group` here
                DevSpecification::Include(_) => Self::Only(only_group),
            }
        } else {
            from_mode
        }
    }
}
