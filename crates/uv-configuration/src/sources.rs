#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceStrategy {
    /// Use `tool.uv.sources` when resolving dependencies.
    #[default]
    Enabled,
    /// Ignore `tool.uv.sources` when resolving dependencies.
    Disabled,
}

impl SourceStrategy {
    /// Return the [`SourceStrategy`] from the command-line arguments, if any.
    pub fn from_args(no_sources: bool) -> Self {
        if no_sources {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }
}
