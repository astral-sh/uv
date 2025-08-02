use anyhow::Result;
use owo_colors::OwoColorize;

use uv_cuda::ManagedCudaInstallations;

pub(crate) fn cuda_dir() -> Result<()> {
    let installations = ManagedCudaInstallations::from_settings(None)?;
    println!("CUDA installations directory: {}", installations.root().display());

    let mut found_any = false;
    for installation in installations.find_all()? {
        found_any = true;
        println!();
        println!("CUDA {}:", installation.version().cyan());
        println!("  Installation path: {}", installation.path().display());
        println!("  Environment file: {}", installation.env_file_path().display());
        println!("  NVCC compiler: {}", installation.nvcc().display());
        println!("  Libraries: {}", installation.lib_dir().display());
        println!("  Headers: {}", installation.include_dir().display());
    }

    if !found_any {
        println!();
        println!("No CUDA installations found.");
        println!("Install a CUDA version with: {}", "uv cuda install <version>".bold());
    }

    Ok(())
}