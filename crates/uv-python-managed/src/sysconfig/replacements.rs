/// Replacement mode for sysconfig values.
#[derive(Debug)]
pub(crate) enum ReplacementMode {
    Partial { from: String },
    Full,
}

/// A replacement entry to patch in sysconfig data.
#[derive(Debug)]
pub(crate) struct ReplacementEntry {
    pub(crate) mode: ReplacementMode,
    pub(crate) to: String,
}

impl ReplacementEntry {
    /// Patches a sysconfig value either partially (replacing a specific word) or fully.
    pub(crate) fn patch(&self, entry: &str) -> String {
        match &self.mode {
            ReplacementMode::Partial { from } => entry
                .split_whitespace()
                .map(|word| if word == from { &self.to } else { word })
                .collect::<Vec<_>>()
                .join(" "),
            ReplacementMode::Full => self.to.clone(),
        }
    }
}
