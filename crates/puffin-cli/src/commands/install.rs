use std::path::Path;
use std::str::FromStr;

use anyhow::Result;

use crate::commands::ExitStatus;

pub(crate) fn install(src: &Path) -> Result<ExitStatus> {
    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = puffin_requirements::Requirements::from_str(&requirements_txt)?;
    for requirement in requirements.iter() {
        #[allow(clippy::print_stdout)]
        {
            println!("{requirement:#?}");
        }
    }

    Ok(ExitStatus::Success)
}
