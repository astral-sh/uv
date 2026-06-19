use rustc_hash::FxHashMap;

use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;


#[derive(Debug, Default, Clone)]
pub struct Excludes(FxHashMap<PackageName, VersionSpecifiers>);

impl Excludes {

    pub fn contains(&self, name: &PackageName) -> bool {
        matches!(self.0.get(name), Some(specifiers) if specifiers.is_empty())
    }

    pub fn is_excluded(&self, name: &PackageName, version: &Version) -> bool {
        match self.0.get(name) {
            // Name is excluded for all versions.
            Some(specifiers) if specifiers.is_empty() => true,
            // Name is excluded only for matching versions.
            Some(specifiers) => specifiers.contains(version),
            None => false,
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &PackageName> {
        self.0.keys()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Build name-only exclusions (every version excluded). Preserves the legacy behavior of
/// `tool.uv.exclude-dependencies = ["werkzeug"]`.
impl FromIterator<PackageName> for Excludes {
    fn from_iter<I: IntoIterator<Item = PackageName>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|name| (name, VersionSpecifiers::empty()))
                .collect(),
        )
    }
}

impl<T: uv_pep508::Pep508Url> FromIterator<Requirement<T>> for Excludes {
    fn from_iter<I: IntoIterator<Item = Requirement<T>>>(iter: I) -> Self {
        let mut map: FxHashMap<PackageName, VersionSpecifiers> = FxHashMap::default();
        for requirement in iter {
            let specifiers = match requirement.version_or_url {
                Some(uv_pep508::VersionOrUrl::VersionSpecifier(specifiers)) => specifiers,
                // A URL or bare name excludes every version.
                _ => VersionSpecifiers::empty(),
            };
            // PROTOTYPE: duplicate entries for the same name simply overwrite. A real
            // implementation should union the specifier sets.
            map.insert(requirement.name, specifiers);
        }
        Self(map)
    }
}
