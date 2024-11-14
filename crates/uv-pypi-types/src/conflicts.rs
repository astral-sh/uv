use uv_normalize::{ExtraName, PackageName};

/// A list of conflicting groups pre-defined by an end user.
///
/// This is useful to force the resolver to fork according to extras that have
/// unavoidable conflicts with each other. (The alternative is that resolution
/// will fail.)
#[derive(
    Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
)]
pub struct Conflicts(Vec<ConflictSet>);

impl Conflicts {
    /// Returns no conflicting groups.
    ///
    /// This results in no effect on resolution.
    pub fn empty() -> Conflicts {
        Conflicts::default()
    }

    /// Push a single set of conflicts.
    pub fn push(&mut self, set: ConflictSet) {
        self.0.push(set);
    }

    /// Returns an iterator over all sets of conflicting sets.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictSet> + '_ {
        self.0.iter()
    }

    /// Returns true if these conflicts contain any set that contains the given
    /// package and extra name pair.
    pub fn contains(&self, package: &PackageName, extra: &ExtraName) -> bool {
        self.iter().any(|groups| groups.contains(package, extra))
    }

    /// Returns true if there are no conflicts.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends the given conflicts to this one. This drains all sets from the
    /// conflicts given, such that after this call, it is empty.
    pub fn append(&mut self, other: &mut Conflicts) {
        self.0.append(&mut other.0);
    }
}

/// A single set of package-extra pairs that conflict with one another.
///
/// Within each set of conflicts, the resolver should isolate the requirements
/// corresponding to each extra from the requirements of other extras in
/// this set. That is, the resolver should put each set of requirements in a
/// different fork.
///
/// A `TryFrom<Vec<ConflictItem>>` impl may be used to build a set from a
/// sequence. Note though that at least 2 items are required.
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct ConflictSet(Vec<ConflictItem>);

impl ConflictSet {
    /// Create a pair of items that conflict with one another.
    pub fn pair(item1: ConflictItem, item2: ConflictItem) -> ConflictSet {
        ConflictSet(vec![item1, item2])
    }

    /// Add a new conflicting item to this set.
    pub fn push(&mut self, item: ConflictItem) {
        self.0.push(item);
    }

    /// Returns an iterator over all conflicting items.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictItem> + '_ {
        self.0.iter()
    }

    /// Returns true if this conflicting item contains the given package and
    /// extra name pair.
    pub fn contains(&self, package: &PackageName, extra: &ExtraName) -> bool {
        self.iter()
            .any(|group| group.package() == package && group.extra() == extra)
    }
}

impl<'de> serde::Deserialize<'de> for ConflictSet {
    fn deserialize<D>(deserializer: D) -> Result<ConflictSet, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let groups = Vec::<ConflictItem>::deserialize(deserializer)?;
        Self::try_from(groups).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Vec<ConflictItem>> for ConflictSet {
    type Error = ConflictError;

    fn try_from(items: Vec<ConflictItem>) -> Result<ConflictSet, ConflictError> {
        match items.len() {
            0 => return Err(ConflictError::ZeroItems),
            1 => return Err(ConflictError::OneItem),
            _ => {}
        }
        Ok(ConflictSet(items))
    }
}

/// A single item in a conflicting set.
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
pub struct ConflictItem {
    package: PackageName,
    extra: ExtraName,
}

impl ConflictItem {
    /// Returns the package name of this conflicting item.
    pub fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the extra name of this conflicting item.
    pub fn extra(&self) -> &ExtraName {
        &self.extra
    }

    /// Returns this item as a new type with its fields borrowed.
    pub fn as_ref(&self) -> ConflictItemRef<'_> {
        ConflictItemRef {
            package: self.package(),
            extra: self.extra(),
        }
    }
}

impl From<(PackageName, ExtraName)> for ConflictItem {
    fn from((package, extra): (PackageName, ExtraName)) -> ConflictItem {
        ConflictItem { package, extra }
    }
}

/// A single item in a conflicting set, by reference.
///
/// Each item is a pair of a package and a corresponding extra name for that
/// package.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ConflictItemRef<'a> {
    package: &'a PackageName,
    extra: &'a ExtraName,
}

