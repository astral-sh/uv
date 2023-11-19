//! Cache all the wheel metadata cases:
//! * Metadata we got from a remote wheel
//!   * From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
//!   * From a remote wheel by partial zip reading
//!   * From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
//! * Metadata we got from building a source dist, keyed by the wheel name since we can have multiple wheels per source dist (e.g. for python version specific builds)

pub enum WheelMetadataCachingIndex {
    Pypi,
    // https://github.com/astral-sh/puffin/issues/448
    Index,
    Url,
}

impl WheelMetadataCachingIndex {
    pub fn to_key_segment(&self) -> String {
        match self {
            WheelMetadataCachingIndex::Pypi => "pypi".to_string(),
            WheelMetadataCachingIndex::Index => {
                // TODO(konstin): https://github.com/astral-sh/puffin/issues/448
                // let hash = Sha256::new().chain_update(url.as_str()).finalize();
                // format!("{hash:x}.json")
                "index".to_string()
            }
            WheelMetadataCachingIndex::Url => "url".to_string(),
        }
    }
}

/*enum WheelSource {
    Pypi(WheelFilename),
    Index { root: Url, url: Url },
    Url(Url),
    Git { url: Url, rev: String },
}*/
