use std::fmt::Write as _;
use std::io;

use anyhow::{Result, bail};
use rustc_hash::FxHashSet;

use uv_audit::{VulnerabilityID, VulnerabilityServiceFormat};
use uv_cache::Cache;
use uv_cli::AuditOutputFormat;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, ExtrasSpecification};
use uv_fs::Simplified;
use uv_normalize::{DefaultExtras, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::{Lock, LockParseError};
use uv_settings::{Combine, ResolverInstallerOptions};
use uv_tool::InstalledTools;
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::commands::project::audit::{
    AuditResults, artifact_uri, audit_lock, json, sarif, warn_unmatched_ignores,
};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Audit selected installed tools, or every installed tool if no names are provided.
pub(crate) async fn audit(
    names: Vec<PackageName>,
    output_format: AuditOutputFormat,
    service: VulnerabilityServiceFormat,
    service_url: Option<DisplaySafeUrl>,
    ignore: Vec<VulnerabilityID>,
    ignore_until_fixed: Vec<VulnerabilityID>,
    filesystem: ResolverInstallerOptions,
    client_builder: BaseClientBuilder<'_>,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let mut missing_features = Vec::new();
    if !preview.is_enabled(PreviewFeature::AuditCommand) {
        missing_features.push("audit");
    }
    if !preview.is_enabled(PreviewFeature::ToolInstallLocks) {
        missing_features.push("tool-install-locks");
    }
    if !missing_features.is_empty() {
        warn_user!(
            "`uv tool audit` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            missing_features.join(",")
        );
    }
    if matches!(output_format, AuditOutputFormat::Json)
        && !preview.is_enabled(PreviewFeature::JsonOutput)
    {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    let installed_tools = InstalledTools::from_settings()?;
    let _lock = match installed_tools.lock().await {
        Ok(lock) => lock,
        Err(error)
            if error
                .as_io_error()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            if let Some(name) = names.first() {
                bail!("`{name}` is not installed; run `uv tool install {name}` to install");
            }
            if matches!(output_format, AuditOutputFormat::Text) {
                writeln!(printer.stderr(), "No tools installed")?;
                return Ok(ExitStatus::Success);
            }
            return render_audits(&[], output_format, printer);
        }
        Err(error) => return Err(error.into()),
    };

    let explicit_tool = !names.is_empty();
    let mut tools = if names.is_empty() {
        installed_tools.tools()?
    } else {
        let mut tools = Vec::with_capacity(names.len());
        for name in names {
            match installed_tools.get_tool_receipt(&name) {
                Ok(Some(tool)) => tools.push((name, Ok(tool))),
                Ok(None) => {
                    bail!("`{name}` is not installed; run `uv tool install {name}` to install");
                }
                Err(error) => {
                    bail!("Tool `{name}` has an invalid receipt: {error}");
                }
            }
        }
        tools
    };
    tools.sort_by(|(left, _), (right, _)| left.cmp(right));
    tools.dedup_by(|(left, _), (right, _)| left == right);

    if tools.is_empty() {
        if matches!(output_format, AuditOutputFormat::Text) {
            writeln!(printer.stderr(), "No tools installed")?;
            return Ok(ExitStatus::Success);
        }
        return render_audits(&[], output_format, printer);
    }

    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let mut audits = Vec::new();
    let mut matched_ignores = FxHashSet::default();

    for (name, tool) in tools {
        let tool = match tool {
            Ok(tool) => tool,
            Err(error) => {
                if explicit_tool {
                    bail!("Tool `{name}` has an invalid receipt: {error}");
                }
                warn_user!(
                    "Ignoring malformed tool `{name}` (run `uv tool uninstall {name}` to remove)"
                );
                continue;
            }
        };

        let root = installed_tools.tool_dir(&name);
        let lock_path = root.join("uv.lock");
        let contents = match fs_err::read_to_string(&lock_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if explicit_tool {
                    bail!(
                        "Tool `{name}` does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it"
                    );
                }
                warn_user!(
                    "Skipping tool `{name}` because it does not have a lockfile; reinstall it with `--preview-features tool-install-locks` to audit it"
                );
                continue;
            }
            Err(error) => {
                if explicit_tool {
                    bail!(
                        "Failed to read the lockfile for tool `{name}` at `{}`: {error}",
                        lock_path.user_display()
                    );
                }
                warn_user!(
                    "Skipping tool `{name}` because its lockfile at `{}` could not be read: {error}",
                    lock_path.user_display()
                );
                continue;
            }
        };
        let lock = match Lock::from_toml(&contents) {
            Ok(lock) => lock,
            Err(
                LockParseError::UnsupportedVersion { supported, version }
                | LockParseError::UnparsableVersion {
                    supported, version, ..
                },
            ) => {
                if explicit_tool {
                    bail!(
                        "The lockfile for tool `{name}` at `{}` uses an unsupported schema version (v{version}, but only v{supported} is supported)",
                        lock_path.user_display()
                    );
                }
                warn_user!(
                    "Skipping tool `{name}` because its lockfile at `{}` uses an unsupported schema version (v{version}, but only v{supported} is supported)",
                    lock_path.user_display()
                );
                continue;
            }
            Err(LockParseError::Toml(error)) => {
                if explicit_tool {
                    bail!(
                        "Failed to parse the lockfile for tool `{name}` at `{}`: {error}",
                        lock_path.user_display()
                    );
                }
                warn_user!(
                    "Skipping tool `{name}` because its lockfile at `{}` is invalid: {error}",
                    lock_path.user_display()
                );
                continue;
            }
        };

        let settings = ResolverInstallerSettings::from(
            ResolverInstallerOptions::from(tool.options().clone()).combine(filesystem.clone()),
        );
        let outcome = audit_lock(
            &lock,
            &root,
            &extras,
            &groups,
            &settings.resolver,
            client_builder.clone(),
            concurrency.clone(),
            cache,
            printer,
            service,
            service_url.clone(),
            &ignore,
            &ignore_until_fixed,
        )
        .await?;

        matched_ignores.extend(outcome.matched_ignores);
        audits.push((
            name,
            AuditResults {
                printer,
                n_packages: outcome.n_packages,
                output_format,
                findings: outcome.findings,
                artifact_uri: artifact_uri(&lock_path),
            },
        ));
    }

    warn_unmatched_ignores(
        &ignore,
        &ignore_until_fixed,
        &matched_ignores,
        "the selected tools",
    );

    if audits.is_empty() && matches!(output_format, AuditOutputFormat::Text) {
        writeln!(printer.stderr(), "No auditable tools installed")?;
        return Ok(ExitStatus::Success);
    }

    render_audits(&audits, output_format, printer)
}

fn render_audits(
    audits: &[(PackageName, AuditResults)],
    output_format: AuditOutputFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    match output_format {
        AuditOutputFormat::Text => {
            for (name, results) in audits {
                writeln!(printer.stderr(), "Auditing `{name}`")?;
                if !results.findings.is_empty() {
                    writeln!(printer.stdout_important(), "Tool `{name}`:")?;
                }
                results.render()?;
            }
        }
        AuditOutputFormat::Json => {
            let report = json::ToolReports::from_audits(audits);
            writeln!(
                printer.stdout_important(),
                "{}",
                serde_json::to_string_pretty(&report)?
            )?;
        }
        AuditOutputFormat::Sarif => {
            let report = sarif::Report::from_audits(audits);
            writeln!(
                printer.stdout_important(),
                "{}",
                serde_json::to_string_pretty(&report)?
            )?;
        }
    }

    Ok(
        if audits
            .iter()
            .any(|(_, results)| matches!(results.exit_status(), ExitStatus::Failure))
        {
            ExitStatus::Failure
        } else {
            ExitStatus::Success
        },
    )
}
