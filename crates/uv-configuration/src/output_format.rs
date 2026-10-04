/// The output format for listing Python installations.
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PythonListFormat {
    /// Plain text (for humans).
    #[default]
    Text,
    /// JSON (for computers).
    Json,
}

/// The output format for audit findings.
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum AuditOutputFormat {
    /// Display the result in a human-readable format.
    #[default]
    Text,
    /// Display the result in JSON format.
    Json,
    /// Display the result in SARIF format.
    Sarif,
}
