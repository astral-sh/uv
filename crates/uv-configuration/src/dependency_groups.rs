use std::{borrow::Cow, sync::Arc};

use uv_normalize::{GroupName, DEV_DEPENDENCIES};

/// Manager of all dependency-group decisions and settings history.
///
/// This is an Arc mostly just to avoid size bloat on things that contain these.
#[derive(Debug, Default, Clone)]
pub struct DependencyGroups(Arc<DependencyGroupsInner>);

/// Manager of all dependency-group decisions and settings history.
#[derive(Debug, Default, Clone)]
pub struct DependencyGroupsInner {
    /// Groups to include.
    include: IncludeGroups,
    /// Groups to exclude (always wins over include).
    exclude: Vec<GroupName>,
    /// Whether an `--only` flag was passed.
    ///
    /// If true, users of this API should refrain from looking at packages
    /// that *aren't* specified by the dependency-groups. This is exposed
    /// via [`DependencyGroupsInner::prod`][].
    only_groups: bool,
    /// The "raw" flags/settings we were passed for diagnostics.
    history: DependencyGroupsHistory,
}

impl DependencyGroups {
    /// Create from history.
    ///
    /// This is the "real" constructor, it's basically taking raw CLI flags but in
    /// a way that's a bit nicer for other constructors to use.
    fn from_history(history: DependencyGroupsHistory) -> Self {
        let DependencyGroupsHistory {
            dev_mode,
            mut group,
            mut only_group,
            mut no_group,
            all_groups,
            no_default_groups,
            mut defaults,
        } = history.clone();

        // First desugar --dev flags
        match dev_mode {
            Some(DevMode::Include) => group.push(DEV_DEPENDENCIES.clone()),
            Some(DevMode::Only) => only_group.push(DEV_DEPENDENCIES.clone()),
            Some(DevMode::Exclude) => no_group.push(DEV_DEPENDENCIES.clone()),
            None => {}
        }

        // `group` and `only_group` actually have the same meanings: packages to include.
        // But if `only_group` is non-empty then *other* packages should be excluded.
        // So we just record whether it was and then treat the two lists as equivalent.
        let only_groups = !only_group.is_empty();
        // --only flags imply --no-default-groups
        let default_groups = !no_default_groups && !only_groups;

        let include = if all_groups {
            // If this is set we can ignore group/only_group/defaults as irrelevant
            // (`--all-groups --only-*` is rejected at the CLI level, don't worry about it).
            IncludeGroups::All
        } else {
            // Merge all these lists, they're equivalent now
            group.append(&mut only_group);
            if default_groups {
                group.append(&mut defaults);
            }
            IncludeGroups::Some(group)
        };

        Self(Arc::new(DependencyGroupsInner {
            include,
            exclude: no_group,
            only_groups,
            history,
        }))
    }

    /// Create from raw CLI args
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn from_args(
        dev: bool,
        no_dev: bool,
        only_dev: bool,
        group: Vec<GroupName>,
        no_group: Vec<GroupName>,
        no_default_groups: bool,
        only_group: Vec<GroupName>,
        all_groups: bool,
    ) -> Self {
        // Lower the --dev flags into a single dev mode.
        //
        // In theory only one of these 3 flags should be set (enforced by CLI),
        // but we explicitly allow `--dev` and `--only-dev` to both be set,
        // and "saturate" that to `--only-dev`.
        let dev_mode = if only_dev {
            Some(DevMode::Only)
        } else if no_dev {
            Some(DevMode::Exclude)
        } else if dev {
            Some(DevMode::Include)
        } else {
            None
        };

        Self::from_history(DependencyGroupsHistory {
            dev_mode,
            group,
            only_group,
            no_group,
            all_groups,
            no_default_groups,
            // This is unknown at CLI-time, use `.with_defaults(...)` to apply this later!
            defaults: Vec::new(),
        })
    }

    /// Helper to make a spec from just a --dev flag
    pub fn from_dev_mode(dev_mode: DevMode) -> Self {
        Self::from_history(DependencyGroupsHistory {
            dev_mode: Some(dev_mode),
            ..Default::default()
        })
    }

    /// Helper to make a spec from just a --group
    pub fn from_group(group: GroupName) -> Self {
        Self::from_history(DependencyGroupsHistory {
            group: vec![group],
            ..Default::default()
        })
    }

    /// Apply defaults to a base [`DependencyGroups`].
    ///
    /// This is appropriate in projects, where the `dev` group is synced by default.
    pub fn with_defaults(&self, defaults: Vec<GroupName>) -> DependencyGroupsWithDefaults {
        // Explicitly clone the inner history and set the defaults, then remake the result.
        let mut history = self.0.history.clone();
        history.defaults = defaults;

        DependencyGroupsWithDefaults {
            cur: Self::from_history(history),
            prev: self.clone(),
        }
    }
}

