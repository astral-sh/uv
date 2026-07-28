// Scaffolding for resolution policy configuration.
// To be wired into uv-configuration's lib once stabilized.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PythonResolution {
    #[serde(default)]
    pub only: Option<String>,
    #[serde(default)]
    pub range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ResolutionPolicy {
    #[serde(default)]
    pub python: PythonResolution,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_only() {
        let v = json!({"python":{"only":"3.11.9"}});
        let p: ResolutionPolicy = serde_json::from_value(v).unwrap();
        assert_eq!(p.python.only.as_deref(), Some("3.11.9"));
        assert!(p.python.range.is_none());
    }

    #[test]
    fn parse_range() {
        let v = json!({"python":{"range":">=3.10,<3.13"}});
        let p: ResolutionPolicy = serde_json::from_value(v).unwrap();
        assert_eq!(p.python.range.as_deref(), Some(">=3.10,<3.13"));
        assert!(p.python.only.is_none());
    }
}
