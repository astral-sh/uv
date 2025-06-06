use std::{borrow::Cow, sync::Arc};

use uv_normalize::{DefaultExtras, ExtraName};

/// Manager of all extra decisions and settings history.
///
/// This is an Arc mostly just to avoid size bloat on things that contain these.
#[derive(Debug, Default, Clone)]
pub struct ExtrasSpecification(Arc<ExtrasSpecificationInner>);

/// Manager of all dependency-group decisions and settings history.
#[derive(Debug, Default, Clone)]
pub struct ExtrasSpecificationInner {
    /// Extras to include.
    include: IncludeExtras,
    /// Extras to exclude (always wins over include).
    exclude: Vec<ExtraName>,
    /// Whether an `--only` flag was passed.
    ///
    /// If true, users of this API should refrain from looking at packages
    /// that *aren't* specified by the extras. This is exposed
    /// via [`ExtrasSpecificationInner::prod`][].
    only_extras: bool,
    /// The "raw" flags/settings we were passed for diagnostics.
    history: ExtrasSpecificationHistory,
}

impl ExtrasSpecification {
    /// Create from history.
    ///
    /// This is the "real" constructor, it's basically taking raw CLI flags but in
    /// a way that's a bit nicer for other constructors to use.
    fn from_history(history: ExtrasSpecificationHistory) -> Self {
        let ExtrasSpecificationHistory {
            mut extra,
            mut only_extra,
            no_extra,
            all_extras,
            no_default_extras,
            mut defaults,
        } = history.clone();

        // `extra` and `only_extra` actually have the same meanings: packages to include.
        // But if `only_extra` is non-empty then *other* packages should be excluded.
        // So we just record whether it was and then treat the two lists as equivalent.
        let only_extras = !only_extra.is_empty();
        // --only flags imply --no-default-extras
        let default_extras = !no_default_extras && !only_extras;

        let include = if all_extras {
            // If this is set we can ignore extra/only_extra/defaults as irrelevant.
            IncludeExtras::All
        } else {
            // Merge all these lists, they're equivalent now
            extra.append(&mut only_extra);
            // Resolve default extras potentially also setting All
            if default_extras {
                match &mut defaults {
                    DefaultExtras::All => IncludeExtras::All,
                    DefaultExtras::List(defaults) => {
                        extra.append(defaults);
                        IncludeExtras::Some(extra)
                    }
                }
            } else {
                IncludeExtras::Some(extra)
            }
        };

        Self(Arc::new(ExtrasSpecificationInner {
            include,
            exclude: no_extra,
            only_extras,
            history,
        }))
    }

    /// Create from raw CLI args
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn from_args(
        extra: Vec<ExtraName>,
        no_extra: Vec<ExtraName>,
        no_default_extras: bool,
        only_extra: Vec<ExtraName>,
        all_extras: bool,
    ) -> Self {
        Self::from_history(ExtrasSpecificationHistory {
            extra,
            only_extra,
            no_extra,
            all_extras,
            no_default_extras,
            // This is unknown at CLI-time, use `.with_defaults(...)` to apply this later!
            defaults: DefaultExtras::default(),
        })
    }

    /// Helper to make a spec from just a --extra
    pub fn from_extra(extra: Vec<ExtraName>) -> Self {
        Self::from_history(ExtrasSpecificationHistory {
            extra,
            ..Default::default()
        })
    }

    /// Helper to make a spec from just --all-extras
    pub fn from_all_extras() -> Self {
        Self::from_history(ExtrasSpecificationHistory {
            all_extras: true,
            ..Default::default()
        })
    }

    /// Apply defaults to a base [`ExtrasSpecification`].
    pub fn with_defaults(&self, defaults: DefaultExtras) -> ExtrasSpecificationWithDefaults {
        // Explicitly clone the inner history and set the defaults, then remake the result.
        let mut history = self.0.history.clone();
        history.defaults = defaults;

        ExtrasSpecificationWithDefaults {
            cur: Self::from_history(history),
            prev: self.clone(),
        }
    }
}

impl std::ops::Deref for ExtrasSpecification {
    type Target = ExtrasSpecificationInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ExtrasSpecificationInner {
    /// Returns `true` if packages other than the ones referenced by these
    /// extras should be considered.
    ///
    /// That is, if I tell you to install a project and this is false,
    /// you should ignore the project itself and all its dependencies,
    /// and instead just install the extras.
    ///
    /// (This is really just asking if an --only flag was passed.)
    pub fn prod(&self) -> bool {
        !self.only_extras
    }

    /// Returns `true` if the specification includes the given extra.
    pub fn contains(&self, extra: &ExtraName) -> bool {
        // exclude always trumps include
        !self.exclude.contains(extra) && self.include.contains(extra)
    }

    /// Iterate over all extras that we think should exist.
    pub fn desugarred_names(&self) -> impl Iterator<Item = &ExtraName> {
        self.include.names().chain(&self.exclude)
    }

    /// Returns `true` if the specification includes the given extra.
    pub fn extra_names<'a, Names>(
        &'a self,
        all_names: Names,
    ) -> impl Iterator<Item = &'a ExtraName> + 'a
    where
        Names: Iterator<Item = &'a ExtraName> + 'a,
    {
        all_names.filter(move |name| self.contains(name))
    }

