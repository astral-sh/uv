//! Generate the Packse scenario integration tests.

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anstream::println;
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use itertools::Itertools;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{Requirement, VersionOrUrl};
use uv_test::packse::scenario::{Package, PackageMetadata, Scenario};
use walkdir::WalkDir;

use crate::ROOT_DIR;

const GENERATED_FROM: &str = "test/scenarios";
const GENERATED_WITH: &str = "cargo dev generate-scenarios";

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Regenerate only the selected scenario test files.
    #[arg(long, value_enum)]
    templates: Vec<TemplateKind>,

    /// Skip `cargo insta test --accept` after refreshing generated Rust files.
    #[arg(long)]
    no_snapshot_update: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum TemplateKind {
    Install,
    Compile,
    Lock,
}

impl TemplateKind {
    const ALL: [Self; 3] = [Self::Install, Self::Compile, Self::Lock];

    fn test_file(self) -> &'static str {
        match self {
            Self::Install => "crates/uv/tests/it/pip_install_scenarios.rs",
            Self::Compile => "crates/uv/tests/it/pip_compile_scenarios.rs",
            Self::Lock => "crates/uv/tests/it/lock_scenarios.rs",
        }
    }

    fn test_name(self) -> &'static str {
        match self {
            Self::Install => "pip_install_scenarios",
            Self::Compile => "pip_compile_scenarios",
            Self::Lock => "lock_scenarios",
        }
    }
}

struct ScenarioCase {
    scenario: Scenario,
    path: String,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let scenarios = load_scenarios()?;
    println!("Loaded {} scenarios from {GENERATED_FROM}", scenarios.len());

    let selected_templates = if args.templates.is_empty() {
        TemplateKind::ALL.to_vec()
    } else {
        args.templates.clone()
    };

    for template in selected_templates {
        let cases = scenarios_for_template(template, &scenarios);
        let output = render(template, &cases)?;
        let path = Path::new(ROOT_DIR).join(template.test_file());

        println!("Updating: {}", template.test_file());
        fs_err::write(&path, output.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
        format_rust_file(&path)?;

        if args.no_snapshot_update {
            println!("Skipping snapshots for {}", template.test_name());
        } else {
            update_snapshots(template)?;
        }
    }

    Ok(())
}

fn load_scenarios() -> Result<Vec<ScenarioCase>> {
    let scenarios_dir = Path::new(ROOT_DIR).join(GENERATED_FROM);
    let mut entries = WalkDir::new(&scenarios_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "toml")
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            let scenario = Scenario::from_path(&path);
            let relative = path.strip_prefix(&scenarios_dir).with_context(|| {
                format!("scenario path was outside {}", scenarios_dir.display())
            })?;
            Ok(ScenarioCase {
                scenario,
                path: path_to_forward_slashes(relative),
            })
        })
        .collect()
}

fn path_to_forward_slashes(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .join("/")
}

fn scenarios_for_template<'a>(
    template: TemplateKind,
    scenarios: &'a [ScenarioCase],
) -> Vec<&'a ScenarioCase> {
    scenarios
        .iter()
        .filter(|case| match template {
            TemplateKind::Install => {
                !case.scenario.resolver_options.universal
                    && case.scenario.resolver_options.python.is_none()
            }
            TemplateKind::Compile => {
                !case.scenario.resolver_options.universal
                    && case.scenario.resolver_options.python.is_some()
            }
            TemplateKind::Lock => case.scenario.resolver_options.universal,
        })
        .collect()
}

fn format_rust_file(path: &Path) -> Result<()> {
    let status = Command::new("rustfmt")
        .arg(path)
        .stdout(Stdio::null())
        .status()
        .context("failed to run rustfmt")?;
    if !status.success() {
        bail!("rustfmt failed for {}", path.display());
    }
    Ok(())
}

fn update_snapshots(template: TemplateKind) -> Result<()> {
    println!("Updating snapshots for {}", template.test_name());
    let status = Command::new("cargo")
        .args([
            "insta",
            "test",
            "--features",
            "test-python,test-python-patch",
            "--accept",
            "--test-runner",
            "nextest",
            "--test",
            "it",
            "--",
            template.test_name(),
        ])
        .current_dir(ROOT_DIR)
        .status()
        .context("failed to run cargo insta")?;
    if !status.success() {
        bail!("snapshot update failed for {}", template.test_name());
    }
    Ok(())
}

