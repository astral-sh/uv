use uv_normalize::{ExtraName, PackageName};

/// A list of conflicting groups pre-defined by an end user.
///
/// This is useful to force the resolver to fork according to extras that have
/// unavoidable conflicts with each other. (The alternative is that resolution
/// will fail.)
#[derive(
    Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
)]
pub struct ConflictingGroupList(Vec<ConflictingGroups>);

impl ConflictingGroupList {
    /// Returns no conflicting groups.
    ///
    /// This results in no effect on resolution.
    pub fn empty() -> ConflictingGroupList {
        ConflictingGroupList::default()
    }

    /// Push a set of conflicting groups.
    pub fn push(&mut self, groups: ConflictingGroups) {
        self.0.push(groups);
    }

    /// Returns an iterator over all sets of conflicting groups.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictingGroups> + '_ {
        self.0.iter()
    }

    /// Returns true if this conflicting group list contains any conflicting
    /// group set that contains the given package and extra name pair.
    pub fn contains(&self, package: &PackageName, extra: &ExtraName) -> bool {
        self.iter().any(|groups| groups.contains(package, extra))
    }

    /// Returns true if this set of conflicting groups is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A single set of package-extra pairs that conflict with one another.
///
/// Within each set of conflicting groups, the resolver should isolate
/// the requirements corresponding to each extra from the requirements of
/// other extras in this set. That is, the resolver should put each set of
/// requirements in a different fork.
///
/// A `TryFrom<Vec<ConflictingGroup>>` impl may be used to build a set
/// from a sequence. Note though that at least 2 groups are required.
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct ConflictingGroups(Vec<ConflictingGroup>);

impl ConflictingGroups {
    /// Create a pair of groups that conflict with one another.
    pub fn pair(group1: ConflictingGroup, group2: ConflictingGroup) -> ConflictingGroups {
        ConflictingGroups(vec![group1, group2])
    }

    /// Add a new conflicting group to this set.
    pub fn push(&mut self, group: ConflictingGroup) {
        self.0.push(group);
    }

    /// Returns an iterator over all conflicting groups.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictingGroup> + '_ {
        self.0.iter()
    }

    /// Returns true if this conflicting group contains the given
    /// package and extra name pair.
    pub fn contains(&self, package: &PackageName, extra: &ExtraName) -> bool {
        self.iter()
            .any(|group| group.package() == package && group.extra() == extra)
    }
}

impl<'de> serde::Deserialize<'de> for ConflictingGroups {
    fn deserialize<D>(deserializer: D) -> Result<ConflictingGroups, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let groups = Vec::<ConflictingGroup>::deserialize(deserializer)?;
        Self::try_from(groups).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Vec<ConflictingGroup>> for ConflictingGroups {
    type Error = ConflictingGroupError;

    fn try_from(groups: Vec<ConflictingGroup>) -> Result<ConflictingGroups, ConflictingGroupError> {
        match groups.len() {
            0 => return Err(ConflictingGroupError::ZeroGroups),
            1 => return Err(ConflictingGroupError::OneGroup),
            _ => {}
        }
        Ok(ConflictingGroups(groups))
    }
}

/// A single item in a set conflicting groups.
///
/// Each item is a pair of a package and a corresponding extra name for that
/// package.
#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    Hash,
    PartialEq,
    PartialOrd,
    Ord,
    serde::Deserialize,
    serde::Serialize,
    schemars::JsonSchema,
)]
pub struct ConflictingGroup {
    package: PackageName,
    extra: ExtraName,
}

impl ConflictingGroup {
    /// Returns the package name of this conflicting group.
    pub fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the extra name of this conflicting group.
    pub fn extra(&self) -> &ExtraName {
        &self.extra
    }

    /// Returns this group as a new type with its fields borrowed.
    pub fn as_ref(&self) -> ConflictingGroupRef<'_> {
        ConflictingGroupRef {
            package: self.package(),
            extra: self.extra(),
        }
    }
}

impl From<(PackageName, ExtraName)> for ConflictingGroup {
    fn from((package, extra): (PackageName, ExtraName)) -> ConflictingGroup {
        ConflictingGroup { package, extra }
    }
}

/// A single item in a set conflicting groups, by reference.
///
/// Each item is a pair of a package and a corresponding extra name for that
/// package.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ConflictingGroupRef<'a> {
    package: &'a PackageName,
    extra: &'a ExtraName,
}

impl<'a> ConflictingGroupRef<'a> {
    /// Returns the package name of this conflicting group.
    pub fn package(&self) -> &'a PackageName {
        self.package
    }

    /// Returns the extra name of this conflicting group.
    pub fn extra(&self) -> &'a ExtraName {
        self.extra
    }

    /// Converts this borrowed conflicting group to its owned variant.
    pub fn to_owned(&self) -> ConflictingGroup {
        ConflictingGroup {
            package: self.package().clone(),
            extra: self.extra().clone(),
        }
    }
}

impl<'a> From<(&'a PackageName, &'a ExtraName)> for ConflictingGroupRef<'a> {
    fn from((package, extra): (&'a PackageName, &'a ExtraName)) -> ConflictingGroupRef<'a> {
        ConflictingGroupRef { package, extra }
    }
}

/// An error that occurs when the given conflicting groups are invalid somehow.
#[derive(Debug, thiserror::Error)]
pub enum ConflictingGroupError {
    /// An error for when there are zero conflicting groups.
    #[error("Each set of conflicting groups must have at least two entries, but found none")]
    ZeroGroups,
    /// An error for when there is one conflicting group.
    #[error("Each set of conflicting groups must have at least two entries, but found only one")]
    OneGroup,
}
