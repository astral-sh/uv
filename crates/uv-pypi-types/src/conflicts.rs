use petgraph::{
    algo::toposort,
    graph::{DiGraph, NodeIndex},
};
use rustc_hash::{FxHashMap, FxHashSet};
#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::{collections::BTreeSet, hash::Hash, rc::Rc};
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::dependency_groups::{DependencyGroupSpecifier, DependencyGroups};

/// A list of conflicting sets of extras/groups pre-defined by an end user.
///
/// This is useful to force the resolver to fork according to extras that have
/// unavoidable conflicts with each other. (The alternative is that resolution
/// will fail.)
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct Conflicts(Vec<ConflictSet>);

impl Conflicts {
    /// Returns no conflicts.
    ///
    /// This results in no effect on resolution.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Push a single set of conflicts.
    pub fn push(&mut self, set: ConflictSet) {
        self.0.push(set);
    }

    /// Returns an iterator over all sets of conflicting sets.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictSet> + Clone + '_ {
        self.0.iter()
    }

    /// Returns true if these conflicts contain any set that contains the given
    /// package and extra name pair.
    pub fn contains<'a>(
        &self,
        package: &PackageName,
        conflict: impl Into<ConflictPackageRef<'a>>,
    ) -> bool {
        let conflict = conflict.into();
        self.iter().any(|set| set.contains(package, conflict))
    }

    /// Returns true if there are no conflicts.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends the given conflicts to this one. This drains all sets from the
    /// conflicts given, such that after this call, it is empty.
    pub fn append(&mut self, other: &mut Self) {
        self.0.append(&mut other.0);
    }

    /// Expand [`Conflicts`]s to include all [`ConflictSet`]s that can
    /// be transitively inferred from group conflicts directly defined
    /// in configuration.
    ///
    /// A directed acyclic graph (DAG) is created representing all
    /// transitive group includes, with nodes corresponding to group conflict
    /// items. For every conflict item directly mentioned in configuration,
    /// its node starts with a set of canonical items with itself as the only
    /// member.
    ///
    /// The graph is traversed one node at a time in topological order and
    /// canonical items are propagated to each neighbor. We also update our
    /// substitutions at each neighbor to reflect that this neighbor transitively
    /// includes all canonical items visited so far to reach it.
    ///
    /// Finally, we apply the substitutions to the conflict sets that were
    /// directly defined in configuration to generate all transitively inferable
    /// [`ConflictSet`]s.
    ///
    /// There is an assumption that inclusion graphs will not be very large
    /// or complex. This algorithm creates all combinations of substitutions.
    /// Each resulting [`ConflictSet`] would also later correspond to a separate
    /// resolver fork during resolution.
    pub fn expand_transitive_group_includes(
        &mut self,
        package: &PackageName,
        groups: &DependencyGroups,
    ) {
        let mut graph = DiGraph::new();
        let mut group_node_idxs: FxHashMap<&GroupName, NodeIndex> = FxHashMap::default();
        let mut node_conflict_items: FxHashMap<NodeIndex, Rc<ConflictItem>> = FxHashMap::default();
        // Used for transitively deriving new conflict sets with substitutions.
        // The keys are canonical items (mentioned directly in configured conflicts).
        // The values correspond to groups that transitively include them.
        let mut substitutions: FxHashMap<Rc<ConflictItem>, FxHashSet<Rc<ConflictItem>>> =
            FxHashMap::default();

        // Conflict sets that were directly defined in configuration.
        let mut direct_conflict_sets: FxHashSet<&ConflictSet> = FxHashSet::default();
        // Conflict sets that we will transitively infer in this method.
        let mut transitive_conflict_sets: FxHashSet<ConflictSet> = FxHashSet::default();

        // Add groups in directly defined conflict sets to the graph.
        let mut seen: FxHashSet<&GroupName> = FxHashSet::default();

        for set in &self.0 {
            direct_conflict_sets.insert(set);
            for item in set.iter() {
                let ConflictPackage::Group(group) = &item.conflict else {
                    // TODO(john): Do we also want to handle extras here?
                    continue;
                };
                if !seen.insert(group) {
                    continue;
                }
                let item = Rc::new(item.clone());
                let mut canonical_items = FxHashSet::default();
                canonical_items.insert(item.clone());
                let node_id = graph.add_node(canonical_items);
                group_node_idxs.insert(group, node_id);
                node_conflict_items.insert(node_id, item.clone());
            }
        }

        // Create conflict items for remaining groups and add them to the graph.
        for group in groups.keys() {
            if !seen.insert(group) {
                continue;
            }
            let group_conflict_item = ConflictItem {
                package: package.clone(),
                conflict: ConflictPackage::Group(group.clone()),
            };
            let node_id = graph.add_node(FxHashSet::default());
            group_node_idxs.insert(group, node_id);
            node_conflict_items.insert(node_id, Rc::new(group_conflict_item));
        }

        // Create edges representing group inclusion (with edges reversed so that
        // included groups point to including groups).
        for (group, specifiers) in groups {
            if let Some(includer) = group_node_idxs.get(group) {
                for specifier in specifiers {
                    if let DependencyGroupSpecifier::IncludeGroup { include_group } = specifier {
                        if let Some(included) = group_node_idxs.get(include_group) {
                            graph.add_edge(*included, *includer, ());
                        }
                    }
                }
            }
        }

        let Ok(topo_nodes) = toposort(&graph, None) else {
            return;
        };
        // Propagate canonical items through the graph and populate substitutions.
        for node in topo_nodes {
            for neighbor_idx in graph.neighbors(node).collect::<Vec<_>>() {
                let mut neighbor_canonical_items = Vec::new();
                if let Some(canonical_items) = graph.node_weight(node) {
                    let neighbor_item = node_conflict_items
                        .get(&neighbor_idx)
                        .expect("ConflictItem should already be in graph")
                        .clone();
                    for canonical_item in canonical_items {
                        neighbor_canonical_items.push(canonical_item.clone());
                        substitutions
                            .entry(canonical_item.clone())
                            .or_default()
                            .insert(neighbor_item.clone());
                    }
                }
                graph
                    .node_weight_mut(neighbor_idx)
                    .expect("Graph node should have weight")
                    .extend(neighbor_canonical_items.into_iter());
            }
        }

        // Create new conflict sets for all possible replacements of canonical
        // items by substitution items.
        // Note that new sets are (potentially) added to transitive_conflict_sets
        // at the end of each iteration.
        for (canonical_item, subs) in substitutions {
            let mut new_conflict_sets = FxHashSet::default();
            for conflict_set in direct_conflict_sets
                .iter()
                .copied()
                .chain(transitive_conflict_sets.iter())
                .filter(|set| set.contains_item(&canonical_item))
            {
                for sub in &subs {
                    let mut new_set = conflict_set
                        .replaced_item(&canonical_item, (**sub).clone())
                        .expect("`ConflictItem` should be in `ConflictSet`");
                    if !direct_conflict_sets.contains(&new_set) {
                        new_set = new_set.with_inferred_conflict();
                        if !transitive_conflict_sets.contains(&new_set) {
                            new_conflict_sets.insert(new_set);
                        }
                    }
                }
            }
            transitive_conflict_sets.extend(new_conflict_sets.into_iter());
        }

        self.0.extend(transitive_conflict_sets);
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
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct ConflictSet {
    set: BTreeSet<ConflictItem>,
    is_inferred_conflict: bool,
}

impl ConflictSet {
    /// Create a pair of items that conflict with one another.
    pub fn pair(item1: ConflictItem, item2: ConflictItem) -> Self {
        Self {
            set: BTreeSet::from_iter(vec![item1, item2]),
            is_inferred_conflict: false,
        }
    }

    /// Returns an iterator over all conflicting items.
    pub fn iter(&self) -> impl Iterator<Item = &'_ ConflictItem> + Clone + '_ {
        self.set.iter()
    }

    /// Returns true if this conflicting item contains the given package and
    /// extra name pair.
    pub fn contains<'a>(
        &self,
        package: &PackageName,
        conflict: impl Into<ConflictPackageRef<'a>>,
    ) -> bool {
        let conflict = conflict.into();
        self.iter()
            .any(|set| set.package() == package && *set.conflict() == conflict)
    }

    /// Returns true if these conflicts contain any set that contains the given
    /// [`ConflictItem`].
    pub fn contains_item(&self, conflict_item: &ConflictItem) -> bool {
        self.set.contains(conflict_item)
    }

    /// This [`ConflictSet`] was inferred from directly defined conflicts.
    pub fn is_inferred_conflict(&self) -> bool {
        self.is_inferred_conflict
    }

    /// Replace an old [`ConflictItem`] with a new one.
    pub fn replaced_item(
        &self,
        old: &ConflictItem,
        new: ConflictItem,
    ) -> Result<Self, ConflictError> {
        let mut new_set = self.set.clone();
        if !new_set.contains(old) {
            return Err(ConflictError::ReplaceMissingConflictItem);
        }
        new_set.remove(old);
        new_set.insert(new);
        Ok(Self {
            set: new_set,
            is_inferred_conflict: false,
        })
    }

    /// Mark this [`ConflictSet`] as being inferred from directly
    /// defined conflicts.
    fn with_inferred_conflict(mut self) -> Self {
        self.is_inferred_conflict = true;
        self
    }
}

