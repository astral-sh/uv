use std::collections::HashSet;

use uv_normalize::ExtraName;

#[derive(Debug, Default, Clone)]
pub enum ExtrasSpecification {
    #[default]
    None,
    All,
    Some(Vec<ExtraName>),
    Exclude(HashSet<ExtraName>),
}

impl ExtrasSpecification {
    /// Determine the extras specification to use based on the command-line arguments.
    pub fn from_args(all_extras: bool, no_extra: Vec<ExtraName>, extra: Vec<ExtraName>) -> Self {
        if all_extras && !no_extra.is_empty() {
            ExtrasSpecification::Exclude(HashSet::from_iter(no_extra))
        } else if all_extras {
            ExtrasSpecification::All
        } else if extra.is_empty() {
            ExtrasSpecification::None
        } else {
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

pub enum ExtrasIter<'a, Names: Iterator<Item = &'a ExtraName>> {
    None,
    All(Names),
    Some(std::slice::Iter<'a, ExtraName>),
    Exclude(Names, &'a HashSet<ExtraName>),
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
