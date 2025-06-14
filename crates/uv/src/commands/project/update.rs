use crate::commands::ExitStatus;
use crate::commands::pip::latest::LatestClient;
use crate::printer::Printer;
use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use prettytable::format::FormatBuilder;
use prettytable::row;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::min;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt::Write;
use std::io::ErrorKind;
use std::path::Path;
use std::str::FromStr;
use std::sync::LazyLock;
use tokio::sync::Semaphore;
use uv_cache::Cache;
use uv_cli::{Maybe, UpgradeProjectArgs};
use uv_client::{BaseClientBuilder, RegistryClient, RegistryClientBuilder};
use uv_configuration::Concurrency;
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{IndexCapabilities, IndexLocations, RequiresPython};
use uv_pep440::{Version, VersionDigit};
use uv_pep508::{PackageName, Requirement};
use uv_resolver::{ExcludeNewer, ExcludeNewerPackage, PrereleaseMode};
use uv_warnings::warn_user;
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::{DependencyTarget, PackageVersions, PyProjectTomlMut};
use walkdir::WalkDir;

/// Upgrade all dependencies in the project requirements (pyproject.toml).
///
/// This doesn't read or modify uv.lock, only constraints like `<1.0` are bumped.
pub(crate) async fn upgrade_project_dependencies(
    args: UpgradeProjectArgs,
    cache: Cache,
) -> Result<ExitStatus> {
    let tables: Vec<_> = match args
        .types
        .iter()
        .filter_map(|t| t.clone().into_option())
        .collect::<Vec<_>>()
    {
        tables if !tables.is_empty() => tables,
        _ => DependencyType::iter().to_vec(),
    };
    let allow: Vec<_> = match args
        .allow
        .iter()
        .filter_map(|t| t.clone().into_option())
        .collect::<Vec<_>>()
    {
        allow if !allow.is_empty() => allow,
        _ => vec![1, 2, 3, 4],
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
    let uv_sync = format!("{}", "`uv sync -U`".green().bold());

    let capabilities = IndexCapabilities::default();
    let client_builder = BaseClientBuilder::default();

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache)
        .index_locations(IndexLocations::default())
        .build();
    let concurrency = Concurrency::default();

    let mut item_written = false;
    let mut all_found = 0;
    let mut all_bumped = 0;
    let mut files_bumped = 0;
    let mut all_count_skipped = 0;
    let mut all_skipped = VersionDigit::default();

    // 1. args (override) 2. group (tool.uv.dependency-groups) 3. toml (project.requires-python)
    let python_args = args
        .python
        .clone()
        .and_then(Maybe::into_option)
        .and_then(|v| RequiresPython::from_str(&v).ok());
    let mut all_versioned = FxHashMap::default();
    let mut toml_contents = BTreeMap::default();
    for toml_dir in &tomls {
        let pyproject_toml = Path::new(toml_dir).join("pyproject.toml");
        let toml = match read_pyproject_toml(&pyproject_toml).await {
            Ok(value) => value,
            Err(value) => return value,
        };
        let versioned = toml.find_versioned_dependencies();
        if versioned.is_empty() {
            continue; // Skip pyproject.toml without versioned dependencies
        }
        let python_toml = get_requires_python(&toml);
        for (python_group, packages) in versioned {
            let python = python_args.clone().or(python_group).or(python_toml.clone());
            all_versioned
                .entry(python)
                .or_insert_with(FxHashSet::default)
                .extend(packages);
        }
        toml_contents.insert(toml_dir, toml);
    }

    let mut package_versions = PackageVersions::default();
    for (requires_python, packages) in all_versioned {
        let latest_versions = find_latest(
            &client,
            &capabilities,
            requires_python.clone(),
            &packages,
            concurrency.downloads,
        )
        .await;
        // A package can be downloaded multiple times (one time per requires_python)
        for (name, version) in latest_versions {
            package_versions.insert(name.clone(), version, requires_python.clone());
        }
    }

    for (toml_dir, toml) in &mut toml_contents {
        let pyproject_toml = Path::new(*toml_dir).join("pyproject.toml");
        let relative = if *toml_dir == "." {
            String::new()
        } else {
            format!("{}/", &toml_dir[2..])
        };
        let subpath = format!("{relative}pyproject.toml");
        let mut skipped = VersionDigit::default();

        let python_toml = get_requires_python(toml);
        let requires_python = python_args.clone().or(python_toml);
        let (upgrades, found, count_skipped) = toml.upgrade_all_dependencies(
            &package_versions,
            &tables,
            &allow,
            &mut skipped,
            &requires_python,
        );
        all_skipped.add_other(&skipped);
        all_count_skipped += count_skipped;
        let bumped = upgrades.len();
        all_found += found;
        all_bumped += bumped;
        files_bumped += min(bumped, 1);
        if upgrades.is_empty() {
            if args.recursive && bumped == 0 {
                if !skipped.is_empty() {
                    writeln!(
                        printer.stderr(),
                        "{info} Skipped {skipped} ({count_skipped} upgrades) of {found} dependencies in {subpath}"
                    )?;
                }
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
                    "{info} No upgrades found for {found} dependencies in {subpath}, check manually if not committed yet{}",
                    skipped.format(" (skipped ", &format!(" of {count_skipped} upgrades)"))
                )?;
            }
            continue;
        }
        if item_written {
            writeln!(printer.stderr()).expect("");
        }
        item_written = false;
        let mut table = prettytable::Table::new();
        table.set_format(FormatBuilder::new().column_separator(' ').build());
        let dry_run = format!(
            "{} {subpath}",
            if args.dry_run { "dry-run" } else { "upgraded" }
        );
        table.add_row(
            row![r->"#", rb->"name", Fr->"-old", bFg->"+new", "latest", "S", "type", "py", dry_run],
        ); // diff-like
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
            .for_each(|(i, (_, _dep, old, new, version, upgraded, dependency_type, semver_change, python))| {
                let from = remove_spaces(old);
                let to = remove_spaces(new);
                let upordown = if *upgraded { "✅ up" } else { "❌ down" };
                let _type = match dependency_type {
                    DependencyType::Production => "prod".into(),
                    DependencyType::Dev => "dev".into(),
                    DependencyType::Optional(extra) => format!("{extra} [extra]"),
                    DependencyType::Group(group) => format!("{group} [group]"),
                };
                let semver = semver_change.map_or(String::new(), |s| s.to_string());
                let _python = format_requires_python(python.clone());
                table.add_row(
                    row![r->i + 1, rb->old.name, Fr->from, bFg->to, version.to_string(), semver, _type, _python, upordown],
                );
            });
        table.printstd();
        if !args.dry_run {
            if let Err(err) = fs_err::tokio::write(pyproject_toml, toml.to_string()).await {
                return Err(err.into());
            }
            writeln!(
                printer.stderr(),
                "{info} Upgraded {bumped}/{found} in {subpath} 🚀 Check manually, update {uv_sync} and run tests{}",
                skipped.format(" (skipped ", &format!(" of {count_skipped} upgrades)"))
            )?;
        } else if !skipped.is_empty() {
            writeln!(
                printer.stderr(),
                "{info} Skipped {skipped} ({count_skipped} upgrades), upgraded {bumped} of {found} dependencies in {subpath}"
            )?;
        }
        if !item_written {
            item_written = true;
        }
    }
    let files = format!(
        "{} file{}",
        tomls.len(),
        if tomls.len() == 1 { "" } else { "s" }
    );
    if args.recursive && files_bumped != 1 {
        if tomls.is_empty() {
            warn_user!("No pyproject.toml files found recursively");
            return Ok(ExitStatus::Error);
        } else if all_bumped == 0 {
            if all_found == 0 {
                writeln!(
                    printer.stderr(),
                    "{info} No dependencies in {files} found recursively"
                )?;
            } else if !all_skipped.is_empty() {
                writeln!(
                    printer.stderr(),
                    "{info} Skipped {all_skipped} ({all_count_skipped} upgrades), {all_found} dependencies in {files} not upgraded for --allow={}",
                    format_allow(&allow)
                )?;
            } else {
                writeln!(
                    printer.stderr(),
                    "{info} No upgrades in {all_found} dependencies and {files} found, check manually if not committed yet"
                )?;
            }
        } else if !all_skipped.is_empty() {
            writeln!(
                printer.stderr(),
                "{info} Total: Skipped {all_skipped} ({all_count_skipped} upgrades), upgraded {all_bumped} of {all_found} dependencies for --allow={}",
                format_allow(&allow)
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{info} Upgraded {all_bumped}/{all_found} dependencies in {files} 🚀 Check manually, update {uv_sync} and run tests{}",
                all_skipped.format(" (skipped ", &format!(" of {all_count_skipped} upgrades)"))
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

fn get_requires_python(toml: &PyProjectTomlMut) -> Option<RequiresPython> {
    toml.get_requires_python()
        .map(RequiresPython::from_str)
        .transpose()
        .ok()
        .flatten()
}

fn format_requires_python(python: Option<RequiresPython>) -> String {
    match python.map(|r| r.remove_zeroes()) {
        Some(s) if s == ">4" => String::new(), // hide default value
        Some(s) => s,
        _ => String::new(),
    }
}

fn format_allow(allow: &[usize]) -> String {
    allow
        .iter()
        .sorted()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

async fn read_pyproject_toml(
    pyproject_toml: &Path,
) -> Result<PyProjectTomlMut, Result<ExitStatus>> {
    let content = match fs_err::tokio::read_to_string(pyproject_toml.to_path_buf()).await {
        Ok(content) => content,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                warn_user!("No pyproject.toml found in current directory");
                return Err(Ok(ExitStatus::Error));
            }
            return Err(Err(err.into()));
        }
    };
    let toml = match PyProjectTomlMut::from_toml(&content, DependencyTarget::PyProjectToml) {
        Ok(toml) => toml,
        Err(err) => {
            warn_user!("Couldn't read pyproject.toml: {}", err);
            return Err(Ok(ExitStatus::Error));
        }
    };
    Ok(toml)
}

async fn find_latest<'a>(
    client: &RegistryClient,
    capabilities: &IndexCapabilities,
    requires_python: Option<RequiresPython>,
    names: &'a FxHashSet<PackageName>,
    downloads: usize,
) -> FxHashMap<&'a PackageName, Version> {
    static DEFAULT_PYTHON: LazyLock<RequiresPython> =
        LazyLock::new(|| RequiresPython::from_str(">4").ok().unwrap());
    let latest_client = LatestClient {
        client,
        capabilities,
        prerelease: PrereleaseMode::Disallow,
        exclude_newer: &ExcludeNewer::new(None, ExcludeNewerPackage::default()),
        tags: None,
        requires_python: requires_python.as_ref().or_else(|| Some(&*DEFAULT_PYTHON)),
    };

    let download_concurrency = Semaphore::new(downloads);
    let mut fetches = futures::stream::iter(names)
        .map(async |package| {
            let latest = latest_client
                .find_latest(package, None, &download_concurrency)
                .await?;
            Ok::<(&PackageName, Option<DistFilename>), uv_client::Error>((package, latest))
        })
        .buffer_unordered(downloads);

    let mut map = FxHashMap::default();
    while let Ok(Some((package, version))) = fetches.next().await.transpose() {
        if let Some(version) = version.as_ref() {
            map.insert(package, version.clone().into_version());
        }
    }
    map
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
            s.starts_with('.') || s.starts_with('_') || path.is_file() && s != "pyproject.toml"
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
