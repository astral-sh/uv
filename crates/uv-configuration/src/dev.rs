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
    /// Iterate over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        match self {
            Self::Exclude => Either::Left(std::iter::empty()),
            Self::Include | Self::Only => Either::Right(std::iter::once(&*DEV_DEPENDENCIES)),
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        matches!(self, Self::Exclude | Self::Include)
    }
}

#[derive(Debug, Clone)]
pub struct DevGroupsSpecification {
    /// Legacy option for `dependency-group.dev` and `tool.uv.dev-dependencies`.
    ///
    /// Requested via the `--dev`, `--no-dev`, and `--only-dev` flags.
    dev: Option<DevMode>,

    /// The groups to include.
    ///
    /// Requested via the `--group` and `--only-group` options.
    groups: GroupsSpecification,
}

#[derive(Debug, Clone)]
pub enum GroupsSpecification {
    /// Include dependencies from the specified groups.
    Include(Vec<GroupName>),
    /// Do not include dependencies from groups.
    Exclude,
    /// Only include dependencies from the specified groups, exclude all other dependencies.
    Only(Vec<GroupName>),
}

impl GroupsSpecification {
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

impl DevGroupsSpecification {
    /// Returns an [`Iterator`] over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        match self.dev {
            None => Either::Left(self.groups.iter()),
            Some(ref dev_mode) => Either::Right(self.groups.iter().chain(dev_mode.iter())),
        }
    }

    /// Determine the [`DevGroupsSpecification`] policy from the command-line arguments.
    pub fn from_args(
        dev: bool,
        no_dev: bool,
        only_dev: bool,
        group: Vec<GroupName>,
        only_group: Vec<GroupName>,
    ) -> Self {
        let dev_mode = if only_dev {
            Some(DevMode::Only)
        } else if no_dev {
            Some(DevMode::Exclude)
        } else if dev {
            Some(DevMode::Include)
        } else {
            None
        };

        let groups = if !group.is_empty() {
            if matches!(dev_mode, Some(DevMode::Only)) {
                unreachable!("cannot specify both `--only-dev` and `--group`")
            };
            GroupsSpecification::Include(group)
        } else if !only_group.is_empty() {
            if matches!(dev_mode, Some(DevMode::Include)) {
                unreachable!("cannot specify both `--dev` and `--only-group`")
            };
            GroupsSpecification::Only(only_group)
        } else {
            GroupsSpecification::Exclude
        };

        Self {
            dev: dev_mode,
            groups,
        }
    }

    /// Return a new [`DevGroupsSpecification`] with development dependencies included by default.
    ///
    /// This is appropriate in projects, where the `dev` group is synced by default.
    #[must_use]
    pub fn with_default_dev(self) -> Self {
        match self.dev {
            Some(_) => self,
            None => match self.groups {
                // Only include the default `dev` group if `--only-group` wasn't used
                GroupsSpecification::Only(_) => self,
                GroupsSpecification::Exclude | GroupsSpecification::Include(_) => Self {
                    dev: Some(DevMode::Include),
                    ..self
                },
            },
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        (self.dev.is_none() || self.dev.as_ref().is_some_and(DevMode::prod)) && self.groups.prod()
    }

    pub fn dev_mode(&self) -> Option<&DevMode> {
        self.dev.as_ref()
    }
}

impl From<DevMode> for DevGroupsSpecification {
    fn from(dev: DevMode) -> Self {
        Self {
            dev: Some(dev),
            groups: GroupsSpecification::Exclude,
        }
    }
}

impl From<GroupsSpecification> for DevGroupsSpecification {
    fn from(groups: GroupsSpecification) -> Self {
        Self { dev: None, groups }
    }
}
