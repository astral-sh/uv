//! SARIF layout models for `uv audit`.
//!
//! These models are adapted from the MIT-licensed `zizmor-sarif` crate,
//! copyright 2024 William Woodruff. Only the subset of SARIF 2.1.0 that
//! `uv audit` emits is modeled here.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use uv_audit::{AdverseStatus, ProjectStatus, Vulnerability};

/// Top-level SARIF log object (SARIF §3.13).
#[derive(Debug, Serialize)]
pub(crate) struct Report {
    #[serde(rename = "$schema")]
    schema: String,
    runs: Vec<Run>,
    version: String,
}

impl Report {
    pub(crate) fn from_findings(
        vulnerabilities: &[&Vulnerability],
        statuses: &[&ProjectStatus],
        artifact_uri: &str,
    ) -> Self {
        let mut vulnerabilities = vulnerabilities.to_vec();
        vulnerabilities.sort_by(|first, second| {
            first
                .dependency
                .name()
                .cmp(second.dependency.name())
                .then_with(|| first.dependency.version().cmp(second.dependency.version()))
                .then_with(|| first.best_id().as_str().cmp(second.best_id().as_str()))
        });

        let mut statuses = statuses.to_vec();
        statuses.sort_by(|first, second| {
            first
                .name
                .cmp(&second.name)
                .then_with(|| first.status.to_string().cmp(&second.status.to_string()))
        });

        let mut rules = BTreeMap::new();
        let mut results = Vec::with_capacity(vulnerabilities.len() + statuses.len());

        for vulnerability in vulnerabilities {
            let rule = ReportingDescriptor::from_vulnerability(vulnerability);
            let rule_id = rule.id.clone();
            rules.entry(rule_id.clone()).or_insert(rule);
            results.push(Result::from_vulnerability(
                vulnerability,
                rule_id,
                artifact_uri,
            ));
        }

        for status in statuses {
            let rule = ReportingDescriptor::from_status(status);
            let rule_id = rule.id.clone();
            rules.entry(rule_id.clone()).or_insert(rule);
            results.push(Result::from_status(status, rule_id, artifact_uri));
        }

        Self {
            schema:
                "https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/schemas/sarif-schema-2.1.0.json"
                    .to_string(),
            runs: vec![Run {
                invocations: vec![Invocation {
                    execution_successful: true,
                }],
                results,
                tool: Tool {
                    driver: ToolComponent {
                        download_uri: Some(env!("CARGO_PKG_REPOSITORY").to_string()),
                        information_uri: Some(env!("CARGO_PKG_HOMEPAGE").to_string()),
                        name: env!("CARGO_PKG_NAME").to_string(),
                        rules: rules.into_values().collect(),
                        semantic_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    },
                },
            }],
            version: "2.1.0".to_string(),
        }
    }
}

/// A single tool invocation's results (SARIF §3.14).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    invocations: Vec<Invocation>,
    results: Vec<Result>,
    tool: Tool,
}

/// Tool metadata wrapper (SARIF §3.18).
#[derive(Debug, Serialize)]
struct Tool {
    driver: ToolComponent,
}

/// Tool driver metadata (SARIF §3.19).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolComponent {
    #[serde(skip_serializing_if = "Option::is_none")]
    download_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    information_uri: Option<String>,
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rules: Vec<ReportingDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// Invocation describing the tool execution (SARIF §3.20).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Invocation {
    execution_successful: bool,
}

/// A reporting descriptor, i.e. a rule definition (SARIF §3.49).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportingDescriptor {
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<MultiformatMessageString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help_uri: Option<String>,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<PropertyBag>,
}

