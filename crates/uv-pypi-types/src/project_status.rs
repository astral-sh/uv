//! PEP 792 project status marker types.

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
            "active" => Some(Status::Active),
            "archived" => Some(Status::Archived),
            "quarantined" => Some(Status::Quarantined),
            "deprecated" => Some(Status::Deprecated),
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
