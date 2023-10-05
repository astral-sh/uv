use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use futures::{StreamExt, TryFutureExt};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, Requirement, StringVersion, VersionOrUrl};
use tracing::trace;

use puffin_client::{PypiClientBuilder, SimpleJson};
use puffin_requirements::package_name::PackageName;
use puffin_requirements::wheel::WheelName;

use crate::commands::ExitStatus;

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
    let client = PypiClientBuilder::default().build();

    // Fetch metadata in parallel.
    let (package_sink, package_stream) = futures::channel::mpsc::unbounded();

    let mut resolution: HashMap<PackageName, Version> = HashMap::with_capacity(requirements.len());

    // Create a stream of futures that fetch metadata for each requirement.
    let mut package_stream = package_stream
        .map(|requirement: Requirement| {
            client
                .simple(requirement.name.clone())
                .map_ok(move |metadata| (metadata, requirement))
        })
        .buffer_unordered(48)
        .ready_chunks(48);

    // Push all the requirements into the sink.
    let mut in_flight: HashSet<PackageName> = HashSet::with_capacity(requirements.len());
    for requirement in requirements.iter() {
        package_sink.unbounded_send(requirement.clone())?;
        in_flight.insert(PackageName::normalize(&requirement.name));
    }

    while let Some(chunk) = package_stream.next().await {
        for result in chunk {
            let (metadata, requirement): (SimpleJson, Requirement) = result?;

            // Remove this requirement from the in-flight set.
            let normalized_name = PackageName::normalize(&requirement.name);
            in_flight.remove(&normalized_name);

            // TODO(charlie): Support URLs. Right now, we treat a URL as an unpinned dependency.
            let specifiers = requirement
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

            // Fetch the metadata for this specific version.
            let metadata = client.file(file).await?;
            trace!(
                "Selecting {version} for {requirement}",
                version = metadata.version,
                requirement = requirement
            );

            // Add to the resolved set.
            let normalized_name = PackageName::normalize(&requirement.name);
            resolution.insert(normalized_name, metadata.version);

            // Enqueue its dependencies.
            for dependency in metadata.requires_dist {
                if !dependency
                    .evaluate_markers(&env, requirement.extras.clone().unwrap_or_default())
                {
                    trace!("Ignoring {dependency} because it doesn't match the environment");
                    continue;
                }

                if dependency
                    .extras
                    .as_ref()
                    .is_some_and(|extras| !extras.is_empty())
                {
                    trace!("Ignoring {dependency} because it has extras");
                    continue;
                }

                let normalized_name = PackageName::normalize(&dependency.name);
                if resolution.contains_key(&normalized_name) {
                    continue;
                }

                if !in_flight.insert(normalized_name) {
                    continue;
                }

                trace!("Enqueueing {dependency}");
                package_sink.unbounded_send(dependency)?;
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
