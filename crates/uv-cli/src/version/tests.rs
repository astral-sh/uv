use insta::{assert_json_snapshot, assert_snapshot};

use super::{CommitInfo, VersionInfo};

#[test]
fn version_formatting() {
    let version = VersionInfo {
        version: "0.0.0".to_string(),
        commit_info: None,
    };
    assert_snapshot!(version, @"0.0.0");
}

#[test]
fn version_formatting_with_commit_info() {
    let version = VersionInfo {
        version: "0.0.0".to_string(),
        commit_info: Some(CommitInfo {
            short_commit_hash: "53b0f5d92".to_string(),
            commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
            last_tag: Some("v0.0.1".to_string()),
            commit_date: "2023-10-19".to_string(),
            commits_since_last_tag: 0,
        }),
    };
    assert_snapshot!(version, @"0.0.0 (53b0f5d92 2023-10-19)");
}

#[test]
fn version_formatting_with_commits_since_last_tag() {
    let version = VersionInfo {
        version: "0.0.0".to_string(),
        commit_info: Some(CommitInfo {
            short_commit_hash: "53b0f5d92".to_string(),
            commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
            last_tag: Some("v0.0.1".to_string()),
            commit_date: "2023-10-19".to_string(),
            commits_since_last_tag: 24,
        }),
    };
    assert_snapshot!(version, @"0.0.0+24 (53b0f5d92 2023-10-19)");
}

#[test]
fn version_serializable() {
    let version = VersionInfo {
        version: "0.0.0".to_string(),
        commit_info: Some(CommitInfo {
            short_commit_hash: "53b0f5d92".to_string(),
            commit_hash: "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7".to_string(),
            last_tag: Some("v0.0.1".to_string()),
            commit_date: "2023-10-19".to_string(),
            commits_since_last_tag: 0,
        }),
    };
    assert_json_snapshot!(version, @r#"
    {
      "version": "0.0.0",
      "commit_info": {
        "short_commit_hash": "53b0f5d92",
        "commit_hash": "53b0f5d924110e5b26fbf09f6fd3a03d67b475b7",
        "commit_date": "2023-10-19",
        "last_tag": "v0.0.1",
        "commits_since_last_tag": 0
      }
    }
    "#);
}