impl<'a> ConflictItemRef<'a> {
    /// Returns the package name of this conflicting item.
    pub fn package(&self) -> &'a PackageName {
        self.package
    }

    /// Returns the extra name of this conflicting item.
    pub fn extra(&self) -> &'a ExtraName {
        self.extra
    }

    /// Converts this borrowed conflicting item to its owned variant.
    pub fn to_owned(&self) -> ConflictItem {
        ConflictItem {
            package: self.package().clone(),
            extra: self.extra().clone(),
        }
    }
}

impl<'a> From<(&'a PackageName, &'a ExtraName)> for ConflictItemRef<'a> {
    fn from((package, extra): (&'a PackageName, &'a ExtraName)) -> ConflictItemRef<'a> {
        ConflictItemRef { package, extra }
    }
}

/// An error that occurs when the given conflicting set is invalid somehow.
#[derive(Debug, thiserror::Error)]
pub enum ConflictError {
    /// An error for when there are zero conflicting items.
    #[error("Each set of conflicting groups must have at least two entries, but found none")]
    ZeroItems,
    /// An error for when there is one conflicting items.
    #[error("Each set of conflicting groups must have at least two entries, but found only one")]
    OneItem,
}

/// Like [`Conflicts`], but for deserialization in `pyproject.toml`.
///
/// The schema format is different from the in-memory format. Specifically, the
/// schema format does not allow specifying the package name (or will make it
/// optional in the future), where as the in-memory format needs the package
/// name.
///
/// N.B. `Conflicts` is still used for (de)serialization. Specifically, in the
/// lock file, where the package name is required.
#[derive(
    Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
)]
pub struct SchemaConflicts(Vec<SchemaConflictSet>);

impl SchemaConflicts {
    /// Convert the public schema "conflicting" type to our internal fully
    /// resolved type. Effectively, this pairs the corresponding package name
    /// with each conflict.
    ///
    /// If a conflict has an explicit package name (written by the end user),
    /// then that takes precedence over the given package name, which is only
    /// used when there is no explicit package name written.
    pub fn to_conflicting_with_package_name(&self, package: &PackageName) -> Conflicts {
        let mut conflicting = Conflicts::empty();
        for tool_uv_set in &self.0 {
            let mut set = vec![];
            for item in &tool_uv_set.0 {
                let package = item.package.clone().unwrap_or_else(|| package.clone());
                set.push(ConflictItem::from((package, item.extra.clone())));
            }
            // OK because we guarantee that
            // `SchemaConflictingGroupList` is valid and there aren't
            // any new errors that can occur here.
            let set = ConflictSet::try_from(set).unwrap();
            conflicting.push(set);
        }
        conflicting
    }
}

/// Like [`ConflictSet`], but for deserialization in `pyproject.toml`.
///
/// The schema format is different from the in-memory format. Specifically, the
/// schema format does not allow specifying the package name (or will make it
/// optional in the future), where as the in-memory format needs the package
/// name.
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct SchemaConflictSet(Vec<SchemaConflictItem>);

/// Like [`ConflictItem`], but for deserialization in `pyproject.toml`.
///
/// The schema format is different from the in-memory format. Specifically, the
/// schema format does not allow specifying the package name (or will make it
/// optional in the future), where as the in-memory format needs the package
/// name.
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
#[serde(deny_unknown_fields)]
pub struct SchemaConflictItem {
    #[serde(default)]
    package: Option<PackageName>,
    extra: ExtraName,
}

impl<'de> serde::Deserialize<'de> for SchemaConflictSet {
    fn deserialize<D>(deserializer: D) -> Result<SchemaConflictSet, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let items = Vec::<SchemaConflictItem>::deserialize(deserializer)?;
        Self::try_from(items).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Vec<SchemaConflictItem>> for SchemaConflictSet {
    type Error = ConflictError;

    fn try_from(items: Vec<SchemaConflictItem>) -> Result<SchemaConflictSet, ConflictError> {
        match items.len() {
            0 => return Err(ConflictError::ZeroItems),
            1 => return Err(ConflictError::OneItem),
            _ => {}
        }
        Ok(SchemaConflictSet(items))
    }
}