    /// Iterate over all groups the user explicitly asked for on the CLI
    pub fn explicit_names(&self) -> impl Iterator<Item = &ExtraName> {
        let ExtrasSpecificationHistory {
            extra,
            only_extra,
            no_extra,
            // These reference no extras explicitly
            all_extras: _,
            no_default_extras: _,
            defaults: _,
        } = self.history();

        extra.iter().chain(no_extra).chain(only_extra)
    }

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        self.prod() && self.exclude.is_empty() && self.include.is_empty()
    }

    /// Get the raw history for diagnostics
    pub fn history(&self) -> &ExtrasSpecificationHistory {
        &self.history
    }
}

/// Context about a [`ExtrasSpecification`][] that we've preserved for diagnostics
#[derive(Debug, Default, Clone)]
pub struct ExtrasSpecificationHistory {
    pub extra: Vec<ExtraName>,
    pub only_extra: Vec<ExtraName>,
    pub no_extra: Vec<ExtraName>,
    pub all_extras: bool,
    pub no_default_extras: bool,
    pub defaults: DefaultExtras,
}

impl ExtrasSpecificationHistory {
    /// Returns all the CLI flags that this represents.
    ///
    /// If a flag was provided multiple times (e.g. `--extra A --extra B`) this will
    /// elide the arguments and just show the flag once (e.g. just yield "--extra").
    ///
    /// Conceptually this being an empty list should be equivalent to
    /// [`ExtrasSpecification::is_empty`][] when there aren't any defaults set.
    /// When there are defaults the two will disagree, and rightfully so!
    pub fn as_flags_pretty(&self) -> Vec<Cow<str>> {
        let ExtrasSpecificationHistory {
            extra,
            no_extra,
            all_extras,
            only_extra,
            no_default_extras,
            // defaults aren't CLI flags!
            defaults: _,
        } = self;

        let mut flags = vec![];
        if *all_extras {
            flags.push(Cow::Borrowed("--all-extras"));
        }
        if *no_default_extras {
            flags.push(Cow::Borrowed("--no-default-extras"));
        }
        match &**extra {
            [] => {}
            [extra] => flags.push(Cow::Owned(format!("--extra {extra}"))),
            [..] => flags.push(Cow::Borrowed("--extra")),
        }
        match &**only_extra {
            [] => {}
            [extra] => flags.push(Cow::Owned(format!("--only-extra {extra}"))),
            [..] => flags.push(Cow::Borrowed("--only-extra")),
        }
        match &**no_extra {
            [] => {}
            [extra] => flags.push(Cow::Owned(format!("--no-extra {extra}"))),
            [..] => flags.push(Cow::Borrowed("--no-extra")),
        }
        flags
    }
}

/// A trivial newtype wrapped around [`ExtrasSpecification`][] that signifies "defaults applied"
///
/// It includes a copy of the previous semantics to provide info on if
/// the group being a default actually affected it being enabled, because it's obviously "correct".
/// (These are Arcs so it's ~free to hold onto the previous semantics)
#[derive(Debug, Clone)]
pub struct ExtrasSpecificationWithDefaults {
    /// The active semantics
    cur: ExtrasSpecification,
    /// The semantics before defaults were applied
    prev: ExtrasSpecification,
}

impl ExtrasSpecificationWithDefaults {
    /// Returns `true` if the specification was enabled, and *only* because it was a default
    pub fn contains_because_default(&self, extra: &ExtraName) -> bool {
        self.cur.contains(extra) && !self.prev.contains(extra)
    }
}
impl std::ops::Deref for ExtrasSpecificationWithDefaults {
    type Target = ExtrasSpecification;
    fn deref(&self) -> &Self::Target {
        &self.cur
    }
}

#[derive(Debug, Clone)]
pub enum IncludeExtras {
    /// Include dependencies from the specified extras.
    Some(Vec<ExtraName>),
    /// A marker indicates including dependencies from all extras.
    All,
}

impl IncludeExtras {
    /// Returns `true` if the specification includes the given extra.
    pub fn contains(&self, extra: &ExtraName) -> bool {
        match self {
            IncludeExtras::Some(extras) => extras.contains(extra),
            IncludeExtras::All => true,
        }
    }

    /// Returns `true` if the specification will have no effect.
    pub fn is_empty(&self) -> bool {
        match self {
            IncludeExtras::Some(extras) => extras.is_empty(),
            // Although technically this is a noop if they have no extras,
            // conceptually they're *trying* to have an effect, so treat it as one.
            IncludeExtras::All => false,
        }
    }

    /// Iterate over all extras referenced in the [`IncludeExtras`].
    pub fn names(&self) -> std::slice::Iter<ExtraName> {
        match self {
            IncludeExtras::Some(extras) => extras.iter(),
            IncludeExtras::All => [].iter(),
        }
    }
}

impl Default for IncludeExtras {
    fn default() -> Self {
        Self::Some(Vec::new())
    }
}
