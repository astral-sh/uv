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

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum VersionFormat {
    /// Display the version as plain text.
    Text,
    /// Display the version as JSON.
    Json,
}

#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum SyncFormat {
    /// Display the result in a human-readable format.
    #[default]
    Text,
    /// Display the result in JSON format.
    Json,
}

#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PipInstallFormat {
    /// Display the result in a human-readable format.
    #[default]
    Text,
    /// Display the result in JSON format.
    Json,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum TreeFormat {
    /// Display the dependency graph as a human-readable tree.
    #[default]
    Text,
    /// Display the dependency graph as JSON.
    Json,
}

/// The output version for workspace metadata. The default is the latest supported version.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataOutputVersion {
    /// An unstable, experimental schema.
    #[default]
    Preview,
}

impl From<&str> for MetadataOutputVersion {
    fn from(version: &str) -> Self {
        match version {
            "preview" => Self::Preview,
            _ => Self::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum ListFormat {
    /// Display the list of packages in a human-readable table.
    #[default]
    Columns,
    /// Display the list of packages in a `pip freeze`-like format, with one package per line
    /// alongside its version.
    Freeze,
    /// Display the list of packages in a machine-readable JSON format.
    Json,
}
