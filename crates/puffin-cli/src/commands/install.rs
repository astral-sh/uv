use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use futures::{StreamExt, TryFutureExt};

use puffin_client::PypiClientBuilder;
use puffin_requirements::Requirement;

use crate::commands::ExitStatus;

pub(crate) async fn install(src: &Path) -> Result<ExitStatus> {
    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = puffin_requirements::Requirements::from_str(&requirements_txt)?;

    // Instantiate a client.
    let client = PypiClientBuilder::default().build();

    // Fetch metadata in parallel.
    let (package_sink, package_stream) = futures::channel::mpsc::unbounded();

    // Create a stream of futures that fetch metadata for each requirement.
    let mut package_stream = package_stream
        .map(|requirement: Requirement| {
            client
                .simple(requirement.clone().name)
                .map_ok(move |metadata| (metadata, requirement))
        })
        .buffer_unordered(32)
        .ready_chunks(32);

    // Push all the requirements into the sink.
    let mut in_flight = 0;
    for requirement in requirements.iter() {
        package_sink.unbounded_send(requirement.clone())?;
        in_flight += 1;
    }

    while let Some(chunk) = package_stream.next().await {
        in_flight -= chunk.len();
        for result in chunk {
            let (metadata, requirement) = result?;
            #[allow(clippy::print_stdout)]
            {
                println!("{metadata:#?}");
                println!("{requirement:#?}");
            }
        }

        if in_flight == 0 {
            break;
        }
    }

    Ok(ExitStatus::Success)
}
