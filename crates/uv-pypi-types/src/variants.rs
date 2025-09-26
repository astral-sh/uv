use indoc::formatdoc;

use crate::VerbatimParsedUrl;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct VariantProviderBackend {
    /// The provider backend string such as `fictional_tech.provider`.
    pub backend: String,
    /// The requirements that the backend requires (e.g., `["fictional_tech>=1.0"]`).
    pub requires: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
}

impl VariantProviderBackend {
    pub fn import(&self) -> String {
        let import = if let Some((path, object)) = self.backend.split_once(':') {
            format!("from {path} import {object} as backend")
        } else {
            format!("import {} as backend", self.backend)
        };

        formatdoc! {r#"
            import sys

            if sys.path[0] == "":
                sys.path.pop(0)

            {import}
        "#}
    }
}
