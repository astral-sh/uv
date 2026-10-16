//! Compare complete lock-to-selection and export operations in an isolated process.
//!
//! The experimental representation consists of a compact lock and a serialized request graph.
//! Both files count toward its size and are parsed inside the timed operation. Production
//! materialization and requirements formatting are shared by both paths.

// Match the allocator used by the CLI and the operation benchmarks.
extern crate uv_performance_memory_allocator;

use std::collections::BTreeMap;
use std::env;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Instant;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use uv_configuration::{
    BuildOptions, DependencyGroups, EditableMode, ExtrasSpecification, InstallOptions,
};
use uv_distribution_types::{Node, Resolution, SimplifiedMarkerTree};
use uv_normalize::{DefaultExtras, DefaultGroups, ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};
use uv_platform_tags::{Arch, Os, Platform, Tags, TagsOptions};
use uv_pypi_types::{
    ConflictItem, ConflictKind, ConflictSet, Conflicts, ResolverMarkerEnvironment,
};
use uv_resolver_types::selection::{ActivationGraph, ActivationNode, SelectionError};
use uv_resolver_types::{ConflictMarker, UniversalMarker};

use super::selection_tests::{Section, activation_graph};
use super::{DependencyContext, Lock, LockError, LockErrorKind};
use crate::Installable;
use crate::lock::export::RequirementsTxtExport;

#[derive(Deserialize)]
struct Case {
    action: String,
    directory: PathBuf,
    roots: Vec<PackageName>,
    #[serde(default)]
    extras: Vec<ExtraName>,
    #[serde(default)]
    groups: Vec<GroupName>,
    #[serde(default)]
    only_groups: bool,
    #[serde(default = "default_python")]
    python: String,
    #[serde(default = "default_platform")]
    platform: ComparisonPlatform,
    #[serde(default = "one")]
    iterations: usize,
}