fn render(template: TemplateKind, cases: &[&ScenarioCase]) -> Result<String> {
    let mut output = String::new();
    match template {
        TemplateKind::Install => render_install(&mut output, cases)?,
        TemplateKind::Compile => render_compile(&mut output, cases)?,
        TemplateKind::Lock => render_lock(&mut output, cases)?,
    }
    Ok(output)
}

fn render_header(output: &mut String) {
    output.push_str("//! DO NOT EDIT\n");
    output.push_str("//!\n");
    writeln!(output, "//! Generated with `{GENERATED_WITH}`").unwrap();
    writeln!(output, "//! Scenarios from <{GENERATED_FROM}>").unwrap();
    output.push_str("//!\n");
}

fn render_install(output: &mut String, cases: &[&ScenarioCase]) -> Result<()> {
    render_header(output);
    output.push_str("#![cfg(all(feature = \"test-python\", unix))]\n\n");
    output.push_str("use std::process::Command;\n\n");
    output.push_str("use uv_static::EnvVars;\n\n");
    output.push_str("use uv_test::packse::PackseServer;\n");
    output.push_str("use uv_test::{TestContext, uv_snapshot};\n\n");
    output
        .push_str("/// Create a `pip install` command with options shared across all scenarios.\n");
    output.push_str("fn command(context: &TestContext, server: &PackseServer) -> Command {\n");
    output.push_str("    let mut command = context.pip_install();\n");
    output.push_str("    command.arg(\"--index-url\").arg(server.index_url());\n");
    output.push_str("    command.env_remove(EnvVars::UV_EXCLUDE_NEWER);\n");
    output.push_str("    command\n");
    output.push_str("}\n\n");

    for case in cases {
        render_install_case(output, case)?;
    }
    Ok(())
}

fn render_install_case(output: &mut String, case: &ScenarioCase) -> Result<()> {
    render_case_docs(output, &case.scenario)?;
    if case.scenario.name.contains("patch") {
        output.push_str("#[cfg(feature = \"test-python-patch\")]\n");
    }
    output.push_str("#[test]\n");
    writeln!(output, "fn {}() {{", module_name(&case.scenario.name)).unwrap();
    writeln!(
        output,
        "    let context = uv_test::test_context!(\"{}\");",
        case.scenario.environment.python
    )
    .unwrap();
    writeln!(
        output,
        "    let server = PackseServer::new(\"{}\");",
        case.path
    )
    .unwrap();
    output.push('\n');
    output.push_str("    uv_snapshot!(context.filters(), command(&context, &server)\n");
    render_resolver_args(output, &case.scenario, ScenarioCommand::Install)?;
    output.push_str("        , @r#\"<snapshot>\n");
    output.push_str("    \"#);\n\n");
    render_expected_explanation(output, &case.scenario, "    // ");
    render_install_assertions(output, &case.scenario);
    output.push_str("}\n\n");
    Ok(())
}

fn render_compile(output: &mut String, cases: &[&ScenarioCase]) -> Result<()> {
    render_header(output);
    output.push_str("#![cfg(all(feature = \"test-python\", unix))]\n\n");
    output.push_str("use std::process::Command;\n\n");
    output.push_str("use anyhow::Result;\n");
    output.push_str("use assert_cmd::assert::OutputAssertExt;\n");
    output.push_str("use assert_fs::fixture::{FileWriteStr, PathChild};\n");
    output.push_str("use predicates::prelude::predicate;\n\n");
    output.push_str("use uv_static::EnvVars;\n\n");
    output.push_str("use uv_test::packse::PackseServer;\n");
    output.push_str(
        "use uv_test::{TestContext, get_bin, python_path_with_versions, uv_snapshot};\n\n",
    );
    output.push_str("/// Provision python binaries and return a `pip compile` command with options shared across all scenarios.\n");
    output.push_str("fn command(context: &TestContext, python_versions: &[&str], server: &PackseServer) -> Command {\n");
    output.push_str(
        "    let python_path = python_path_with_versions(&context.temp_dir, python_versions)\n",
    );
    output.push_str("        .expect(\"Failed to create Python test path\");\n");
    output.push_str("    let mut command = Command::new(get_bin!());\n");
    output.push_str("    command\n");
    output.push_str("        .arg(\"pip\")\n");
    output.push_str("        .arg(\"compile\")\n");
    output.push_str("        .arg(\"requirements.in\")\n");
    output.push_str("        .arg(\"--index-url\")\n");
    output.push_str("        .arg(server.index_url());\n");
    output.push_str("    context.add_shared_options(&mut command, true);\n");
    output.push_str("    command.env_remove(EnvVars::UV_EXCLUDE_NEWER);\n");
    output.push_str("    command.env(EnvVars::UV_PYTHON_SEARCH_PATH, python_path);\n\n");
    output.push_str("    command\n");
    output.push_str("}\n\n");

    for case in cases {
        render_compile_case(output, case)?;
    }
    Ok(())
}

