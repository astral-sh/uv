use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use puffin_client::PypiClientBuilder;

use crate::commands::ExitStatus;

pub(crate) async fn install(src: &Path) -> Result<ExitStatus> {
    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = puffin_requirements::Requirements::from_str(&requirements_txt)?;

    // Instantiate a client.
    let client = PypiClientBuilder::default().build();

    for requirement in requirements.iter() {
        let packument = client.simple(&requirement.name).await?;
        #[allow(clippy::print_stdout)]
        {
            println!("{:#?}", packument);
            println!("{requirement:#?}");
        }
    }

    Ok(ExitStatus::Success)
}