impl ReportingDescriptor {
    fn from_vulnerability(vulnerability: &Vulnerability) -> Self {
        let id = vulnerability.id.as_str().to_string();
        let name = vulnerability.best_id().as_str().to_string();
        let help = vulnerability
            .description
            .as_ref()
            .or(vulnerability.summary.as_ref())
            .map(|description| MultiformatMessageString {
                markdown: None,
                text: description.clone(),
            });

        Self {
            help,
            help_uri: vulnerability
                .link
                .as_ref()
                .map(|link| link.as_str().to_string()),
            name: Some(name),
            id,
            properties: Some(PropertyBag {
                tags: vec!["security".to_string(), "vulnerability".to_string()],
                additional_properties: BTreeMap::new(),
            }),
        }
    }

    fn from_status(status: &ProjectStatus) -> Self {
        let (name, description) = match status.status {
            AdverseStatus::Archived => (
                "archived",
                "The project is archived and is no longer maintained.",
            ),
            AdverseStatus::Deprecated => (
                "deprecated",
                "The project is deprecated and may have been superseded by another project.",
            ),
            AdverseStatus::Quarantined => (
                "quarantined",
                "The project is quarantined and is considered unsafe for use.",
            ),
        };

        Self {
            help: Some(MultiformatMessageString {
                markdown: None,
                text: description.to_string(),
            }),
            help_uri: Some(format!(
                "https://packaging.python.org/en/latest/specifications/project-status-markers/#{name}"
            )),
            id: format!("uv/project-status/{name}"),
            name: Some(name.to_string()),
            properties: Some(PropertyBag {
                tags: vec!["package".to_string(), "project-status".to_string()],
                additional_properties: BTreeMap::new(),
            }),
        }
    }
}

/// Plain-text and Markdown message (SARIF §3.12).
#[derive(Debug, Serialize)]
struct MultiformatMessageString {
    #[serde(skip_serializing_if = "Option::is_none")]
    markdown: Option<String>,
    text: String,
}

/// Property bag (SARIF §3.8).
#[derive(Debug, Serialize)]
struct PropertyBag {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(flatten)]
    additional_properties: BTreeMap<String, Value>,
}

/// A single finding within a run (SARIF §3.27).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Result {
    kind: ResultKind,
    level: ResultLevel,
    locations: Vec<Location>,
    message: Message,
    /// Tool-specific values that identify findings independently of their source location.
    ///
    /// GitHub code scanning only consumes `primaryLocationLineHash`. We intentionally do not use
    /// that key for the semantic package identities below: all findings currently point at line 1,
    /// so those values would not actually be location hashes. Other SARIF consumers can still use
    /// these stable uv-specific keys.
    partial_fingerprints: BTreeMap<String, String>,
    properties: PropertyBag,
    rule_id: String,
}

impl Result {
    fn from_vulnerability(
        vulnerability: &Vulnerability,
        rule_id: String,
        artifact_uri: &str,
    ) -> Self {
        let dependency = &vulnerability.dependency;
        let name = dependency.name().to_string();
        let version = dependency.version().to_string();
        let display_id = vulnerability.best_id().as_str();
        let message = if let Some(summary) = &vulnerability.summary {
            format!("{name} {version} is vulnerable to {display_id}: {summary}")
        } else {
            format!("{name} {version} is vulnerable to {display_id}")
        };

        let mut partial_fingerprints = BTreeMap::new();
        partial_fingerprints.insert(
            "uv/vulnerability".to_string(),
            format!("{}:{name}:{version}", vulnerability.id.as_str()),
        );

        let mut additional_properties = BTreeMap::new();
        additional_properties.insert(
            "uv/aliases".to_string(),
            Value::Array(
                vulnerability
                    .aliases
                    .iter()
                    .map(|alias| Value::String(alias.as_str().to_string()))
                    .collect(),
            ),
        );
        additional_properties.insert(
            "uv/displayId".to_string(),
            Value::String(display_id.to_string()),
        );
        additional_properties.insert(
            "uv/fixVersions".to_string(),
            Value::Array(
                vulnerability
                    .fix_versions
                    .iter()
                    .map(|version| Value::String(version.to_string()))
                    .collect(),
            ),
        );
        additional_properties.insert(
            "uv/id".to_string(),
            Value::String(vulnerability.id.as_str().to_string()),
        );
        additional_properties.insert("uv/package".to_string(), Value::String(name.clone()));
        if let Some(modified) = &vulnerability.modified {
            additional_properties.insert(
                "uv/modified".to_string(),
                Value::String(modified.to_string()),
            );
        }
        if let Some(published) = &vulnerability.published {
            additional_properties.insert(
                "uv/published".to_string(),
                Value::String(published.to_string()),
            );
        }
        additional_properties.insert("uv/version".to_string(), Value::String(version.clone()));

        Self {
            kind: ResultKind::Fail,
            level: ResultLevel::Error,
            locations: vec![Location::package(&name, Some(&version), artifact_uri)],
            message: Message { text: message },
            partial_fingerprints,
            properties: PropertyBag {
                tags: Vec::new(),
                additional_properties,
            },
            rule_id,
        }
    }