fn render_compile_case(output: &mut String, case: &ScenarioCase) -> Result<()> {
    render_case_docs(output, &case.scenario)?;
    if case.scenario.name.contains("patch") {
        output.push_str("#[cfg(feature = \"test-python-patch\")]\n");
    }
    output.push_str("#[test]\n");
    writeln!(
        output,
        "fn {}() -> Result<()> {{",
        module_name(&case.scenario.name)
    )
    .unwrap();
    writeln!(
        output,
        "    let context = uv_test::test_context!(\"{}\");",
        case.scenario.environment.python
    )
    .unwrap();
    write!(output, "    let python_versions = &[").unwrap();
    for version in &case.scenario.environment.additional_python {
        write!(output, "\"{version}\", ").unwrap();
    }
    output.push_str("];\n");
    writeln!(
        output,
        "    let server = PackseServer::new(\"{}\");",
        case.path
    )
    .unwrap();
    output.push('\n');
    output.push_str("    let requirements_in = context.temp_dir.child(\"requirements.in\");\n");
    for requirement in &case.scenario.root.requires {
        writeln!(
            output,
            "    requirements_in.write_str(\"{}\")?;",
            requirement
        )
        .unwrap();
    }
    output.push('\n');
    render_expected_explanation(output, &case.scenario, "    // ");
    output.push_str(
        "    let output = uv_snapshot!(context.filters(), command(&context, python_versions, &server)\n",
    );
    render_resolver_args(output, &case.scenario, ScenarioCommand::Compile)?;
    output.push_str("        , @r###\"<snapshot>\n");
    output.push_str("    \"###\n");
    output.push_str("    );\n\n");
    output.push_str("    output\n");
    output.push_str("        .assert()\n");
    if case.scenario.expected.satisfiable {
        output.push_str("        .success()\n");
        for (name, version) in &case.scenario.expected.packages {
            writeln!(
                output,
                "            .stdout(predicate::str::contains(\"{name}=={version}\"))"
            )
            .unwrap();
        }
    } else {
        output.push_str("        .failure()\n");
    }
    output.push_str("    ;\n\n");
    output.push_str("    Ok(())\n");
    output.push_str("}\n\n");
    Ok(())
}

fn render_lock(output: &mut String, cases: &[&ScenarioCase]) -> Result<()> {
    render_header(output);
    output.push_str("#![cfg(feature = \"test-python\")]\n");
    output.push_str("#![expect(clippy::needless_raw_string_hashes)]\n");
    output.push_str("#![expect(clippy::doc_markdown)]\n");
    output.push_str("#![expect(clippy::doc_lazy_continuation)]\n\n");
    output.push_str("use anyhow::Result;\n");
    output.push_str("use assert_cmd::assert::OutputAssertExt;\n");
    output.push_str("use assert_fs::prelude::*;\n");
    output.push_str("use insta::assert_snapshot;\n\n");
    output.push_str("use uv_static::EnvVars;\n\n");
    output.push_str("use uv_test::packse::PackseServer;\n");
    output.push_str("use uv_test::uv_snapshot;\n\n");

    for case in cases {
        render_lock_case(output, case)?;
    }
    Ok(())
}