impl<'de> serde::Deserialize<'de> for ConflictSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let set = Vec::<ConflictItem>::deserialize(deserializer)?;
        Self::try_from(set).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Vec<ConflictItem>> for ConflictSet {
    type Error = ConflictError;

    fn try_from(items: Vec<ConflictItem>) -> Result<Self, ConflictError> {
        match items.len() {
            0 => return Err(ConflictError::ZeroItems),
            1 => return Err(ConflictError::OneItem),
            _ => {}
        }
        Ok(Self {
            set: BTreeSet::from_iter(items),
            is_inferred_conflict: false,
        })
    }
}

/// A single item in a conflicting set.
///
/// Each item is a pair of a package and a corresponding extra or group name
/// for that package.
#[derive(
    Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(
    deny_unknown_fields,
    try_from = "ConflictItemWire",
    into = "ConflictItemWire"
)]
pub struct ConflictItem {
    package: PackageName,
    conflict: ConflictPackage,
}

impl ConflictItem {
    /// Returns the package name of this conflicting item.
    pub fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the package-specific conflict.
    ///
    /// i.e., Either an extra or a group name.
    pub fn conflict(&self) -> &ConflictPackage {
        &self.conflict
    }

    /// Returns the extra name of this conflicting item.
    pub fn extra(&self) -> Option<&ExtraName> {
        self.conflict.extra()
    }