fn default_python() -> String {
    "3.12.0".to_string()
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ComparisonPlatform {
    #[default]
    Linux,
    Darwin,
    Win32,
}

fn default_platform() -> ComparisonPlatform {
    ComparisonPlatform::Linux
}

fn one() -> usize {
    1
}

#[derive(Serialize, Deserialize)]
struct RequestGraph {
    nodes: Vec<(Vec<ConflictItem>, bool)>,
    edges: Vec<(usize, usize, Option<String>)>,
    sections: Vec<(usize, Section, usize)>,
}

impl RequestGraph {
    /// Serialize requests before compacting installation guards under the conflict constraint.
    fn prepare(lock: &Lock) -> Self {
        // Missing declarations must not silently become packages with no requirements.
        for package in &lock.packages {
            for context in std::iter::once(DependencyContext::Production)
                .chain(
                    package
                        .optional_dependencies
                        .keys()
                        .map(DependencyContext::Extra),
                )
                .chain(
                    package
                        .dependency_groups
                        .keys()
                        .map(DependencyContext::Group),
                )
            {
                let requirements = match context {
                    DependencyContext::Production | DependencyContext::Extra(_) => {
                        &package.metadata.requires_dist
                    }
                    DependencyContext::Group(group) => package
                        .metadata
                        .dependency_groups
                        .get(group)
                        .expect("comparison requires group declaration metadata"),
                };
                for dependency in context.dependencies(package) {
                    assert!(
                        requirements.iter().any(|requirement| {
                            requirement.name == dependency.package_id.name
                                && !context.requirement_marker(requirement.marker).is_false()
                        }),
                        "comparison requires declaration metadata for {} -> {}",
                        package.id.name,
                        dependency.package_id.name,
                    );
                }
            }
        }
        assert!(
            lock.conflicts
                .iter()
                .flat_map(ConflictSet::iter)
                .all(|item| match item.kind() {
                    ConflictKind::Extra(_) | ConflictKind::Group(_) => true,
                    ConflictKind::Project => false,
                }),
            "project conflicts require a production-context adapter",
        );
        assert!(
            lock.manifest.requirements.is_empty() && lock.manifest.dependency_groups.is_empty(),
            "non-project workspace requirements require an explicit root adapter",
        );
        let (graph, sections) = activation_graph(lock);
        let constraints = conflict_constraint(lock);
        Self {
            nodes: graph
                .graph
                .node_weights()
                .map(|node| (node.requests.clone(), node.requires_choice))
                .collect(),
            edges: graph
                .graph
                .edge_references()
                .map(|edge| {
                    let marker = if graph.graph[edge.source()].requires_choice {
                        edge.weight().restrict(constraints)
                    } else {
                        *edge.weight()
                    };
                    (
                        edge.source().index(),
                        edge.target().index(),
                        marker.contents().map(|contents| contents.to_string()),
                    )
                })
                .collect(),
            sections: sections
                .into_iter()
                .map(|((package, section), node)| (package, section, node.index()))
                .collect(),
        }
    }

    /// Validate the requested roots, then eliminate conflict variables from installation edges.
    fn project(
        self,
        mut lock: Lock,
        case: &Case,
        concrete: Option<MarkerTree>,
    ) -> Result<Lock, String> {
        let mut graph = ActivationGraph::default();
        for (requests, requires_choice) in self.nodes {
            graph.graph.add_node(ActivationNode {
                requests,
                requires_choice,
            });
        }
        for (source, target, marker) in self.edges {
            let marker = marker.map_or(MarkerTree::TRUE, |marker| {
                marker.parse().expect("serialized request marker")
            });
            graph.graph.add_edge(
                NodeIndex::new(source),
                NodeIndex::new(target),
                concrete.map_or(marker, |environment| marker.restrict(environment)),
            );
        }
        let roots = self.sections.iter().filter_map(|(package, section, node)| {
            if !case.roots.contains(&lock.packages[*package].id.name) {
                return None;
            }
            let selected = match section {
                Section::Production => !case.only_groups,
                Section::Extra(extra) => !case.only_groups && case.extras.contains(extra),
                Section::Group(group) => case.groups.contains(group),
            };
            selected.then_some((NodeIndex::new(*node), MarkerTree::TRUE))
        });
        let environment = concrete.map_or_else(
            || lock.requires_python.to_marker_tree(),
            |_| MarkerTree::TRUE,
        );
        let selected = graph
            .select(roots, environment, &lock.conflicts)
            .map_err(selection_error)?;
        let mut activated = BTreeMap::new();
        for node in graph.graph.node_indices() {
            for item in &graph.graph[node].requests {
                let marker = selected.marker(node);
                UniversalMarker::new(MarkerTree::TRUE, ConflictMarker::from_conflict_item(item))
                    .combined()
                    .visit_extras(|_, name| {
                        let condition = activated.entry(name.clone()).or_insert(MarkerTree::FALSE);
                        *condition = condition.or(marker);
                    });
            }
        }
        for package in &mut lock.packages {
            for dependencies in std::iter::once(&mut package.dependencies)
                .chain(package.optional_dependencies.values_mut())
                .chain(package.dependency_groups.values_mut())
            {
                for dependency in dependencies.iter_mut() {
                    let marker = dependency.complexified_marker.combined();
                    let marker =
                        concrete.map_or(marker, |environment| marker.restrict(environment));
                    let marker = marker.resolve_extras(
                        activated
                            .iter()
                            .map(|(name, marker)| (name.clone(), *marker)),
                    );
                    dependency.complexified_marker = UniversalMarker::from_combined(marker);
                    dependency.simplified_marker =
                        SimplifiedMarkerTree::new(&lock.requires_python, marker);
                }
                dependencies.retain(|dependency| !dependency.complexified_marker.is_false());
            }
        }
        lock.conflicts = Conflicts::empty();
        Ok(lock)
    }
}

fn conflict_constraint(lock: &Lock) -> MarkerTree {
    UniversalMarker::new(
        MarkerTree::TRUE,
        ConflictMarker::from_conflicts(&lock.conflicts),
    )
    .combined()
}

fn selection_error(error: SelectionError) -> String {
    match error {
        SelectionError::Conflict { first, second, .. } => match (first.kind(), second.kind()) {
            (ConflictKind::Extra(first_extra), ConflictKind::Extra(second_extra)) => {
                LockError::from(LockErrorKind::ConflictingExtra {
                    package1: first.package().clone(),
                    extra1: first_extra.clone(),
                    package2: second.package().clone(),
                    extra2: second_extra.clone(),
                })
                .to_string()
            }
            (ConflictKind::Project | ConflictKind::Group(_), _)
            | (ConflictKind::Extra(_), ConflictKind::Project | ConflictKind::Group(_)) => {
                format!("conflict: {first:?} / {second:?}")
            }
        },
        SelectionError::Unsatisfiable(environment) => format!("unsatisfiable: {environment:?}"),
        SelectionError::Ambiguous(environment) => format!("ambiguous: {environment:?}"),
        SelectionError::UnknownConflictItem(item) => format!("unknown conflict item: {item}"),
    }
}

struct Target<'a> {
    lock: &'a Lock,
    case: &'a Case,
}