    fn from_status(status: &ProjectStatus, rule_id: String, artifact_uri: &str) -> Self {
        let name = status.name.to_string();
        let status_name = status.status.to_string();
        let message = if let Some(reason) = &status.reason {
            format!("{name} is {status_name}: {reason}")
        } else {
            format!("{name} is {status_name}")
        };

        let mut partial_fingerprints = BTreeMap::new();
        partial_fingerprints.insert(
            "uv/project-status".to_string(),
            format!("{name}:{status_name}"),
        );

        let mut additional_properties = BTreeMap::new();
        additional_properties.insert("uv/package".to_string(), Value::String(name.clone()));
        additional_properties.insert("uv/status".to_string(), Value::String(status_name));
        if let Some(reason) = &status.reason {
            additional_properties.insert("uv/reason".to_string(), Value::String(reason.clone()));
        }

        Self {
            kind: ResultKind::Fail,
            level: ResultLevel::Warning,
            locations: vec![Location::package(&name, None, artifact_uri)],
            message: Message { text: message },
            partial_fingerprints,
            properties: PropertyBag {
                tags: Vec::new(),
                additional_properties,
            },
            rule_id,
        }
    }
}

/// A human-readable message (SARIF §3.11).
#[derive(Debug, Serialize)]
struct Message {
    text: String,
}

/// A location (SARIF §3.28).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    logical_locations: Vec<LogicalLocation>,
    physical_location: PhysicalLocation,
}

impl Location {
    fn package(name: &str, version: Option<&str>, artifact_uri: &str) -> Self {
        Self {
            logical_locations: vec![LogicalLocation {
                fully_qualified_name: Some(
                    version.map_or_else(|| name.to_string(), |version| format!("{name}@{version}")),
                ),
                kind: Some("package".to_string()),
                name: Some(name.to_string()),
            }],
            // TODO: Point each finding at its `[[package]]` table instead of line 1.
            // This requires us to have a spanning view of a lockfile similar to
            // how `pyproject.toml` spans are emitted. We'd also need to figure out
            // how to represent a discarded lockfile, e.g. from a audit of an
            // unlocked script.
            physical_location: PhysicalLocation {
                artifact_location: ArtifactLocation {
                    uri: artifact_uri.to_string(),
                },
                region: Region { start_line: 1 },
            },
        }
    }
}

/// A physical location (SARIF §3.29).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation {
    artifact_location: ArtifactLocation,
    region: Region,
}

/// Pointer to a single artifact (SARIF §3.4).
#[derive(Debug, Serialize)]
struct ArtifactLocation {
    uri: String,
}

/// A region within an artifact (SARIF §3.30).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: u32,
}

/// A logical location (SARIF §3.33).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogicalLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    fully_qualified_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Classification of a result (SARIF §3.27.9).
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResultKind {
    Fail,
}

/// Severity of a result (SARIF §3.27.10).
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResultLevel {
    Warning,
    Error,
}
