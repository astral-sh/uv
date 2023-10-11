use anyhow::Result;

use pep440_rs::{Operator, VersionSpecifier};
use std::ops::Range;

use crate::facade::{PubGrubVersion, VERSION_INFINITY, VERSION_ZERO};

pub fn to_ranges(specifier: &VersionSpecifier) -> Result<Vec<Range<PubGrubVersion>>> {
    match specifier.operator() {
        Operator::Equal => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version.clone()..version.next()])
        }
        Operator::EqualStar => {
            todo!("Operator::EqualStar")
        }
        Operator::ExactEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version.clone()..version.next()])
        }
        Operator::NotEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![
                VERSION_ZERO.clone()..version.clone(),
                version.next()..VERSION_INFINITY.clone(),
            ])
        }
        Operator::NotEqualStar => {
            todo!("Operator::NotEqualStar")
        }
        Operator::TildeEqual => {
            todo!("Operator::TildeEqual")
        }
        Operator::LessThan => {
            // "The exclusive ordered comparison <V MUST NOT allow a pre-release of
            // the specified version unless the specified version is itself a
            // pre-release."
            if specifier.version().any_prerelease() {
                let version = PubGrubVersion::from(specifier.version().clone());
                Ok(vec![VERSION_ZERO.clone()..version.clone()])
            } else {
                let max_version = pep440_rs::Version {
                    post: None,
                    dev: Some(0),
                    local: None,
                    ..specifier.version().clone()
                };
                let version = PubGrubVersion::from(max_version);
                Ok(vec![VERSION_ZERO.clone()..version.clone()])
            }
        }
        Operator::LessThanEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![VERSION_ZERO.clone()..version.next()])
        }
        Operator::GreaterThan => {
            todo!("Operator::GreaterThan")
        }
        Operator::GreaterThanEqual => {
            let version = PubGrubVersion::from(specifier.version().clone());
            Ok(vec![version..VERSION_INFINITY.clone()])
        }
    }
}