    /// Returns the group name of this conflicting item.
    pub fn group(&self) -> Option<&GroupName> {
        self.conflict.group()
    }

    /// Returns this item as a new type with its fields borrowed.
    pub fn as_ref(&self) -> ConflictItemRef<'_> {
        ConflictItemRef {
            package: self.package(),
            conflict: self.conflict.as_ref(),
        }
    }
}

impl From<(PackageName, ExtraName)> for ConflictItem {
    fn from((package, extra): (PackageName, ExtraName)) -> Self {
        let conflict = ConflictPackage::Extra(extra);
        Self { package, conflict }
    }
}

impl From<(PackageName, GroupName)> for ConflictItem {
    fn from((package, group): (PackageName, GroupName)) -> Self {
        let conflict = ConflictPackage::Group(group);
        Self { package, conflict }
    }
}

/// A single item in a conflicting set, by reference.
///
/// Each item is a pair of a package and a corresponding extra name for that
/// package.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ConflictItemRef<'a> {
    package: &'a PackageName,
    conflict: ConflictPackageRef<'a>,
}

impl<'a> ConflictItemRef<'a> {
    /// Returns the package name of this conflicting item.
    pub fn package(&self) -> &'a PackageName {
        self.package
    }

    /// Returns the package-specific conflict.
    ///
    /// i.e., Either an extra or a group name.
    pub fn conflict(&self) -> ConflictPackageRef<'a> {
        self.conflict
    }

    /// Returns the extra name of this conflicting item.
    pub fn extra(&self) -> Option<&'a ExtraName> {
        self.conflict.extra()
    }

    /// Returns the group name of this conflicting item.
    pub fn group(&self) -> Option<&'a GroupName> {
        self.conflict.group()
    }

    /// Converts this borrowed conflicting item to its owned variant.
    pub fn to_owned(&self) -> ConflictItem {
        ConflictItem {
            package: self.package().clone(),
            conflict: self.conflict.to_owned(),
        }
    }
}

impl<'a> From<(&'a PackageName, &'a ExtraName)> for ConflictItemRef<'a> {
    fn from((package, extra): (&'a PackageName, &'a ExtraName)) -> Self {
        let conflict = ConflictPackageRef::Extra(extra);
        ConflictItemRef { package, conflict }
    }
}

impl<'a> From<(&'a PackageName, &'a GroupName)> for ConflictItemRef<'a> {
    fn from((package, group): (&'a PackageName, &'a GroupName)) -> Self {
        let conflict = ConflictPackageRef::Group(group);
        ConflictItemRef { package, conflict }
    }
}

