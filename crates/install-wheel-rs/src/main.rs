use clap::Parser;
use fs_err::File;
use install_wheel_rs::{install_wheel, CompatibleTags, Error, InstallLocation, WheelFilename};
#[cfg(feature = "rayon")]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::path::PathBuf;
use std::str::FromStr;

/// Low level install CLI, mainly used for testing
#[derive(Parser)]
struct Args {
    wheels: Vec<PathBuf>,
    /// The root of the venv to install in
    #[clap(long, env = "VIRTUAL_ENV")]
    venv: PathBuf,
    /// The major version of the current python interpreter
    #[clap(long)]
    major: u8,
    /// The minor version of the current python interpreter
    #[clap(long)]
    minor: u8,
    /// Compile .py files to .pyc (errors are ignored)
    #[clap(long)]
    compile: bool,
    /// Don't check the hashes in RECORD
    #[clap(long)]
    skip_hashes: bool,
}

fn main() -> Result<(), Error> {
    let args = Args::parse();
    let venv_base = args.venv.canonicalize()?;
    let location = InstallLocation::Venv {
        venv_base,
        python_version: (args.major, args.minor),
    };
    let locked_dir = location.acquire_lock()?;

    let wheels: Vec<(PathBuf, WheelFilename)> = args
        .wheels
        .into_iter()
        .map(|wheel| {
            let filename = wheel
                .file_name()
                .ok_or_else(|| Error::InvalidWheel("Expected a file".to_string()))?
                .to_string_lossy();
            let filename = WheelFilename::from_str(&filename)?;
            let compatible_tags = CompatibleTags::current(location.get_python_version())?;
            filename.compatibility(&compatible_tags)?;
            Ok((wheel, filename))
        })
        .collect::<Result<_, Error>>()?;

    let wheels = {
        #[cfg(feature = "rayon")]
        {
            wheels.into_par_iter()
        }
        #[cfg(not(feature = "rayon"))]
        {
            wheels.into_iter()
        }
    };
    wheels
        .map(|(wheel, filename)| {
            install_wheel(
                &locked_dir,
                File::open(wheel)?,
                filename,
                args.compile,
                !args.skip_hashes,
                &[],
                // Only relevant for monotrail style installation
                "",
                location.get_python(),
            )?;
            Ok(())
        })
        .collect::<Result<_, Error>>()?;
    Ok(())
}