fn render_lock_case(output: &mut String, case: &ScenarioCase) -> Result<()> {
    render_case_docs(output, &case.scenario)?;
    output.push_str("#[test]\n");
    writeln!(
        output,
        "fn {}() -> Result<()> {{",
        module_name(&case.scenario.name)
    )
    .unwrap();
    writeln!(
        output,
        "    let context = uv_test::test_context!(\"{}\");",
        case.scenario.environment.python
    )
    .unwrap();
    writeln!(
        output,
        "    let server = PackseServer::new(\"{}\");",
        case.path
    )
    .unwrap();
    output.push('\n');
    output.push_str("    let pyproject_toml = context.temp_dir.child(\"pyproject.toml\");\n");
    output.push_str("    pyproject_toml.write_str(\n");
    output.push_str("        r###\"\n");
    output.push_str("        [project]\n");
    output.push_str("        name = \"project\"\n");
    output.push_str("        version = \"0.1.0\"\n");
    output.push_str("        dependencies = [\n");
    for requirement in &case.scenario.root.requires {
        writeln!(output, "          '''{requirement}''',").unwrap();
    }
    output.push_str("        ]\n");
    if let Some(requires_python) = &case.scenario.root.requires_python {
        writeln!(output, "        requires-python = \"{requires_python}\"").unwrap();
    }
    if !case
        .scenario
        .resolver_options
        .required_environments
        .is_empty()
    {
        output.push_str("        [tool.uv]\n");
        output.push_str("        required-environments = [\n");
        for environment in &case.scenario.resolver_options.required_environments {
            let environment = environment
                .contents()
                .context("required environment markers should not be empty")?;
            writeln!(output, "          '''{environment}''',").unwrap();
        }
        output.push_str("        ]\n");
    }
    output.push_str("        \"###\n");
    output.push_str("    )?;\n\n");
    output.push_str("    let mut filters = context.filters();\n");
    output.push_str("    // The \"hint\" about non-current environments is platform-dependent, so filter it out.\n");
    output.push_str("    filters.push((r\"\\n\\s+hint: .*\", \"\"));\n\n");
    output.push_str("    let mut cmd = context.lock();\n");
    output.push_str("    cmd.env_remove(EnvVars::UV_EXCLUDE_NEWER);\n");
    output.push_str("    cmd.arg(\"--index-url\").arg(server.index_url());\n");
    render_expected_explanation(output, &case.scenario, "    // ");
    output.push_str("    uv_snapshot!(filters, cmd, @r###\"<snapshot>\n");
    output.push_str("    \"###\n");
    output.push_str("    );\n\n");
    if case.scenario.expected.satisfiable {
        output.push_str("    let lock = context.read(\"uv.lock\");\n");
        output.push_str("    insta::with_settings!({\n");
        output.push_str("        filters => filters,\n");
        output.push_str("    }, {\n");
        output.push_str("        assert_snapshot!(\n");
        output.push_str("            lock, @r###\"<snapshot>\n");
        output.push_str("            \"###\n");
        output.push_str("        );\n");
        output.push_str("    });\n\n");
        output.push_str("    // Assert the idempotence of `uv lock` when resolving from the lockfile (`--locked`).\n");
        output.push_str("    context\n");
        output.push_str("        .lock()\n");
        output.push_str("        .arg(\"--locked\")\n");
        output.push_str("        .env_remove(EnvVars::UV_EXCLUDE_NEWER)\n");
        output.push_str("        .arg(\"--index-url\")\n");
        output.push_str("        .arg(server.index_url())\n");
        output.push_str("        .assert()\n");
        output.push_str("        .success();\n");
    }
    output.push('\n');
    output.push_str("    Ok(())\n");
    output.push_str("}\n\n");
    Ok(())
}

#[derive(Copy, Clone)]
enum ScenarioCommand {
    Install,
    Compile,
}

fn render_resolver_args(
    output: &mut String,
    scenario: &Scenario,
    command: ScenarioCommand,
) -> Result<()> {
    if scenario.resolver_options.prereleases {
        output.push_str("        .arg(\"--prerelease=allow\")\n");
    }
    for package in &scenario.resolver_options.no_build {
        writeln!(output, "        .arg(\"--only-binary\")").unwrap();
        writeln!(output, "        .arg(\"{package}\")").unwrap();
    }
    for package in &scenario.resolver_options.no_binary {
        writeln!(output, "        .arg(\"--no-binary\")").unwrap();
        writeln!(output, "        .arg(\"{package}\")").unwrap();
    }
    if let Some(platform) = &scenario.resolver_options.python_platform {
        let platform = platform
            .to_possible_value()
            .context("target triple should have a clap representation")?;
        writeln!(
            output,
            "        .arg(\"--python-platform={}\")",
            platform.get_name()
        )
        .unwrap();
    }
    if matches!(command, ScenarioCommand::Compile)
        && let Some(python) = &scenario.resolver_options.python
    {
        writeln!(output, "        .arg(\"--python-version={python}\")").unwrap();
    }
    if matches!(command, ScenarioCommand::Install) {
        for requirement in &scenario.root.requires {
            writeln!(output, "        .arg(\"{requirement}\")").unwrap();
        }
    }
    Ok(())
}