impl<'a> Installable<'a> for Target<'a> {
    fn install_path(&self) -> &'a Path {
        &self.case.directory
    }
    fn lock(&self) -> &'a Lock {
        self.lock
    }
    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        self.case.roots.iter()
    }
    fn project_name(&self) -> Option<&PackageName> {
        self.case.roots.first()
    }
}

/// Construct selection inputs, creating a Boolean restriction only for the experimental reader.
fn environment(
    case: &Case,
    project: bool,
) -> (ResolverMarkerEnvironment, Option<MarkerTree>, Tags) {
    let (system, operating_system, sys_platform, platform) = match case.platform {
        ComparisonPlatform::Linux => (
            "Linux",
            "posix",
            "linux",
            Os::Manylinux {
                major: 2,
                minor: 31,
            },
        ),
        ComparisonPlatform::Darwin => (
            "Darwin",
            "posix",
            "darwin",
            Os::Macos {
                major: 14,
                minor: 0,
            },
        ),
        ComparisonPlatform::Win32 => ("Windows", "nt", "win32", Os::Windows),
    };
    let minor = case.python.split('.').nth(1).expect("Python minor version");
    let short_python = format!("3.{minor}");
    let marker = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: &case.python,
        os_name: operating_system,
        platform_machine: "x86_64",
        platform_python_implementation: "CPython",
        platform_release: "6.8.0",
        platform_system: system,
        platform_version: "comparison",
        python_full_version: &case.python,
        python_version: &short_python,
        sys_platform,
    })
    .expect("comparison marker environment");
    let constraint = project.then(|| format!(
        "implementation_name == 'cpython' and implementation_version == '{}' and os_name == '{}' and platform_machine == 'x86_64' and platform_python_implementation == 'CPython' and platform_release == '6.8.0' and platform_system == '{}' and platform_version == 'comparison' and python_full_version == '{}' and sys_platform == '{}'",
        case.python, operating_system, system, case.python, sys_platform,
    ).parse().expect("concrete comparison environment"));
    let python = (3, minor.parse().expect("Python minor version"));
    let tags = Tags::from_env(
        Platform::new(platform, Arch::X86_64),
        python,
        "cpython",
        python,
        TagsOptions::default(),
    )
    .expect("comparison tags");
    (marker.into(), constraint, tags)
}

enum Output {
    Selection(Resolution),
    Export(String),
}

impl Output {
    /// Compare full package identities and graph edges after the timed operation has finished.
    fn snapshot(self) -> String {
        match self {
            Self::Export(contents) => contents,
            Self::Selection(resolution) => {
                let graph = resolution.graph();
                let labels = graph
                    .node_weights()
                    .map(|node| match node {
                        Node::Root => "root".to_string(),
                        Node::Dist {
                            dist,
                            hashes,
                            install,
                        } => format!("{dist:?} {install} {hashes:?}"),
                    })
                    .collect::<Vec<_>>();
                let mut edges = graph
                    .edge_references()
                    .map(|edge| {
                        format!(
                            "{} --{:?}--> {}",
                            labels[edge.source().index()],
                            edge.weight(),
                            labels[edge.target().index()]
                        )
                    })
                    .collect::<Vec<_>>();
                let mut nodes = labels;
                nodes.sort_unstable();
                edges.sort_unstable();
                serde_json::to_string(&(nodes, edges)).expect("graph snapshot")
            }
        }
    }
}

