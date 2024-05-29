use uv_normalize::ExtraName;

#[derive(Debug, Default, Clone)]
pub enum ExtrasSpecification {
    #[default]
    None,
    All,
    Some(Vec<ExtraName>),
}

impl ExtrasSpecification {
    /// Determine the extras specification to use based on the command-line arguments.
    pub fn from_args(all_extras: bool, extra: Vec<ExtraName>) -> Self {
        if all_extras {
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
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, ExtrasSpecification::None)
    }
}
