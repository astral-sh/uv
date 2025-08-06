use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use uv_audit::{
    AuditCache, AuditError, AuditReport, DependencyScanner, MatcherConfig, ReportGenerator,
    VulnerabilityMatcher, VulnerabilitySource as AuditVulnerabilitySource,
};
use uv_cache::Cache;
use uv_cli::{AuditFormat, SeverityLevel, VulnerabilitySource};
use uv_configuration::Preview;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Audit Python packages for known security vulnerabilities.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn audit(
    path: Option<&Path>,
    format: AuditFormat,
    severity: SeverityLevel,
    ignore_ids: &[String],
    output: Option<&Path>,
    dev: bool,
    optional: bool,
    direct_only: bool,
    no_cache: bool,
    _cache_dir: Option<&Path>,
    source: VulnerabilitySource,
    cache: &Cache,
    printer: Printer,
    _preview: Preview,
) -> Result<ExitStatus> {
    let project_dir = path.unwrap_or_else(|| Path::new("."));

    writeln!(
        printer.stderr(),
        "Auditing dependencies for vulnerabilities in {}...",
        project_dir.display()
    )?;

    if matches!(printer, Printer::Verbose) {
        writeln!(
            printer.stderr(),
            "Configuration: format={format:?}, severity={severity:?}, source={source:?}, dev={dev}, optional={optional}, direct_only={direct_only}"
        )?;

        if !ignore_ids.is_empty() {
            writeln!(
                printer.stderr(),
                "Ignoring vulnerability IDs: {}",
                ignore_ids.join(", ")
            )?;
        }
    }

    let audit_result = perform_audit(
        project_dir,
        severity,
        ignore_ids,
        dev,
        optional,
        direct_only,
        no_cache,
        source,
        cache,
        &printer,
    )
    .await;

    let report = match audit_result {
        Ok(report) => report,
        Err(e) => {
            return handle_audit_error(&uv_audit::AuditError::Anyhow(e), printer);
        }
    };

    let report_output = ReportGenerator::generate(&report, format, Some(project_dir))
        .map_err(|e| anyhow::anyhow!("Failed to generate report: {e}"))?;

    if let Some(output_path) = output {
        fs_err::write(output_path, &report_output)?;
        writeln!(
            printer.stderr(),
            "Audit results written to: {}",
            output_path.display()
        )?;
    } else {
        writeln!(printer.stdout(), "{report_output}")?;
    }

    if report.has_vulnerabilities() {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

#[allow(clippy::fn_params_excessive_bools)]
async fn perform_audit(
    project_dir: &Path,
    severity: SeverityLevel,
    ignore_ids: &[String],
    dev: bool,
    optional: bool,
    direct_only: bool,
    no_cache: bool,
    source: VulnerabilitySource,
    cache: &Cache,
    printer: &Printer,
) -> Result<AuditReport> {
    let audit_cache = AuditCache::new(cache.clone());

    // Create the vulnerability source
    let vuln_source = AuditVulnerabilitySource::new(source, audit_cache, no_cache);

    // Get source name for display
    let source_name = vuln_source.name();
    writeln!(
        printer.stderr(),
        "Fetching vulnerability data from {source_name}..."
    )?;

    writeln!(printer.stderr(), "Scanning project dependencies...")?;
    let scanner = DependencyScanner::new(dev, optional, direct_only);
    let dependencies = scanner.scan_project(project_dir).await?;

    let dependency_stats = scanner.get_stats(&dependencies);

    if matches!(printer, Printer::Verbose) {
        writeln!(printer.stderr(), "{dependency_stats}")?;
    }

    let warnings = scanner.validate_dependencies(&dependencies);
    for warning in &warnings {
        writeln!(printer.stderr(), "Warning: {warning}")?;
    }

    // Prepare package list for vulnerability fetching
    let packages: Vec<(String, String)> = dependencies
        .iter()
        .map(|dep| (dep.name.to_string(), dep.version.to_string()))
        .collect();

    // Fetch vulnerabilities from the selected source
    writeln!(
        printer.stderr(),
        "Fetching vulnerabilities for {} packages from {}...",
        packages.len(),
        source_name
    )?;
    let database = vuln_source.fetch_vulnerabilities(&packages).await?;

    writeln!(
        printer.stderr(),
        "Matching against vulnerability database..."
    )?;
    let matcher_config = MatcherConfig::new(severity, ignore_ids.to_vec(), direct_only);
    let matcher = VulnerabilityMatcher::new(database, matcher_config);

    let matches = matcher.find_vulnerabilities(&dependencies)?;
    let filtered_matches = matcher.filter_matches(matches);

    let database_stats = matcher.get_database_stats();
    let fix_analysis = matcher.analyze_fixes(&filtered_matches);

    let report = AuditReport::new(
        dependency_stats,
        database_stats,
        filtered_matches,
        fix_analysis,
        warnings,
    );

    let summary = report.summary();
    writeln!(
        printer.stderr(),
        "Audit complete: {} vulnerabilities found in {} packages",
        summary.total_vulnerabilities,
        summary.vulnerable_packages
    )?;

    Ok(report)
}

/// Handle audit errors gracefully
fn handle_audit_error(error: &AuditError, printer: Printer) -> Result<ExitStatus> {
    match error {
        AuditError::NoDependencyInfo => {
            writeln!(printer.stderr(), "Error: No dependency information found.")?;
            writeln!(
                printer.stderr(),
                "Run 'uv lock' to generate a lock file, or ensure you're in a Python project directory."
            )?;
            Ok(ExitStatus::Failure)
        }
        AuditError::DatabaseDownload(e) => {
            writeln!(
                printer.stderr(),
                "Error: Failed to download vulnerability database: {e}"
            )?;
            writeln!(
                printer.stderr(),
                "Check your internet connection and try again."
            )?;
            Ok(ExitStatus::Failure)
        }
        AuditError::DependencyRead(e) => {
            writeln!(
                printer.stderr(),
                "Error: Failed to read project dependencies: {e}"
            )?;
            writeln!(
                printer.stderr(),
                "Ensure you're in a valid Python project directory."
            )?;
            Ok(ExitStatus::Failure)
        }
        AuditError::LockFileParse(e) => {
            writeln!(printer.stderr(), "Error: Failed to parse lock file: {e}")?;
            writeln!(
                printer.stderr(),
                "Try running 'uv lock' to regenerate the lock file."
            )?;
            Ok(ExitStatus::Failure)
        }
        AuditError::Anyhow(e) => {
            writeln!(printer.stderr(), "Error: Audit failed: {e}")?;
            writeln!(
                printer.stderr(),
                "This may be due to a missing or corrupted vulnerability database cache."
            )?;
            writeln!(
                printer.stderr(),
                "Try running 'uv audit --no-cache' to force a fresh download."
            )?;
            Ok(ExitStatus::Failure)
        }
        _ => {
            writeln!(printer.stderr(), "Error: Audit failed: {error}")?;
            Ok(ExitStatus::Failure)
        }
    }
}
