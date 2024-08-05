#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SourceStrategy {
    /// Use `tool.uv.sources` when resolving dependencies.
    #[default]
    Enabled,
    /// Ignore `tool.uv.sources` when resolving dependencies.
    Disabled,
}
