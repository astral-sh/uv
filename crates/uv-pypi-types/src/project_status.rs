//! PEP 792 project status marker types.
//!
//! See the living Project Status Markers specification:
//! <https://packaging.python.org/en/latest/specifications/project-status-markers/#project-status-markers>

use std::borrow::Cow;

use serde::Deserialize;
use tracing::info;
use uv_small_str::SmallString;

/// The status marker for a project.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
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
            _ => {
                info!("Unknown project status: '{status}'");
                None
            }
        }
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <Cow<'_, str>>::deserialize(deserializer)?;
        // If we don't recognize the status, default to Active.
        Ok(Self::new(&s).unwrap_or_default())
    }
}

/// The project status information.
///
/// This includes a status marker and an optional reason for the status.
#[derive(Clone, Debug, Default, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[rkyv(derive(Debug))]
pub struct ProjectStatus {
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
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

    /// An empty project status should default to Active with no reason.
    #[test]
    fn test_deserialize_empty() {
        let json = "{}";

        let project_status: ProjectStatus = serde_json::from_str(json).unwrap();
        assert_eq!(project_status.status, Status::Active);
        assert_eq!(project_status.reason, None);
    }

    /// A project status with explicitly null reason should default to Active with no reason.
    #[test]
    fn test_deserialize_null() {
        let json = r#"
        {
            "reason": null
        }
        "#;

        let project_status: ProjectStatus = serde_json::from_str(json).unwrap();
        assert_eq!(project_status.status, Status::Active);
        assert_eq!(project_status.reason, None);
    }

    /// A project status with known status and reason.
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

    /// A project status with an unknown status should default to Active.
    #[test]
    fn test_deserialize_unknown_status() {
        let json = r#"
        {
            "status": "unknown",
            "reason": "This project has an unrecognized status."
        }
        "#;

        let project_status: ProjectStatus = serde_json::from_str(json).unwrap();
        assert_eq!(project_status.status, Status::Active);
        assert_eq!(
            project_status.reason,
            Some(SmallString::from(
                "This project has an unrecognized status."
            ))
        );
    }
}