/// Measure parsing, validation, projection, and the existing materialization or export path.
fn operation(case: &Case) -> Result<Output, String> {
    let experimental = case.action.starts_with("new-");
    let export = case.action.ends_with("export");
    let input = case.directory.join(if experimental {
        "compact.lock"
    } else {
        "baseline.lock"
    });
    let contents = fs_err::read_to_string(input).expect("comparison lockfile");
    let mut lock = Lock::from_toml(&contents).expect("valid comparison lockfile");
    let environment =
        (!export).then(|| environment(case, experimental && !lock.conflicts.is_empty()));
    if experimental && !lock.conflicts.is_empty() {
        let requests = fs_err::read(case.directory.join("requests.json")).expect("request data");
        let graph: RequestGraph =
            serde_json::from_slice(&requests).expect("serialized request graph");
        let constraint = environment
            .as_ref()
            .and_then(|(_, constraint, _)| *constraint);
        lock = graph.project(lock, case, constraint)?;
    }
    let extras = ExtrasSpecification::from_extra(case.extras.clone())
        .with_defaults(DefaultExtras::default());
    let groups = DependencyGroups::from_args(
        None,
        if case.only_groups {
            vec![]
        } else {
            case.groups.clone()
        },
        vec![],
        true,
        if case.only_groups {
            case.groups.clone()
        } else {
            vec![]
        },
        false,
    )
    .with_defaults(DefaultGroups::default());
    let install_options = InstallOptions::default();
    if export {
        let target = Target { lock: &lock, case };
        RequirementsTxtExport::from_lock(
            &target,
            &[],
            &extras,
            &groups,
            false,
            Some(EditableMode::Editable),
            true,
            &install_options,
        )
        .map(|export| Output::Export(export.to_string()))
        .map_err(|error| error.to_string())
    } else {
        let (environment, _, tags) = environment.expect("selection environment");
        let target = Target { lock: &lock, case };
        target
            .to_resolution(
                &environment,
                &tags,
                &extras,
                &groups,
                &BuildOptions::default(),
                &install_options,
            )
            .map(Output::Selection)
            .map_err(|error| error.to_string())
    }
}

/// This ignored harness is invoked once per measured process by the comparison script.
#[test]
#[ignore = "requires an explicit comparison fixture and operation"]
fn compare_selection() {
    let path = PathBuf::from(env::var_os("UV_SELECTION_CASE").expect("comparison case path"));
    let case: Case = serde_json::from_slice(&fs_err::read(&path).expect("comparison case"))
        .expect("valid case JSON");
    if case.action == "prepare" {
        let source =
            fs_err::read_to_string(case.directory.join("uv.lock")).expect("source lockfile");
        let mut lock = Lock::from_toml(&source).expect("source lockfile parses");
        fs_err::write(
            case.directory.join("baseline.lock"),
            lock.to_toml().expect("baseline serialization"),
        )
        .expect("write baseline");
        if !lock.conflicts.is_empty() {
            let requests = RequestGraph::prepare(&lock);
            fs_err::write(
                case.directory.join("requests.json"),
                serde_json::to_vec(&requests).expect("request serialization"),
            )
            .expect("write requests");
            let constraints = conflict_constraint(&lock);
            for package in &mut lock.packages {
                for dependencies in std::iter::once(&mut package.dependencies)
                    .chain(package.optional_dependencies.values_mut())
                    .chain(package.dependency_groups.values_mut())
                {
                    for dependency in dependencies.iter_mut() {
                        let marker = dependency
                            .complexified_marker
                            .combined()
                            .restrict(constraints);
                        dependency.complexified_marker = UniversalMarker::from_combined(marker);
                        dependency.simplified_marker =
                            SimplifiedMarkerTree::new(&lock.requires_python, marker);
                    }
                    dependencies.retain(|dependency| !dependency.complexified_marker.is_false());
                }
            }
        }
        fs_err::write(
            case.directory.join("compact.lock"),
            lock.to_toml().expect("compact serialization"),
        )
        .expect("write compact lock");
        return;
    }
    let start = Instant::now();
    let mut result = None;
    for _ in 0..case.iterations {
        result = Some(black_box(operation(black_box(&case))));
    }
    let elapsed = start.elapsed();
    let peak_rss_kib = fs_err::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
        });
    let result = result
        .expect("at least one iteration")
        .map(Output::snapshot);
    let report = serde_json::json!({
        "action": case.action,
        "iterations": case.iterations,
        "elapsed_ns": elapsed.as_nanos(),
        "peak_rss_kib": peak_rss_kib,
        "result": result,
    });
    fs_err::write(
        path.with_extension("result.json"),
        serde_json::to_vec(&report).expect("comparison report"),
    )
    .expect("write comparison report");
}