impl hashbrown::Equivalent<ConflictItem> for ConflictItemRef<'_> {
    fn equivalent(&self, key: &ConflictItem) -> bool {
        key.as_ref() == *self
    }
}

/// The actual conflicting data for a package.
///
/// That is, either an extra or a group name.
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ConflictPackage {
    Extra(ExtraName),
    Group(GroupName),
}

impl ConflictPackage {
    /// If this conflict corresponds to an extra, then return the
    /// extra name.
    pub fn extra(&self) -> Option<&ExtraName> {
        match self {
            Self::Extra(extra) => Some(extra),
            Self::Group(_) => None,
        }
    }

    /// If this conflict corresponds to a group, then return the
    /// group name.
    pub fn group(&self) -> Option<&GroupName> {
        match self {
            Self::Group(group) => Some(group),
            Self::Extra(_) => None,
        }
    }

    /// Returns this conflict as a new type with its fields borrowed.
    pub fn as_ref(&self) -> ConflictPackageRef<'_> {
        match self {
            Self::Extra(extra) => ConflictPackageRef::Extra(extra),
            Self::Group(group) => ConflictPackageRef::Group(group),
        }
    }
}

/// The actual conflicting data for a package, by reference.
///
/// That is, either a borrowed extra name or a borrowed group name.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ConflictPackageRef<'a> {
    Extra(&'a ExtraName),
    Group(&'a GroupName),
}

impl<'a> ConflictPackageRef<'a> {
    /// If this conflict corresponds to an extra, then return the
    /// extra name.
    pub fn extra(&self) -> Option<&'a ExtraName> {
        match self {
            Self::Extra(extra) => Some(extra),
            Self::Group(_) => None,
        }
    }

    /// If this conflict corresponds to a group, then return the
    /// group name.
    pub fn group(&self) -> Option<&'a GroupName> {
        match self {
            Self::Group(group) => Some(group),
            Self::Extra(_) => None,
        }
    }

    /// Converts this borrowed conflict to its owned variant.
    pub fn to_owned(&self) -> ConflictPackage {
        match *self {
            Self::Extra(extra) => ConflictPackage::Extra(extra.clone()),
            Self::Group(group) => ConflictPackage::Group(group.clone()),
        }
    }
}

impl<'a> From<&'a ExtraName> for ConflictPackageRef<'a> {
    fn from(extra: &'a ExtraName) -> Self {
        Self::Extra(extra)
    }
}

impl<'a> From<&'a GroupName> for ConflictPackageRef<'a> {
    fn from(group: &'a GroupName) -> Self {
        Self::Group(group)
    }
}

impl PartialEq<ConflictPackage> for ConflictPackageRef<'_> {
    fn eq(&self, other: &ConflictPackage) -> bool {
        other.as_ref() == *self
    }
}

impl<'a> PartialEq<ConflictPackageRef<'a>> for ConflictPackage {
    fn eq(&self, other: &ConflictPackageRef<'a>) -> bool {
        self.as_ref() == *other
    }
}

impl hashbrown::Equivalent<ConflictPackage> for ConflictPackageRef<'_> {
    fn equivalent(&self, key: &ConflictPackage) -> bool {
        key.as_ref() == *self
    }
}

