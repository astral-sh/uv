use std::borrow::Cow;

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
    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        matches!(self, Self::Exclude | Self::Include)
    }

    /// Returns `true` if the specification only includes development dependencies.
    pub fn only(&self) -> bool {
        matches!(self, Self::Only)
    }

    /// Returns the flag that was used to request development dependencies.
    pub fn as_flag(&self) -> &'static str {
        match self {
            Self::Exclude => "--no-dev",
            Self::Include => "--dev",
            Self::Only => "--only-dev",
        }
    }

    /// Iterate over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<'a> IntoIterator for &'a DevMode {
    type Item = &'a GroupName;
    type IntoIter = Either<std::iter::Empty<&'a GroupName>, std::iter::Once<&'a GroupName>>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            DevMode::Exclude => Either::Left(std::iter::empty()),
            DevMode::Include | DevMode::Only => Either::Right(std::iter::once(&*DEV_DEPENDENCIES)),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct DevGroupsSpecification {
    /// Legacy option for `dependency-group.dev` and `tool.uv.dev-dependencies`.
    ///
    /// Requested via the `--dev`, `--no-dev`, and `--only-dev` flags.
    dev: Option<DevMode>,

    /// The groups to include.
    ///
    /// Requested via the `--group` and `--only-group` options.
    groups: Option<GroupsSpecification>,
}

#[derive(Debug, Clone)]
pub enum GroupsSpecification {
    /// Include dependencies from the specified groups.
    Include(Vec<GroupName>),
    /// Only include dependencies from the specified groups, exclude all other dependencies.
    Only(Vec<GroupName>),
}

impl GroupsSpecification {
    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        matches!(self, Self::Include(_))
    }

    /// Returns `true` if the specification is limited to a select set of groups.
    pub fn only(&self) -> bool {
        matches!(self, Self::Only(_))
    }

    /// Returns the option that was used to request the groups, if any.
    pub fn as_flag(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Include(groups) => match groups.as_slice() {
                [] => None,
                [group] => Some(Cow::Owned(format!("--group {group}"))),
                [..] => Some(Cow::Borrowed("--group")),
            },
            Self::Only(groups) => match groups.as_slice() {
                [] => None,
                [group] => Some(Cow::Owned(format!("--only-group {group}"))),
                [..] => Some(Cow::Borrowed("--only-group")),
            },
        }
    }

    /// Iterate over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<'a> IntoIterator for &'a GroupsSpecification {
    type Item = &'a GroupName;
    type IntoIter = std::slice::Iter<'a, GroupName>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            GroupsSpecification::Include(groups) | GroupsSpecification::Only(groups) => {
                groups.iter()
            }
        }
    }
}

impl DevGroupsSpecification {
    /// Determine the [`DevGroupsSpecification`] policy from the command-line arguments.
    pub fn from_args(
        dev: bool,
        no_dev: bool,
        only_dev: bool,
        group: Vec<GroupName>,
        only_group: Vec<GroupName>,
    ) -> Self {
        let dev = if only_dev {
            Some(DevMode::Only)
        } else if no_dev {
            Some(DevMode::Exclude)
        } else if dev {
            Some(DevMode::Include)
        } else {
            None
        };

        let groups = if !group.is_empty() {
            if matches!(dev, Some(DevMode::Only)) {
                unreachable!("cannot specify both `--only-dev` and `--group`")
            };
            Some(GroupsSpecification::Include(group))
        } else if !only_group.is_empty() {
            if matches!(dev, Some(DevMode::Include)) {
                unreachable!("cannot specify both `--dev` and `--only-group`")
            };
            Some(GroupsSpecification::Only(only_group))
        } else {
            None
        };

        Self { dev, groups }
    }

    /// Return a new [`DevGroupsSpecification`] with development dependencies included by default.
    ///
    /// This is appropriate in projects, where the `dev` group is synced by default.
    #[must_use]
    pub fn with_defaults(self, defaults: Vec<GroupName>) -> DevGroupsManifest {
        DevGroupsManifest {
            spec: self,
            defaults,
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        self.dev.as_ref().map_or(true, DevMode::prod)
            && self.groups.as_ref().map_or(true, GroupsSpecification::prod)
    }

    /// Returns `true` if the specification is limited to a select set of groups.
    pub fn only(&self) -> bool {
        self.dev.as_ref().is_some_and(DevMode::only)
            || self.groups.as_ref().is_some_and(GroupsSpecification::only)
    }

    /// Returns the flag that was used to request development dependencies, if specified.
    pub fn dev_mode(&self) -> Option<&DevMode> {
        self.dev.as_ref()
    }

    /// Returns the list of groups to include, if specified.
    pub fn groups(&self) -> Option<&GroupsSpecification> {
        self.groups.as_ref()
    }

    /// Returns an [`Iterator`] over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        <&Self as IntoIterator>::into_iter(self)
    }
}

impl<'a> IntoIterator for &'a DevGroupsSpecification {
    type Item = &'a GroupName;
    type IntoIter = std::iter::Chain<
        std::iter::Flatten<std::option::IntoIter<&'a DevMode>>,
        std::iter::Flatten<std::option::IntoIter<&'a GroupsSpecification>>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.dev
            .as_ref()
            .into_iter()
            .flatten()
            .chain(self.groups.as_ref().into_iter().flatten())
    }
}

impl From<DevMode> for DevGroupsSpecification {
    fn from(dev: DevMode) -> Self {
        Self {
            dev: Some(dev),
            groups: None,
        }
    }
}

impl From<GroupsSpecification> for DevGroupsSpecification {
    fn from(groups: GroupsSpecification) -> Self {
        Self {
            dev: None,
            groups: Some(groups),
        }
    }
}

/// The manifest of `dependency-groups` to include, taking into account the user-provided
/// [`DevGroupsSpecification`] and the project-specific default groups.
#[derive(Debug, Clone)]
pub struct DevGroupsManifest {
    /// The specification for the development dependencies.
    pub(crate) spec: DevGroupsSpecification,
    /// The default groups to include.
    pub(crate) defaults: Vec<GroupName>,
}

impl DevGroupsManifest {
    /// Returns a new [`DevGroupsManifest`] with the given default groups.
    pub fn from_defaults(defaults: Vec<GroupName>) -> Self {
        Self {
            spec: DevGroupsSpecification::default(),
            defaults,
        }
    }

    /// Returns a new [`DevGroupsManifest`] with the given specification.
    pub fn from_spec(spec: DevGroupsSpecification) -> Self {
        Self {
            spec,
            defaults: Vec::new(),
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        self.spec.prod()
    }

    /// Returns an [`Iterator`] over the group names to include.
    pub fn iter(&self) -> impl Iterator<Item = &GroupName> {
        if self.spec.only() {
            Either::Left(self.spec.iter())
        } else {
            Either::Right(
                self.spec
                    .iter()
                    .chain(self.defaults.iter().filter(|default| {
                        // If `--no-dev` was provided, exclude the `dev` group from the list of defaults.
                        !matches!(self.spec.dev_mode(), Some(DevMode::Exclude))
                            || *default != &*DEV_DEPENDENCIES
                    })),
            )
        }
    }
}
