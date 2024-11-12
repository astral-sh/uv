#[derive(Debug, Default, Copy, Clone)]
pub enum LowerBound {
    /// Allow missing lower bounds.
    #[default]
    Allow,
    /// Warn about missing lower bounds.
    Warn,
}
