use std::env;
use std::fmt::Write;
use std::io::ErrorKind;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use owo_colors::OwoColorize;
use prettytable::format::FormatBuilder;
use prettytable::row;
use tokio::sync::Semaphore;

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::{Maybe, UpgradeProjectArgs};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::Concurrency;
use uv_distribution_types::{IndexCapabilities, IndexLocations};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{PackageName, Requirement};
use uv_resolver::{PrereleaseMode, RequiresPython};
use uv_warnings::warn_user;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};

use crate::commands::ExitStatus;
use crate::commands::pip::latest::LatestClient;
use crate::printer::Printer;

/// Upgrade all dependencies in the project requirements (pyproject.toml).
///
/// This doesn't read or modify uv.lock, only constraints like `<1.0` are bumped.
pub(crate) async fn upgrade_project_dependencies(args: UpgradeProjectArgs) -> Result<ExitStatus> {
    let pyproject_toml = Path::new("pyproject.toml");
    let content = match fs_err::tokio::read_to_string(pyproject_toml).await {
        Ok(content) => content,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                warn_user!("No pyproject.toml found in current directory");
                return Ok(ExitStatus::Error);
            }
            return Err(err.into());
        }
    };
    let mut toml = match PyProjectTomlMut::from_toml(&content, DependencyTarget::PyProjectToml) {
        Ok(toml) => toml,
        Err(err) => {
            warn_user!("Couldn't read pyproject.toml: {}", err);
            return Ok(ExitStatus::Error);
        }
    };

    #[allow(deprecated)]
    let cache_dir = env::home_dir().unwrap().join(".cache/uv");
    let cache = Cache::from_settings(false, Some(cache_dir))?.init()?;
    let capabilities = IndexCapabilities::default();
    let client_builder = BaseClientBuilder::new();

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .cache(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
        .index_locations(&IndexLocations::default())
        .build();
    let download_concurrency = Semaphore::new(Concurrency::default().downloads);

    let python = args.python.and_then(Maybe::into_option).or_else(|| {
        toml.get_requires_python()
            .map(std::string::ToString::to_string)
    });
    let version_specifiers = python.and_then(|s| VersionSpecifiers::from_str(&s).ok());
    let requires_python = version_specifiers
        .map(|v| RequiresPython::from_specifiers(&v))
        .unwrap_or_else(|| RequiresPython::greater_than_equal_version(&Version::new([4]))); // allow any by default

    // Initialize the client to fetch the latest version of each package.
    let client = LatestClient {
        client: &client,
        capabilities: &capabilities,
        prerelease: PrereleaseMode::Disallow,
        exclude_newer: None,
        tags: None,
        requires_python: &requires_python,
    };

    let find_latest = async |name: String| {
        client
            .find_latest(
                &PackageName::from_str(name.as_str()).unwrap(),
                None,
                &download_concurrency,
            )
            .await
            .ok()
            .flatten()
            .map(uv_distribution_filename::DistFilename::into_version)
    };

    let printer = Printer::Default;
    let mut stderr = printer.stderr();
    let info = "info".cyan();
    let info = info.bold();

    let (found, all_upgrades) = toml.upgrade_all_dependencies(&find_latest).await;
    if all_upgrades.is_empty() {
        if found == 0 {
            writeln!(
                stderr,
                "{info}{} No dependencies found in pyproject.toml",
                ":".bold()
            )?;
        } else {
            writeln!(
                stderr,
                "{info}{} No upgrades found in pyproject.toml, check manually if not committed yet",
                ":".bold()
            )?;
        }
        return Ok(ExitStatus::Success);
    }
    let mut table = prettytable::Table::new();
    table.set_format(FormatBuilder::new().column_separator(' ').build());
    let dry_run = if args.dry_run {
        "upgraded (dry run)"
    } else {
        "upgraded"
    };
    table.add_row(row![r->"#", rb->"name", Fr->"-old", bFg->"+new", "latest", dry_run]); // diff-like
    let remove_spaces = |v: &Requirement| {
        v.clone()
            .version_or_url
            .unwrap()
            .to_string()
            .replace(' ', "")
    };
    all_upgrades
        .iter()
        .enumerate()
        .for_each(|(i, (_, _dep, old, new, version, upgraded))| {
            let from = remove_spaces(old);
            let to = remove_spaces(new);
            let upordown = if *upgraded { "✅ up" } else { "❌ down" };
            table.add_row(
                row![r->i + 1, rb->old.name, Fr->from, bFg->to, version.to_string(), upordown],
            );
        });
    table.printstd();
    if !args.dry_run {
        if let Err(err) = fs_err::tokio::write(pyproject_toml, toml.to_string()).await {
            return Err(err.into());
        }
        writeln!(
            stderr,
            "{info}{} pyproject.toml upgraded 🚀 Check manually then update {} and run tests",
            ":".bold(),
            "`uv lock -U && uv sync`".green().bold()
        )?;
    }

    Ok(ExitStatus::Success)
}
