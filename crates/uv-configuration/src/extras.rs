use rustc_hash::FxHashSet;
use uv_normalize::ExtraName;

#[derive(Debug, Default, Clone)]
pub enum ExtrasSpecification {
    #[default]
    None,
    All,
    Some(Vec<ExtraName>),
    Exclude(FxHashSet<ExtraName>),
}

impl ExtrasSpecification {
    /// Determine the extras specification to use based on the command-line arguments.
    pub fn from_args(
        all_extras: bool,
        no_extra: Vec<ExtraName>,
        mut extra: Vec<ExtraName>,
    ) -> Self {
        if all_extras && !no_extra.is_empty() {
            ExtrasSpecification::Exclude(FxHashSet::from_iter(no_extra))
        } else if all_extras {
            ExtrasSpecification::All
        } else if extra.is_empty() {
            ExtrasSpecification::None
        } else {
            // If a package is included in both `no_extra` and `extra`, it should be excluded.
            extra.retain(|name| !no_extra.contains(name));
            ExtrasSpecification::Some(extra)
        }
    }

    /// Returns true if a name is included in the extra specification.
    pub fn contains(&self, name: &ExtraName) -> bool {
        match self {
            ExtrasSpecification::All => true,
            ExtrasSpecification::None => false,
            ExtrasSpecification::Some(extras) => extras.contains(name),
            ExtrasSpecification::Exclude(excluded) => !excluded.contains(name),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, ExtrasSpecification::None)
    }

    pub fn extra_names<'a, Names>(&'a self, all_names: Names) -> ExtrasIter<'a, Names>
    where
        Names: Iterator<Item = &'a ExtraName>,
    {
        match self {
            ExtrasSpecification::All => ExtrasIter::All(all_names),
            ExtrasSpecification::None => ExtrasIter::None,
            ExtrasSpecification::Some(extras) => ExtrasIter::Some(extras.iter()),
            ExtrasSpecification::Exclude(excluded) => ExtrasIter::Exclude(all_names, excluded),
        }
    }
}

/// An iterator over the extra names to include.
#[derive(Debug)]
pub enum ExtrasIter<'a, Names: Iterator<Item = &'a ExtraName>> {
    None,
    All(Names),
    Some(std::slice::Iter<'a, ExtraName>),
    Exclude(Names, &'a FxHashSet<ExtraName>),
}

impl<'a, Names: Iterator<Item = &'a ExtraName>> Iterator for ExtrasIter<'a, Names> {
    type Item = &'a ExtraName;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::All(names) => names.next(),
            Self::None => None,
            Self::Some(extras) => extras.next(),
            Self::Exclude(names, excluded) => {
                for name in names.by_ref() {
                    if !excluded.contains(name) {
                        return Some(name);
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! extras {
        () => (
            Vec::new()
        );
        ($($x:expr),+ $(,)?) => (
            vec![$(ExtraName::from_owned($x.into()).unwrap()),+]
        )
    }

    #[test]
    fn test_no_extra_full() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras!["dev", "docs", "extra-1", "extra-2"];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras![]);
    }

    #[test]
    fn test_no_extra_partial() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras!["extra-1", "extra-2"];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras!["dev", "docs"]);
    }

    #[test]
    fn test_no_extra_empty() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras![];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras!["dev", "docs", "extra-1", "extra-2"]);
    }

    #[test]
    fn test_no_extra_excessive() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras!["does-not-exists"];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras!["dev", "docs", "extra-1", "extra-2"]);
    }

    #[test]
    fn test_no_extra_without_all_extras() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras!["extra-1", "extra-2"];
        let spec = ExtrasSpecification::from_args(false, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras![]);
    }

    #[test]
    fn test_no_extra_without_package_extras() {
        let pkg_extras = extras![];
        let no_extra = extras!["extra-1", "extra-2"];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras![]);
    }

    #[test]
    fn test_no_extra_duplicates() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-1", "extra-2"];
        let no_extra = extras!["extra-1", "extra-2"];
        let spec = ExtrasSpecification::from_args(true, no_extra, vec![]);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras!["dev", "docs"]);
    }

    #[test]
    fn test_no_extra_extra() {
        let pkg_extras = extras!["dev", "docs", "extra-1", "extra-2"];
        let no_extra = extras!["extra-1", "extra-2"];
        let extra = extras!["extra-1", "extra-2", "docs"];
        let spec = ExtrasSpecification::from_args(false, no_extra, extra);
        let result: Vec<_> = spec.extra_names(pkg_extras.iter()).cloned().collect();
        assert_eq!(result, extras!["docs"]);
    }
}
