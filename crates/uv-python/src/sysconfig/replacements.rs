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
    pub(crate) fn patch(&self, entry: &str) -> Option<String> {
        match &self.mode {
            ReplacementMode::Partial { from } => {
                if !entry.split_whitespace().any(|word| word == from) {
                    return None;
                }

                let mut output = String::with_capacity(entry.len());
                for word in entry.split_whitespace() {
                    if !output.is_empty() {
                        output.push(' ');
                    }
                    output.push_str(if word == from { &self.to } else { word });
                }
                Some(output)
            }
            ReplacementMode::Full => (entry != self.to).then(|| self.to.clone()),
        }
    }
}
