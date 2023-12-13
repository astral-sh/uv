use std::fmt::Write;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use fs_err as fs;
use itertools::Itertools;
use tracing::debug;

use distribution_types::{AnyDist, Metadata};
use install_wheel_rs::linker::LinkMode;
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_installer::{Downloader, InstallPlan, Reinstall, SitePackages};
use puffin_interpreter::Virtualenv;
use puffin_resolver::{
    Graph, Manifest, PreReleaseMode, Resolution, ResolutionMode, ResolutionOptions, Resolver,
};
use puffin_traits::OnceMap;
use pypi_types::IndexUrls;

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ChangeEvent, ChangeEventKind, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

/// Install packages into the current environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pip_install(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification<'_>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    index_urls: IndexUrls,
    reinstall: &Reinstall,
    link_mode: LinkMode,
    no_build: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .build(),
        )
    }))?;

    let start = std::time::Instant::now();

    // Determine the requirements.
    let spec = specification(requirements, constraints, overrides, extras)?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;
    debug!(
        "Using Python interpreter: {}",
        venv.python_executable().display()
    );

    // If the requirements are already satisfied, we're done. Ideally, the resolver would be fast
    // enough to let us remove this check. But right now, for large environments, it's an order of
    // magnitude faster to validate the environment than to resolve the requirements.
    if reinstall.is_none() && satisfied(&spec, &venv)? {
        writeln!(
            printer,
            "{}",
            format!(
                "Audited {} in {}",
                format!("{} package{}", spec.requirements.len(), "s").bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        return Ok(ExitStatus::Success);
    }

    // Resolve the requirements.
    let resolution = resolve(
        spec,
        reinstall,
        resolution_mode,
        prerelease_mode,
        &index_urls,
        no_build,
        exclude_newer,
        &cache,
        &venv,
        printer,
    )
    .await?
    .into();

    // Sync the environment.
    install(
        &resolution,
        reinstall,
        link_mode,
        index_urls,
        no_build,
        &cache,
        &venv,
        printer,
    )
    .await?;

    // Validate the environment.
    validate(&resolution, &venv, printer)?;

    Ok(ExitStatus::Success)
}

/// Consolidate the requirements for an installation.
fn specification(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: &ExtrasSpecification<'_>,
) -> Result<RequirementsSpecification> {
    // If the user requests `extras` but does not provide a pyproject toml source
    if !matches!(extras, ExtrasSpecification::None)
        && !requirements
            .iter()
            .any(|source| matches!(source, RequirementsSource::PyprojectToml(_)))
    {
        return Err(anyhow!(
            "Requesting extras requires a pyproject.toml input file."
        ));
    }

    // Read all requirements from the provided sources.
    let spec =
        RequirementsSpecification::from_sources(requirements, constraints, overrides, extras)?;

    // Check that all provided extras are used
    if let ExtrasSpecification::Some(extras) = extras {
        let mut unused_extras = extras
            .iter()
            .filter(|extra| !spec.extras.contains(extra))
            .collect::<Vec<_>>();
        if !unused_extras.is_empty() {
            unused_extras.sort_unstable();
            unused_extras.dedup();
            let s = if unused_extras.len() == 1 { "" } else { "s" };
            return Err(anyhow!(
                "Requested extra{s} not found: {}",
                unused_extras.iter().join(", ")
            ));
        }
    }

    Ok(spec)
}

/// Returns `true` if the requirements are already satisfied.
fn satisfied(spec: &RequirementsSpecification, venv: &Virtualenv) -> Result<bool> {
    SitePackages::from_executable(venv)?.satisfies(&spec.requirements, &spec.constraints)
}

/// Resolve a set of requirements, similar to running `pip-compile`.
#[allow(clippy::too_many_arguments)]
async fn resolve(
    spec: RequirementsSpecification,
    reinstall: &Reinstall,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    index_urls: &IndexUrls,
    no_build: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: &Cache,
    venv: &Virtualenv,
    mut printer: Printer,
) -> Result<Graph> {
    let start = std::time::Instant::now();

    // Create a manifest of the requirements.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        extras: _,
    } = spec;

    // Respect preferences from the existing environments.
    let preferences: Vec<Requirement> = match reinstall {
        Reinstall::All => vec![],
        Reinstall::None => SitePackages::from_executable(venv)?
            .requirements()
            .collect(),
        Reinstall::Packages(packages) => SitePackages::from_executable(venv)?
            .requirements()
            .filter(|requirement| !packages.contains(&requirement.name))
            .collect(),
    };

    let manifest = Manifest::new(requirements, constraints, overrides, preferences, project);
    let options = ResolutionOptions::new(resolution_mode, prerelease_mode, exclude_newer);

    debug!(
        "Using Python {} at {}",
        venv.interpreter().markers().python_version,
        venv.python_executable().display()
    );

    // Determine the compatible platform tags.
    let tags = Tags::from_interpreter(venv.interpreter())?;

    // Determine the interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();

    // Determine the markers to use for resolution.
    let markers = venv.interpreter().markers();

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_urls.clone())
        .build();

    let build_dispatch = BuildDispatch::new(
        client.clone(),
        cache.clone(),
        interpreter,
        fs::canonicalize(venv.python_executable())?,
        no_build,
        index_urls.clone(),
    )
    .with_options(options);

    // Resolve the dependencies.
    let resolver = Resolver::new(manifest, options, markers, &tags, &client, &build_dispatch)
        .with_reporter(ResolverReporter::from(printer));
    let resolution = match resolver.resolve().await {
        Err(puffin_resolver::ResolveError::NoSolution(err)) => {
            #[allow(clippy::print_stderr)]
            {
                let report = miette::Report::msg(format!("{err}"))
                    .context("No solution found when resolving dependencies:");
                eprint!("{report:?}");
            }
            return Err(puffin_resolver::ResolveError::NoSolution(err).into());
        }
        result => result,
    }?;

    let s = if resolution.len() == 1 { "" } else { "s" };
    writeln!(
        printer,
        "{}",
        format!(
            "Resolved {} in {}",
            format!("{} package{}", resolution.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    Ok(resolution)
}

/// Install a set of requirements into the current environment.
#[allow(clippy::too_many_arguments)]
async fn install(
    resolution: &Resolution,
    reinstall: &Reinstall,
    link_mode: LinkMode,
    index_urls: IndexUrls,
    no_build: bool,
    cache: &Cache,
    venv: &Virtualenv,
    mut printer: Printer,
) -> Result<()> {
    let start = std::time::Instant::now();

    // Determine the current environment markers.
    let markers = venv.interpreter().markers();
    let tags = Tags::from_interpreter(venv.interpreter())?;

    // Partition into those that should be linked from the cache (`local`), those that need to be
    // downloaded (`remote`), and those that should be removed (`extraneous`).
    let InstallPlan {
        local,
        remote,
        reinstalls,
        extraneous: _,
    } = InstallPlan::from_requirements(
        &resolution.requirements(),
        reinstall,
        &index_urls,
        cache,
        venv,
        markers,
        &tags,
    )
    .context("Failed to determine installation plan")?;

    // Nothing to do.
    if remote.is_empty() && local.is_empty() && reinstalls.is_empty() {
        let s = if resolution.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Audited {} in {}",
                format!("{} package{}", resolution.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        return Ok(());
    }

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_urls.clone())
        .build();

    // Resolve any registry-based requirements.
    let remote = remote
        .iter()
        .map(|dist| {
            resolution
                .get(&dist.name)
                .cloned()
                .expect("Resolution should contain all packages")
        })
        .collect::<Vec<_>>();

    // Download, build, and unzip any missing distributions.
    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let build_dispatch = BuildDispatch::new(
            client.clone(),
            cache.clone(),
            venv.interpreter().clone(),
            fs::canonicalize(venv.python_executable())?,
            no_build,
            index_urls.clone(),
        );

        let downloader = Downloader::new(cache, &tags, &client, &build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(remote.len() as u64));

        let wheels = downloader
            .download(remote, &OnceMap::default())
            .await
            .context("Failed to download distributions")?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Downloaded {} in {}",
                format!("{} package{}", wheels.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        wheels
    };

    // Remove any existing installations.
    if !reinstalls.is_empty() {
        for dist_info in &reinstalls {
            let summary = puffin_installer::uninstall(dist_info).await?;
            debug!(
                "Uninstalled {} ({} file{}, {} director{})",
                dist_info.name(),
                summary.file_count,
                if summary.file_count == 1 { "" } else { "s" },
                summary.dir_count,
                if summary.dir_count == 1 { "y" } else { "ies" },
            );
        }
    }

    // Install the resolved distributions.
    let wheels = wheels.into_iter().chain(local).collect::<Vec<_>>();
    if !wheels.is_empty() {
        let start = std::time::Instant::now();
        puffin_installer::Installer::new(venv)
            .with_link_mode(link_mode)
            .with_reporter(InstallReporter::from(printer).with_length(wheels.len() as u64))
            .install(&wheels)?;

        let s = if wheels.len() == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Installed {} in {}",
                format!("{} package{}", wheels.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
    }

    for event in reinstalls
        .into_iter()
        .map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
            kind: ChangeEventKind::Removed,
        })
        .chain(wheels.into_iter().map(|distribution| ChangeEvent {
            dist: AnyDist::from(distribution),
            kind: ChangeEventKind::Added,
        }))
        .sorted_unstable_by(|a, b| {
            a.dist
                .name()
                .cmp(b.dist.name())
                .then_with(|| a.kind.cmp(&b.kind))
        })
    {
        match event.kind {
            ChangeEventKind::Added => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "+".green(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.version_or_url().to_string().dimmed()
                )?;
            }
            ChangeEventKind::Removed => {
                writeln!(
                    printer,
                    " {} {}{}",
                    "-".red(),
                    event.dist.name().as_ref().white().bold(),
                    event.dist.version_or_url().to_string().dimmed()
                )?;
            }
        }
    }

    Ok(())
}

/// Validate the installed packages in the virtual environment.
fn validate(resolution: &Resolution, venv: &Virtualenv, mut printer: Printer) -> Result<()> {
    let site_packages = SitePackages::from_executable(venv)?;
    let diagnostics = site_packages.diagnostics()?;
    for diagnostic in diagnostics {
        // Only surface diagnostics that are "relevant" to the current resolution.
        if resolution
            .packages()
            .any(|package| diagnostic.includes(package))
        {
            writeln!(
                printer,
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }
    Ok(())
}
