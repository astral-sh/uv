//! PEP 792 project status marker types.
//!
//! See the living Project Status Markers specification:
//! <https://packaging.python.org/en/latest/specifications/project-status-markers/#project-status-markers>

use serde::Deserialize;
use uv_small_str::SmallString;

/// The status marker for a project.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Active,
    Archived,
    Quarantined,
    Deprecated,
}

impl Status {
    pub fn new(status: &str) -> Option<Self> {
        match status {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            "quarantined" => Some(Self::Quarantined),
            "deprecated" => Some(Self::Deprecated),
            _ => None,
        }
    }
}

/// The project status information.
///
/// This includes a status marker and an optional reason for the status.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectStatus {
    pub status: Status,
    pub reason: Option<SmallString>,
}

#[cfg(test)]
mod tests {
    use uv_small_str::SmallString;

    use crate::{ProjectStatus, Status};

    #[test]
    fn test_status() {
        assert_eq!(Status::new("active"), Some(Status::Active));
        assert_eq!(Status::new("archived"), Some(Status::Archived));
        assert_eq!(Status::new("quarantined"), Some(Status::Quarantined));
        assert_eq!(Status::new("deprecated"), Some(Status::Deprecated));
        assert_eq!(Status::new("unknown"), None);
        assert_eq!(Status::new("ACTIVE"), None);
        assert_eq!(Status::new("acTiVe"), None);
    }

    #[test]
    fn test_deserialize() {
        let json = r#"
        {
            "status": "archived",
            "reason": "This project is no longer maintained."
        }
        "#;

        let project_status: ProjectStatus = serde_json::from_str(json).unwrap();
        assert_eq!(project_status.status, Status::Archived);
        assert_eq!(
            project_status.reason,
            Some(SmallString::from("This project is no longer maintained."))
        );
    }
}
