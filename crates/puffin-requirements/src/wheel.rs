use std::str::FromStr;

use anyhow::{anyhow, bail};
use once_cell::sync::Lazy;
use pep440_rs::Version;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct WheelName {
    // TODO(charlie): Normalized package name.
    pub distribution: String,
    pub version: Version,
    pub build_number: Option<u32>,
    pub build_name: String,
    pub py_tags: Vec<String>,
    pub abi_tags: Vec<String>,
    pub arch_tags: Vec<String>,
}

static BUILD_TAG_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(^[0-9]*)(.*)$").unwrap());

impl FromStr for WheelName {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = ".whl";

        let stem = s
            .strip_suffix(suffix)
            .ok_or_else(|| anyhow!("expected wheel name to end with {:?}: {:?}", suffix, s))?;

        let mut pieces: Vec<&str> = stem.split('-').collect();

        let build_number: Option<u32>;
        let build_name: String;
        if pieces.len() == 6 {
            let build_tag = pieces.remove(2);
            if build_tag.is_empty() {
                bail!("found empty build tag: {s:?}");
            }
            // unwrap safe because: the regex cannot fail
            let captures = BUILD_TAG_SPLIT.captures(build_tag).unwrap();
            build_number = captures.get(1).and_then(|m| m.as_str().parse().ok());
            // unwrap safe because: this group will always match something, even
            // if only the empty string
            build_name = captures.get(2).unwrap().as_str().into();
        } else {
            build_number = None;
            build_name = "".to_owned();
        }

        let [distribution, version, py_tags, abi_tags, arch_tags] = pieces.as_slice() else {
            bail!("can't parse binary name {s:?}");
        };

        let distribution = distribution.to_string();
        let version = Version::from_str(version)
            .map_err(|e| anyhow!("failed to parse version {:?} from {:?}: {}", version, s, e))?;
        let py_tags = py_tags.split('.').map(|tag| tag.into()).collect();
        let abi_tags = abi_tags.split('.').map(|tag| tag.into()).collect();
        let arch_tags = arch_tags.split('.').map(|tag| tag.into()).collect();

        Ok(Self {
            distribution,
            version,
            build_number,
            build_name,
            py_tags,
            abi_tags,
            arch_tags,
        })
    }
}