fn render_install_assertions(output: &mut String, scenario: &Scenario) {
    if scenario.expected.satisfiable {
        for (name, version) in &scenario.expected.packages {
            writeln!(
                output,
                "    context.assert_installed(\"{}\", \"{version}\");",
                module_name(name.as_ref())
            )
            .unwrap();
        }
    } else {
        for requirement in &scenario.root.requires {
            writeln!(
                output,
                "    context.assert_not_installed(\"{}\");",
                module_name(requirement.name.as_ref())
            )
            .unwrap();
        }
    }
}

fn render_expected_explanation(output: &mut String, scenario: &Scenario, prefix: &str) {
    if let Some(explanation) = &scenario.expected.explanation {
        for line in explanation.lines() {
            writeln!(output, "{prefix}{line}").unwrap();
        }
    }
}

fn render_case_docs(output: &mut String, scenario: &Scenario) -> Result<()> {
    output.push_str("/// ");
    if let Some(description) = &scenario.description {
        output.push_str(&description.replace('\n', "\n/// "));
    } else {
        output.push_str(&scenario.name);
    }
    output.push_str("\n///\n");
    output.push_str("/// ```text\n");
    writeln!(output, "/// {}", scenario.name).unwrap();
    for line in pretty_tree(scenario)? {
        writeln!(output, "/// {line}").unwrap();
    }
    output.push_str("/// ```\n");
    Ok(())
}

fn pretty_tree(scenario: &Scenario) -> Result<Vec<String>> {
    const SPACE: &str = "    ";
    const BRANCH: &str = "│   ";
    const TEE: &str = "├── ";
    const LAST: &str = "└── ";

    let mut lines = Vec::new();
    lines.push(format!("{TEE}environment"));
    if scenario.environment.additional_python.is_empty() {
        lines.push(format!(
            "{BRANCH}{LAST}python{}",
            scenario.environment.python
        ));
    } else {
        let mut versions = scenario.environment.additional_python.clone();
        versions.push(scenario.environment.python.clone());
        versions.sort_by_key(ToString::to_string);
        for (index, version) in versions.iter().enumerate() {
            let pointer = if index + 1 == versions.len() {
                LAST
            } else {
                TEE
            };
            let active = if version == &scenario.environment.python {
                " (active)"
            } else {
                ""
            };
            lines.push(format!("{BRANCH}{pointer}python{version}{active}"));
        }
    }

    let root_pointer = if scenario.packages.is_empty() {
        LAST
    } else {
        TEE
    };
    lines.push(format!("{root_pointer}root"));
    let root_prefix = if root_pointer == TEE { BRANCH } else { SPACE };
    render_requirements(&mut lines, scenario, &scenario.root.requires, root_prefix)?;

    let package_names = scenario.packages.keys().cloned().collect::<Vec<_>>();
    for (index, package_name) in package_names.iter().enumerate() {
        let pointer = if index + 1 == package_names.len() {
            LAST
        } else {
            TEE
        };
        lines.push(format!("{pointer}{package_name}"));
        let prefix = if pointer == TEE { BRANCH } else { SPACE };
        render_versions(&mut lines, scenario, package_name, prefix, None)?;
    }

    Ok(lines)
}

fn render_requirements(
    lines: &mut Vec<String>,
    scenario: &Scenario,
    requirements: &[Requirement],
    prefix: &str,
) -> Result<()> {
    const SPACE: &str = "    ";
    const BRANCH: &str = "│   ";
    const TEE: &str = "├── ";
    const LAST: &str = "└── ";

    let mut filtered = requirements
        .iter()
        .filter(|requirement| !omit_python_requirement(scenario, requirement))
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| left.name.cmp(&right.name));

    for (index, requirement) in filtered.iter().enumerate() {
        let pointer = if index + 1 == filtered.len() {
            LAST
        } else {
            TEE
        };
        if requirement.name.as_ref() == "python" {
            let suffix = if requirement_specifiers(requirement).is_some_and(|specifiers| {
                !specifiers.contains(&scenario.environment.python.python_version())
            }) {
                " (incompatible with environment)"
            } else {
                ""
            };
            lines.push(format!("{prefix}{pointer}requires {requirement}{suffix}"));
            continue;
        }

        lines.push(format!("{prefix}{pointer}requires {requirement}"));
        if scenario.packages.contains_key(&requirement.name) {
            let next_prefix = format!("{prefix}{}", if pointer == TEE { BRANCH } else { SPACE });
            render_versions(
                lines,
                scenario,
                &requirement.name,
                &next_prefix,
                Some(requirement),
            )?;
        } else {
            lines.push(format!(
                "{prefix}{SPACE}{LAST}unsatisfied: no versions for package"
            ));
        }
    }

    Ok(())
}