/// An error that occurs when the given conflicting set is invalid somehow.
#[derive(Debug, thiserror::Error)]
pub enum ConflictError {
    /// An error for when there are zero conflicting items.
    #[error("Each set of conflicts must have at least two entries, but found none")]
    ZeroItems,
    /// An error for when there is one conflicting items.
    #[error("Each set of conflicts must have at least two entries, but found only one")]
    OneItem,
    /// An error that occurs when the `package` field is missing.
    ///
    /// (This is only applicable when deserializing from the lock file.
    /// When deserializing from `pyproject.toml`, the `package` field is
    /// optional.)
    #[error("Expected `package` field in conflicting entry")]
    MissingPackage,
    /// An error that occurs when both `extra` and `group` are missing.
    #[error("Expected `extra` or `group` field in conflicting entry")]
    MissingExtraAndGroup,
    /// An error that occurs when both `extra` and `group` are present.
    #[error("Expected one of `extra` or `group` in conflicting entry, but found both")]
    FoundExtraAndGroup,
    #[error("Expected `ConflictSet` to contain `ConflictItem` to replace")]
    ReplaceMissingConflictItem,
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
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SchemaConflicts(Vec<SchemaConflictSet>);

impl SchemaConflicts {
    /// Convert the public schema "conflicting" type to our internal fully
    /// resolved type. Effectively, this pairs the corresponding package name
    /// with each conflict.
    ///
    /// If a conflict has an explicit package name (written by the end user),
    /// then that takes precedence over the given package name, which is only
    /// used when there is no explicit package name written.
    pub fn to_conflicts_with_package_name(&self, package: &PackageName) -> Conflicts {
        let mut conflicting = Conflicts::empty();
        for tool_uv_set in &self.0 {
            let mut set = vec![];
            for item in &tool_uv_set.0 {
                let package = item.package.clone().unwrap_or_else(|| package.clone());
                set.push(ConflictItem {
                    package: package.clone(),
                    conflict: item.conflict.clone(),
                });
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
#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SchemaConflictSet(Vec<SchemaConflictItem>);

/// Like [`ConflictItem`], but for deserialization in `pyproject.toml`.
///
/// The schema format is different from the in-memory format. Specifically, the
/// schema format does not allow specifying the package name (or will make it
/// optional in the future), where as the in-memory format needs the package
/// name.
#[derive(
    Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(
    deny_unknown_fields,
    try_from = "ConflictItemWire",
    into = "ConflictItemWire"
)]
pub struct SchemaConflictItem {
    package: Option<PackageName>,
    conflict: ConflictPackage,
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SchemaConflictItem {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("SchemaConflictItem")
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        <ConflictItemWire as schemars::JsonSchema>::json_schema(generator)
    }
}

impl<'de> serde::Deserialize<'de> for SchemaConflictSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let items = Vec::<SchemaConflictItem>::deserialize(deserializer)?;
        Self::try_from(items).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Vec<SchemaConflictItem>> for SchemaConflictSet {
    type Error = ConflictError;

    fn try_from(items: Vec<SchemaConflictItem>) -> Result<Self, ConflictError> {
        match items.len() {
            0 => return Err(ConflictError::ZeroItems),
            1 => return Err(ConflictError::OneItem),
            _ => {}
        }
        Ok(Self(items))
    }
}

/// A single item in a conflicting set.
///
/// Each item is a pair of an (optional) package and a corresponding extra or group name for that
/// package.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
struct ConflictItemWire {
    #[serde(default)]
    package: Option<PackageName>,
    #[serde(default)]
    extra: Option<ExtraName>,
    #[serde(default)]
    group: Option<GroupName>,
}

impl TryFrom<ConflictItemWire> for ConflictItem {
    type Error = ConflictError;

    fn try_from(wire: ConflictItemWire) -> Result<Self, ConflictError> {
        let Some(package) = wire.package else {
            return Err(ConflictError::MissingPackage);
        };
        match (wire.extra, wire.group) {
            (None, None) => Err(ConflictError::MissingExtraAndGroup),
            (Some(_), Some(_)) => Err(ConflictError::FoundExtraAndGroup),
            (Some(extra), None) => Ok(Self::from((package, extra))),
            (None, Some(group)) => Ok(Self::from((package, group))),
        }
    }
}

impl From<ConflictItem> for ConflictItemWire {
    fn from(item: ConflictItem) -> Self {
        match item.conflict {
            ConflictPackage::Extra(extra) => Self {
                package: Some(item.package),
                extra: Some(extra),
                group: None,
            },
            ConflictPackage::Group(group) => Self {
                package: Some(item.package),
                extra: None,
                group: Some(group),
            },
        }
    }
}

impl TryFrom<ConflictItemWire> for SchemaConflictItem {
    type Error = ConflictError;

    fn try_from(wire: ConflictItemWire) -> Result<Self, ConflictError> {
        let package = wire.package;
        match (wire.extra, wire.group) {
            (None, None) => Err(ConflictError::MissingExtraAndGroup),
            (Some(_), Some(_)) => Err(ConflictError::FoundExtraAndGroup),
            (Some(extra), None) => Ok(Self {
                package,
                conflict: ConflictPackage::Extra(extra),
            }),
            (None, Some(group)) => Ok(Self {
                package,
                conflict: ConflictPackage::Group(group),
            }),
        }
    }
}

impl From<SchemaConflictItem> for ConflictItemWire {
    fn from(item: SchemaConflictItem) -> Self {
        match item.conflict {
            ConflictPackage::Extra(extra) => Self {
                package: item.package,
                extra: Some(extra),
                group: None,
            },
            ConflictPackage::Group(group) => Self {
                package: item.package,
                extra: None,
                group: Some(group),
            },
        }
    }
}
