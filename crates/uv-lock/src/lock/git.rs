use uv_redacted::DisplaySafeUrl;

/// A Git source with repository identity and checkout settings recorded separately.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct GitSourceWire {
    pub(super) git: String,
    #[serde(flatten)]
    pub(super) fields: GitFieldsWire,
}

/// Checkout settings shared by resolved Git sources and declared Git requirements.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct GitFieldsWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subdirectory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lfs: Option<bool>,
}

impl GitSourceWire {
    /// Extract checkout settings from the internal lockfile URL representation.
    pub(super) fn from_url(mut url: DisplaySafeUrl) -> Self {
        let mut fields = GitFieldsWire {
            commit: url.fragment().map(str::to_owned),
            ..GitFieldsWire::default()
        };
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "branch" => fields.branch = Some(value.into_owned()),
                "tag" => fields.tag = Some(value.into_owned()),
                "rev" => fields.rev = Some(value.into_owned()),
                "subdirectory" => fields.subdirectory = Some(value.into_owned()),
                "path" => fields.path = Some(value.into_owned()),
                "lfs" if value.eq_ignore_ascii_case("true") => fields.lfs = Some(true),
                _ => {}
            }
        }
        url.set_query(None);
        url.set_fragment(None);
        url.remove_credentials();
        Self {
            git: url.to_string(),
            fields,
        }
    }
}

impl GitFieldsWire {
    /// Encode explicit fields for the shared Git URL validation and conversion routines.
    pub(super) fn apply_to_url(
        self,
        mut url: DisplaySafeUrl,
    ) -> Result<DisplaySafeUrl, &'static str> {
        let references = [self.branch.as_ref(), self.tag.as_ref(), self.rev.as_ref()];
        if references.iter().flatten().count() > 1 {
            return Err("only one of `branch`, `tag`, or `rev` may be specified");
        }
        if self.subdirectory.is_some() && self.path.is_some() {
            return Err("only one of `subdirectory` or `path` may be specified");
        }
        let has_fields = references.iter().any(Option::is_some)
            || self.commit.is_some()
            || self.subdirectory.is_some()
            || self.path.is_some()
            || self.lfs.is_some();
        if !has_fields {
            return Ok(url);
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(
                "cannot combine structured Git fields with URL query parameters or fragments",
            );
        }
        for (key, value) in [("subdirectory", self.subdirectory), ("path", self.path)] {
            if let Some(value) = value {
                url.query_pairs_mut().append_pair(key, &value);
            }
        }
        if self.lfs == Some(true) {
            url.query_pairs_mut().append_pair("lfs", "true");
        }
        for (key, value) in [
            ("branch", self.branch),
            ("tag", self.tag),
            ("rev", self.rev),
        ] {
            if let Some(value) = value {
                url.query_pairs_mut().append_pair(key, &value);
            }
        }
        url.set_fragment(self.commit.as_deref());
        Ok(url)
    }
}
