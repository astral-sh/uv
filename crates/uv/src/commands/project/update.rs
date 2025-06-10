use std::env;
use std::ffi::OsStr;
use std::fmt::Write;
use std::io::ErrorKind;
use std::path::Path;
use std::str::FromStr;

use crate::commands::ExitStatus;
use crate::commands::pip::latest::LatestClient;
use crate::printer::Printer;
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
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{IndexCapabilities, IndexLocations};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{PackageName, Requirement};
use uv_resolver::{PrereleaseMode, RequiresPython};
use uv_warnings::warn_user;
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use walkdir::WalkDir;

/// Upgrade all dependencies in the project requirements (pyproject.toml).
///
/// This doesn't read or modify uv.lock, only constraints like `<1.0` are bumped.
pub(crate) async fn upgrade_project_dependencies(args: UpgradeProjectArgs) -> Result<ExitStatus> {
    let tables: Vec<_> = match args
        .types
        .iter()
        .filter_map(|t| t.clone().into_option())
        .collect::<Vec<_>>()
    {
        tables if !tables.is_empty() => tables,
        _ => DependencyType::iter().to_vec(),
    };
    let tomls = match args
        .recursive
        .then(|| search_pyproject_tomls(Path::new(".")))
    {
        None => vec![".".to_string()],
        Some(Ok(tomls)) => tomls,
        Some(Err(err)) => return Err(err),
    };

    let printer = Printer::Default;
    let info = format!("{}{}", "info".cyan().bold(), ":".bold());

    let mut item_written = false;
    let prepend = |written| {
        if written {
            writeln!(printer.stderr()).expect("");
        }
    };

    let (mut all_found, mut all_bumped) = (0, 0);

    for toml_dir in tomls {
        prepend(item_written);
        item_written = false;
        let pyproject_toml = Path::new(&toml_dir).join("pyproject.toml");
        let content = match fs_err::tokio::read_to_string(pyproject_toml.clone()).await {
            Ok(content) => content,
            Err(err) => {
                if err.kind() == ErrorKind::NotFound {
                    warn_user!("No pyproject.toml found in current directory");
                    return Ok(ExitStatus::Error);
                }
                return Err(err.into());
            }
        };
        let mut toml = match PyProjectTomlMut::from_toml(&content, DependencyTarget::PyProjectToml)
        {
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

        let python = args
            .python
            .clone()
            .and_then(Maybe::into_option)
            .or_else(|| {
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
                .map(DistFilename::into_version)
        };

        let relative = if toml_dir == "." {
            String::new()
        } else {
            format!("{}/", &toml_dir[2..])
        };
        let subpath = format!("{relative}pyproject.toml");
        let (found, upgrades) = toml.upgrade_all_dependencies(&find_latest, &tables).await;
        let bumped = upgrades.len();
        all_found += found;
        all_bumped += bumped;
        if upgrades.is_empty() {
            if args.recursive && bumped == 0 {
                continue; // Skip intermediate messages if nothing was changed
            }
            if found == 0 {
                writeln!(
                    printer.stderr(),
                    "{info} No dependencies found in {subpath}"
                )?;
            } else {
                writeln!(
                    printer.stderr(),
                    "{info} No upgrades found in {subpath}, check manually if not committed yet"
                )?;
            }
            continue;
        }
        let mut table = prettytable::Table::new();
        table.set_format(FormatBuilder::new().column_separator(' ').build());
        let dry_run = format!(
            "upgraded {subpath}{}",
            if args.dry_run { " (dry run)" } else { "" }
        );
        table.add_row(row![r->"#", rb->"name", Fr->"-old", bFg->"+new", "latest", "type", dry_run]); // diff-like
        let remove_spaces = |v: &Requirement| {
            v.clone()
                .version_or_url
                .unwrap()
                .to_string()
                .replace(' ', "")
        };
        upgrades
            .iter()
            .enumerate()
            .for_each(|(i, (_, _dep, old, new, version, upgraded, dependency_type))| {
                let from = remove_spaces(old);
                let to = remove_spaces(new);
                let upordown = if *upgraded { "✅ up" } else { "❌ down" };
                let _type = match dependency_type {
                    DependencyType::Production => "prod".into(),
                    DependencyType::Dev => "dev".into(),
                    DependencyType::Optional(extra) => format!("{extra} [extra]"),
                    DependencyType::Group(group) => format!("{group} [group]"),
                };
                table.add_row(
                    row![r->i + 1, rb->old.name, Fr->from, bFg->to, version.to_string(), _type, upordown],
                );
            });
        table.printstd();
        if !args.dry_run {
            if let Err(err) = fs_err::tokio::write(pyproject_toml, toml.to_string()).await {
                return Err(err.into());
            }
            writeln!(
                printer.stderr(),
                "{info} {subpath} upgraded 🚀 Check manually, update lock + venv {} and run tests",
                "`uv sync -U`".green().bold()
            )?;
        }
        item_written = true;
    }
    if args.recursive && all_bumped == 0 {
        if all_found == 0 {
            writeln!(printer.stderr(), "{info} No dependencies found recursively")?;
        } else {
            writeln!(
                printer.stderr(),
                "{info} No upgrades found recursively, check manually if not committed yet"
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

/// Recursively search for pyproject.toml files.
fn search_pyproject_tomls(root: &Path) -> Result<Vec<String>, anyhow::Error> {
    let metadata = match fs_err::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(vec![]),
        Err(err) => return Err(anyhow::Error::from(err)),
    };

    if !metadata.is_dir() {
        return Ok(vec![]);
    }

    // Hint: Doesn't skip special folders like `build`, `dist` or `target`
    let is_hidden_or_not_pyproject = |path: &Path| {
        path.file_name().and_then(OsStr::to_str).is_some_and(|s| {
            s.starts_with('.') || s.starts_with('_') || s == "target" || path.is_file() && s != "pyproject.toml"
        })
    };

    let mut matches: Vec<_> = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            // TODO(konsti): This should be prettier.
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("walkdir starts with root");
            let hidden = is_hidden_or_not_pyproject(relative);
            !hidden
        })
        .filter_map(|entry| {
            let path = entry.as_ref().unwrap().path();
            if path.is_dir() {
                None
            } else {
                Some(path.parent().unwrap().to_str().unwrap().to_string())
            }
        })
        .collect();
    matches.sort();

    Ok(matches)
}
