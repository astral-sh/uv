use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionPolicy {
    /// Resolve only for a single, exact Python version (e.g. 3.12.3)
    Only(String),
    /// Resolve for a PEP 440-compatible version spec (e.g. ">=3.12,<3.13")
    Range(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case" )] 
pub struct PythonResolutionConfig {
    pub resolution: Option<ResolutionPolicy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_only() {
        let toml = r#"[python]
[python.resolution]
only = "3.12.3"
"#;
        let parsed: Result<PythonResolutionConfig, _> = toml::from_str(toml);
        assert!(parsed.is_ok());
        let cfg = parsed.unwrap();
        assert!(matches!(cfg.resolution, Some(ResolutionPolicy::Only(v)) if v == "3.12.3"));
    }

    #[test]
    fn deserialize_range() {
        let toml = r#"[python]
[python.resolution]
range = ">=3.12,<3.13"
"#;
        let parsed: Result<PythonResolutionConfig, _> = toml::from_str(toml);
        assert!(parsed.is_ok());
        let cfg = parsed.unwrap();
        assert!(matches!(cfg.resolution, Some(ResolutionPolicy::Range(v)) if v == ">=3.12,<3.13"));
    }
}
