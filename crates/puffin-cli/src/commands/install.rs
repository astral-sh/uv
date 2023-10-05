use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use futures::future::Either;
use futures::{StreamExt, TryFutureExt};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, Requirement, StringVersion, VersionOrUrl};
use tracing::debug;

use puffin_client::{File, PypiClientBuilder, SimpleJson};
use puffin_requirements::metadata::Metadata21;
use puffin_requirements::package_name::PackageName;
use puffin_requirements::wheel::WheelName;

use crate::commands::ExitStatus;

#[derive(Debug)]
enum Request {
    Package(Requirement),
    Version(Requirement, File),
}

#[derive(Debug)]
enum Response {
    Package(SimpleJson, Requirement),
    Version(Metadata21, Requirement),
}

pub(crate) async fn install(src: &Path) -> Result<ExitStatus> {
    // TODO(charlie): Fetch from the environment.
    let env = MarkerEnvironment {
        implementation_name: String::new(),
        implementation_version: StringVersion::from_str("3.10.0").unwrap(),
        os_name: String::new(),
        platform_machine: String::new(),
        platform_python_implementation: String::new(),
        platform_release: String::new(),
        platform_system: String::new(),
        platform_version: String::new(),
        python_full_version: StringVersion::from_str("3.10.0").unwrap(),
        python_version: StringVersion::from_str("3.10.0").unwrap(),
        sys_platform: String::new(),
    };

    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = puffin_requirements::Requirements::from_str(&requirements_txt)?;

    // Instantiate a client.
    let pypi_client = PypiClientBuilder::default().build();
    let proxy_client = PypiClientBuilder::default().build();

    // A channel to fetch package metadata (e.g., given `flask`, fetch all versions) and version
    // metadata (e.g., given `flask==1.0.0`, fetch the metadata for that version).
    let (package_sink, package_stream) = futures::channel::mpsc::unbounded();

    // Initialize the package stream.
    let mut package_stream = package_stream
        .map(|request: Request| match request {
            Request::Package(requirement) => Either::Left(
                pypi_client
                    .simple(requirement.name.clone())
                    .map_ok(move |metadata| Response::Package(metadata, requirement)),
            ),
            Request::Version(requirement, file) => Either::Right(
                proxy_client
                    .file(file)
                    .map_ok(move |metadata| Response::Version(metadata, requirement)),
            ),
        })
        .buffer_unordered(32)
        .ready_chunks(32);

    // Push all the requirements into the package sink.
    let mut in_flight: HashSet<PackageName> = HashSet::with_capacity(requirements.len());
    for requirement in &*requirements {
        debug!("--> adding root dependency: {}", requirement);
        package_sink.unbounded_send(Request::Package(requirement.clone()))?;
        in_flight.insert(PackageName::normalize(&requirement.name));
    }

    // Resolve the requirements.
    let mut resolution: HashMap<PackageName, Version> = HashMap::with_capacity(requirements.len());

    while let Some(chunk) = package_stream.next().await {
        for result in chunk {
            let result: Response = result?;
            match result {
                Response::Package(metadata, requirement) => {
                    // TODO(charlie): Support URLs. Right now, we treat a URL as an unpinned dependency.
                    let specifiers =
                        requirement
                            .version_or_url
                            .as_ref()
                            .and_then(|version_or_url| match version_or_url {
                                VersionOrUrl::VersionSpecifier(specifiers) => Some(specifiers),
                                VersionOrUrl::Url(_) => None,
                            });

                    // Pick a version that satisfies the requirement.
                    let Some(file) = metadata.files.iter().rev().find(|file| {
                        // We only support wheels for now.
                        let Ok(name) = WheelName::from_str(file.filename.as_str()) else {
                            return false;
                        };

                        specifiers
                            .iter()
                            .all(|specifier| specifier.contains(&name.version))
                    }) else {
                        continue;
                    };

                    package_sink.unbounded_send(Request::Version(requirement, file.clone()))?;
                }
                Response::Version(metadata, requirement) => {
                    debug!(
                        "--> selected version {} for {}",
                        metadata.version, requirement
                    );

                    // Add to the resolved set.
                    let normalized_name = PackageName::normalize(&requirement.name);
                    in_flight.remove(&normalized_name);
                    resolution.insert(normalized_name, metadata.version);

                    // Enqueue its dependencies.
                    for dependency in metadata.requires_dist {
                        if !dependency
                            .evaluate_markers(&env, requirement.extras.clone().unwrap_or_default())
                        {
                            debug!("--> ignoring {dependency} due to environment mismatch");
                            continue;
                        }

                        let normalized_name = PackageName::normalize(&dependency.name);

                        if resolution.contains_key(&normalized_name) {
                            continue;
                        }

                        if !in_flight.insert(normalized_name) {
                            continue;
                        }

                        debug!("--> adding transitive dependency: {}", dependency);

                        package_sink.unbounded_send(Request::Package(dependency))?;
                    }
                }
            }
        }

        if in_flight.is_empty() {
            break;
        }
    }

    for (name, version) in resolution {
        #[allow(clippy::print_stdout)]
        {
            println!("{name}=={version}");
        }
    }

    Ok(ExitStatus::Success)
}