impl std::ops::Deref for DependencyGroups {
    type Target = DependencyGroupsInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DependencyGroupsInner {
    /// Returns `true` if packages other than the ones referenced by these
    /// dependency-groups should be considered.
    ///
    /// That is, if I tell you to install a project and this is false,
    /// you should ignore the project itself and all its dependencies,
    /// and instead just install the dependency-groups.
    ///
    /// (This is really just asking if an --only flag was passed.)
    pub fn prod(&self) -> bool {
        !self.only_groups
    }

    /// Returns `true` if the specification includes the given group.
    pub fn contains(&self, group: &GroupName) -> bool {
        // exclude always trumps include
        !self.exclude.contains(group) && self.include.contains(group)
    }

    /// Iterate over all groups that we think should exist.
    pub fn desugarred_names(&self) -> impl Iterator<Item = &GroupName> {
        self.include.names().chain(&self.exclude)
    }

    /// Iterate over all groups the user explicitly asked for on the CLI
    pub fn explicit_names(&self) -> impl Iterator<Item = &GroupName> {
        let DependencyGroupsHistory {
            // Strictly speaking this is an explicit reference to "dev"
            // but we're currently tolerant of dev not existing when referenced with
            // these flags, since it kinda implicitly always exists even if
            // it's not properly defined in a config file.
            dev_mode: _,
            group,
            only_group,
            no_group,
            // These reference no groups explicitly
            all_groups: _,
            no_default_groups: _,
            // This doesn't include defaults because the `dev` group may not be defined
            // but gets implicitly added as a default sometimes!
            defaults: _,
        } = self.history();

        group.iter().chain(no_group).chain(only_group)
    }

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        self.prod() && self.exclude.is_empty() && self.include.is_empty()
    }

    /// Get the raw history for diagnostics
    pub fn history(&self) -> &DependencyGroupsHistory {
        &self.history
    }
}

/// Context about a [`DependencyGroups`][] that we've preserved for diagnostics
#[derive(Debug, Default, Clone)]
pub struct DependencyGroupsHistory {
    pub dev_mode: Option<DevMode>,
    pub group: Vec<GroupName>,
    pub only_group: Vec<GroupName>,
    pub no_group: Vec<GroupName>,
    pub all_groups: bool,
    pub no_default_groups: bool,
    pub defaults: Vec<GroupName>,
}

impl DependencyGroupsHistory {
    /// Returns all the CLI flags that this represents.
    ///
    /// If a flag was provided multiple times (e.g. `--group A --group B`) this will
    /// elide the arguments and just show the flag once (e.g. just yield "--group").
    ///
    /// Conceptually this being an empty list should be equivalent to
    /// [`DependencyGroups::is_empty`][] when there aren't any defaults set.
    /// When there are defaults the two will disagree, and rightfully so!
    pub fn as_flags_pretty(&self) -> Vec<Cow<str>> {
        let DependencyGroupsHistory {
            dev_mode,
            group,
            only_group,
            no_group,
            all_groups,
            no_default_groups,
            // defaults aren't CLI flags!
            defaults: _,
        } = self;

        let mut flags = vec![];
        if *all_groups {
            flags.push(Cow::Borrowed("--all-groups"));
        }
        if *no_default_groups {
            flags.push(Cow::Borrowed("--no-default-groups"));
        }
        if let Some(dev_mode) = dev_mode {
            flags.push(Cow::Borrowed(dev_mode.as_flag()));
        }
        match &**group {
            [] => {}
            [group] => flags.push(Cow::Owned(format!("--group {group}"))),
            [..] => flags.push(Cow::Borrowed("--group")),
        }
        match &**only_group {
            [] => {}
            [group] => flags.push(Cow::Owned(format!("--only-group {group}"))),
            [..] => flags.push(Cow::Borrowed("--only-group")),
        }
        match &**no_group {
            [] => {}
            [group] => flags.push(Cow::Owned(format!("--no-group {group}"))),
            [..] => flags.push(Cow::Borrowed("--no-group")),
        }
        flags
    }
}

/// A trivial newtype wrapped around [`DependencyGroups`][] that signifies "defaults applied"
///
/// It includes a copy of the previous semantics to provide info on if
/// the group being a default actually affected it being enabled, because it's obviously "correct".
/// (These are Arcs so it's ~free to hold onto the previous semantics)
#[derive(Debug, Clone)]
pub struct DependencyGroupsWithDefaults {
    /// The active semantics
    cur: DependencyGroups,
    /// The semantics before defaults were applied
    prev: DependencyGroups,
}

impl DependencyGroupsWithDefaults {
    /// Returns `true` if the specification was enabled, and *only* because it was a default
    pub fn contains_because_default(&self, group: &GroupName) -> bool {
        self.cur.contains(group) && !self.prev.contains(group)
    }
}
impl std::ops::Deref for DependencyGroupsWithDefaults {
    type Target = DependencyGroups;
    fn deref(&self) -> &Self::Target {
        &self.cur
    }
}

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
    /// Returns the flag that was used to request development dependencies.
    pub fn as_flag(&self) -> &'static str {
        match self {
            Self::Exclude => "--no-dev",
            Self::Include => "--dev",
            Self::Only => "--only-dev",
        }
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

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        match self {
            IncludeGroups::Some(groups) => groups.is_empty(),
            // Although technically this is a noop if they have no groups,
            // conceptually they're *trying* to have an effect, so treat it as one.
            IncludeGroups::All => false,
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

impl Default for IncludeGroups {
    fn default() -> Self {
        Self::Some(Vec::new())
    }
}
