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

    /// Returns `true` if the group is `dev`, and development dependencies should be included.
    pub fn contains(&self, group: &GroupName) -> bool {
        match self {
            DevMode::Exclude => false,
            DevMode::Include | DevMode::Only => group == &*DEV_DEPENDENCIES,
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
    /// Include dependencies from the specified groups alongside the default groups (omitting
    /// those default groups that are explicitly excluded).
    ///
    /// If the `include` is `IncludeGroups::Some`, it is guaranteed to omit groups in the `exclude`
    /// list (i.e., they have an empty intersection).
    Include {
        include: IncludeGroups,
        exclude: Vec<GroupName>,
    },
    /// Include dependencies from the specified groups, omitting any default groups.
    ///
    /// If the list is empty, no group will be included.
    Explicit { include: Vec<GroupName> },
    /// Only include dependencies from the specified groups, exclude all other dependencies.
    ///
    /// The `include` list is guaranteed to omit groups in the `exclude` list (i.e., they have an
    /// empty intersection).
    Only {
        include: Vec<GroupName>,
        exclude: Vec<GroupName>,
    },
}

impl GroupsSpecification {
    /// Create a [`GroupsSpecification`] that includes the given group.
    pub fn from_group(group: GroupName) -> Self {
        Self::Include {
            include: IncludeGroups::Some(vec![group]),
            exclude: Vec::new(),
        }
    }

    /// Returns `true` if the specification allows for production dependencies.
    pub fn prod(&self) -> bool {
        matches!(self, Self::Include { .. } | Self::Explicit { .. })
    }

    /// Returns `true` if the specification is limited to a select set of groups.
    pub fn only(&self) -> bool {
        matches!(self, Self::Only { .. })
    }

    /// Returns the option that was used to request the groups, if any.
    pub fn as_flag(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Include { include, exclude } => match include {
                IncludeGroups::All => Some(Cow::Borrowed("--all-groups")),
                IncludeGroups::Some(groups) => match groups.as_slice() {
                    [] => match exclude.as_slice() {
                        [] => None,
                        [group] => Some(Cow::Owned(format!("--no-group {group}"))),
                        [..] => Some(Cow::Borrowed("--no-group")),
                    },
                    [group] => Some(Cow::Owned(format!("--group {group}"))),
                    [..] => Some(Cow::Borrowed("--group")),
                },
            },
            Self::Only { include, exclude } => match include.as_slice() {
                [] => match exclude.as_slice() {
                    [] => None,
                    [group] => Some(Cow::Owned(format!("--no-group {group}"))),
                    [..] => Some(Cow::Borrowed("--no-group")),
                },
                [group] => Some(Cow::Owned(format!("--only-group {group}"))),
                [..] => Some(Cow::Borrowed("--only-group")),
            },
            Self::Explicit { include } => match include.as_slice() {
                [] => Some(Cow::Borrowed("--no-default-groups")),
                [group] => Some(Cow::Owned(format!("--group {group}"))),
                [..] => Some(Cow::Borrowed("--group")),
            },
        }
    }

    /// Iterate over all groups referenced in the [`GroupsSpecification`].
    pub fn names(&self) -> impl Iterator<Item = &GroupName> {
        match self {
            GroupsSpecification::Include { include, exclude } => {
                Either::Left(include.names().chain(exclude.iter()))
            }
            GroupsSpecification::Only { include, exclude } => {
                Either::Left(include.iter().chain(exclude.iter()))
            }
            GroupsSpecification::Explicit { include } => Either::Right(include.iter()),
        }
    }

    /// Returns `true` if the specification includes the given group.
    pub fn contains(&self, group: &GroupName) -> bool {
        match self {
            GroupsSpecification::Include { include, exclude } => {
                // For `--all-groups`, the group is included unless it is explicitly excluded.
                include.contains(group) && !exclude.contains(group)
            }
            GroupsSpecification::Only { include, .. } => include.contains(group),
            GroupsSpecification::Explicit { include } => include.contains(group),
        }
    }

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        let GroupsSpecification::Include {
            include: IncludeGroups::Some(includes),
            exclude,
        } = self
        else {
            return false;
        };
        includes.is_empty() && exclude.is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum IncludeGroups {
    /// Include dependencies from the specified groups.
    Some(Vec<GroupName>),
    /// A marker indicates including dependencies from all groups.
    All,
}

impl IncludeGroups {
    /// Returns `true` if the specification includes the given group.
    pub fn contains(&self, group: &GroupName) -> bool {
        match self {
            IncludeGroups::Some(groups) => groups.contains(group),
            IncludeGroups::All => true,
        }
    }

    /// Iterate over all groups referenced in the [`IncludeGroups`].
    pub fn names(&self) -> std::slice::Iter<GroupName> {
        match self {
            IncludeGroups::Some(groups) => groups.iter(),
            IncludeGroups::All => [].iter(),
        }
    }
}

impl DevGroupsSpecification {
    /// Determine the [`DevGroupsSpecification`] policy from the command-line arguments.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn from_args(
        dev: bool,
        no_dev: bool,
        only_dev: bool,
        mut group: Vec<GroupName>,
        no_group: Vec<GroupName>,
        no_default_groups: bool,
        mut only_group: Vec<GroupName>,
        all_groups: bool,
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

        let groups = if no_default_groups {
            // Remove groups specified with `--no-group`.
            group.retain(|group| !no_group.contains(group));

            Some(GroupsSpecification::Explicit { include: group })
        } else if all_groups {
            Some(GroupsSpecification::Include {
                include: IncludeGroups::All,
                exclude: no_group,
            })
        } else if !group.is_empty() {
            if matches!(dev, Some(DevMode::Only)) {
                unreachable!("cannot specify both `--only-dev` and `--group`")
            };

            // Ensure that `--no-group` and `--group` are mutually exclusive.
            group.retain(|group| !no_group.contains(group));

            Some(GroupsSpecification::Include {
                include: IncludeGroups::Some(group),
                exclude: no_group,
            })
        } else if !only_group.is_empty() {
            if matches!(dev, Some(DevMode::Include)) {
                unreachable!("cannot specify both `--dev` and `--only-group`")
            };

            // Ensure that `--no-group` and `--only-group` are mutually exclusive.
            only_group.retain(|group| !no_group.contains(group));

            Some(GroupsSpecification::Only {
                include: only_group,
                exclude: no_group,
            })
        } else if !no_group.is_empty() {
            Some(GroupsSpecification::Include {
                include: IncludeGroups::Some(Vec::new()),
                exclude: no_group,
            })
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

    /// Returns `true` if the group is included in the specification.
    pub fn contains(&self, group: &GroupName) -> bool {
        if group == &*DEV_DEPENDENCIES {
            match self.dev.as_ref() {
                None => {}
                Some(DevMode::Exclude) => {
                    // If `--no-dev` was provided, always exclude dev.
                    return false;
                }
                Some(DevMode::Only) => {
                    // If `--only-dev` was provided, always include dev.
                    return true;
                }
                Some(DevMode::Include) => {
                    // If `--no-group dev` was provided, exclude dev.
                    return match self.groups.as_ref() {
                        Some(GroupsSpecification::Include { exclude, .. }) => {
                            !exclude.contains(group)
                        }
                        _ => true,
                    };
                }
            }
        }

        self.groups
            .as_ref()
            .is_some_and(|groups| groups.contains(group))
    }

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        let groups_empty = self
            .groups
            .as_ref()
            .map(GroupsSpecification::is_empty)
            .unwrap_or(true);
        let dev_empty = self.dev_mode().is_none();
        groups_empty && dev_empty
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
#[derive(Debug, Default, Clone)]
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

    /// Returns `true` if the group was enabled by default.
    pub fn is_default(&self, group: &GroupName) -> bool {
        if self.spec.contains(group) {
            // If the group was explicitly requested, then it wasn't enabled by default.
            false
        } else {
            // If the group was enabled, but wasn't explicitly requested, then it was enabled by
            // default.
            self.contains(group)
        }
    }

    /// Returns `true` if the group is included in the manifest.
    pub fn contains(&self, group: &GroupName) -> bool {
        if self.spec.contains(group) {
            return true;
        }
        if self.spec.only() {
            return false;
        }
        self.defaults
            .iter()
            .filter(|default| {
                // If `--no-dev` was provided, exclude the `dev` group from the list of defaults.
                if matches!(self.spec.dev_mode(), Some(DevMode::Exclude)) {
                    if *default == &*DEV_DEPENDENCIES {
                        return false;
                    };
                }

                // If `--no-default-groups` was provided, only include group if it's explicitly
                // included with `--group <group>`.
                if let Some(GroupsSpecification::Explicit { include }) = self.spec.groups() {
                    return include.contains(group);
                }

                // If `--no-group` was provided, exclude the group from the list of defaults.
                if let Some(GroupsSpecification::Include {
                    include: _,
                    exclude,
                }) = self.spec.groups()
                {
                    if exclude.contains(default) {
                        return false;
                    }
                }

                true
            })
            .any(|default| default == group)
    }
}
