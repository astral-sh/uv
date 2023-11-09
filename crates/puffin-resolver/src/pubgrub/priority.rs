use std::cmp::Reverse;

use fxhash::FxHashMap;

use puffin_normalize::PackageName;

use crate::pubgrub::package::PubGrubPackage;

#[derive(Debug, Default)]
pub(crate) struct PubGrubPriorities(FxHashMap<PackageName, usize>);

impl PubGrubPriorities {
    /// Add a package to the priority map.
    pub(crate) fn add(&mut self, package: PackageName) {
        let priority = self.0.len();
        self.0.entry(package).or_insert(priority);
    }

    /// Return the priority of the given package, if it exists.
    pub(crate) fn get(&self, package: &PubGrubPackage) -> Option<PubGrubPriority> {
        match package {
            PubGrubPackage::Root(_) => Some(Reverse(0)),
            PubGrubPackage::Package(name, ..) | PubGrubPackage::UrlPackage(name, ..) => self
                .0
                .get(name)
                .copied()
                .map(|priority| priority + 1)
                .map(Reverse),
        }
    }
}

pub(crate) type PubGrubPriority = Reverse<usize>;