fn omit_python_requirement(scenario: &Scenario, requirement: &Requirement) -> bool {
    if requirement.name.as_ref() != "python" {
        return false;
    }
    let Some(specifiers) = requirement_specifiers(requirement) else {
        return false;
    };
    let specifiers = specifiers.iter().collect::<Vec<_>>();
    specifiers.len() == 1
        && specifiers[0].version() == &scenario.environment.python.python_version()
        && requirement_specifiers(requirement).is_some_and(|specifiers| {
            specifiers.contains(&scenario.environment.python.python_version())
        })
}

fn render_versions(
    lines: &mut Vec<String>,
    scenario: &Scenario,
    package_name: &PackageName,
    prefix: &str,
    for_requirement: Option<&Requirement>,
) -> Result<()> {
    const SPACE: &str = "    ";
    const BRANCH: &str = "│   ";
    const TEE: &str = "├── ";
    const LAST: &str = "└── ";

    let package = scenario
        .packages
        .get(package_name)
        .with_context(|| format!("missing package metadata for {package_name}"))?;

    let versions = matching_versions(package, for_requirement);
    if for_requirement.is_some() && versions.is_empty() {
        lines.push(format!("{prefix}{LAST}unsatisfied: no matching version"));
        return Ok(());
    }

    let rows = version_rows(package_name, &versions);
    for (index, row) in rows.iter().enumerate() {
        let pointer = if index + 1 == rows.len() { LAST } else { TEE };
        let satisfied = if for_requirement.is_some() {
            "satisfied by "
        } else {
            ""
        };
        let yanked = if row.metadata.yanked { " (yanked)" } else { "" };
        lines.push(format!(
            "{prefix}{pointer}{satisfied}{package_name}-{}{yanked}",
            row.label
        ));
        if for_requirement.is_none() {
            let next_prefix = format!("{prefix}{}", if pointer == TEE { BRANCH } else { SPACE });
            render_requirements(lines, scenario, &row.requirements, &next_prefix)?;
        }
    }

    Ok(())
}

fn matching_versions<'a>(
    package: &'a Package,
    requirement: Option<&Requirement>,
) -> Vec<(&'a Version, &'a PackageMetadata)> {
    package
        .versions
        .iter()
        .filter(|(version, metadata)| {
            if metadata.yanked {
                return requirement.is_none();
            }
            requirement
                .and_then(requirement_specifiers)
                .is_none_or(|specifiers| specifiers.contains(version))
        })
        .collect()
}

struct VersionRow<'a> {
    label: String,
    requirements: Vec<Requirement>,
    metadata: &'a PackageMetadata,
}

fn version_rows<'a>(
    _package_name: &PackageName,
    versions: &[(&Version, &'a PackageMetadata)],
) -> Vec<VersionRow<'a>> {
    let mut rows = Vec::new();
    for (version, metadata) in versions {
        let mut requirements = metadata.requires.clone();
        if let Some(requires_python) = &metadata.requires_python {
            let requirement = format!("python{requires_python}")
                .parse()
                .expect("generated Python requirement should parse");
            requirements.push(requirement);
        }
        rows.push(VersionRow {
            label: version.to_string(),
            requirements,
            metadata,
        });
        for (extra, requirements) in &metadata.extras {
            rows.push(VersionRow {
                label: format!("{version}[{extra}]"),
                requirements: requirements.clone(),
                metadata,
            });
        }
    }
    rows
}

fn module_name(name: &str) -> String {
    name.replace('-', "_")
}

fn requirement_specifiers(requirement: &Requirement) -> Option<&uv_pep440::VersionSpecifiers> {
    match requirement.version_or_url.as_ref()? {
        VersionOrUrl::VersionSpecifier(specifiers) => Some(specifiers),
        VersionOrUrl::Url(_) => None,
    }
}
